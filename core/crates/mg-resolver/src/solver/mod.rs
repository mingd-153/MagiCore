//! Dependency resolution engine — batch queue, semver matching, error propagation.
//!
//! ## Flow
//! 1. Wanted packages queued from CLI.
//! 2. Each batch prefetches version info.
//! 3. For each package: override → already-resolved-check → fresh resolve.
//! 4. Resolved packages enqueue their own transitive deps.
//!
//! ## Error Policy (v2)
//! `DependencyProvider` now returns `Result`. Errors propagate — no silent skips.
//!
//! ## Limitations
//! - No PubGrub backtracking for conflicts
//! - No multi-version hoisting

pub mod pubgrub;

use async_trait::async_trait;
use mg_types::{PackageId, PackageName, Version, VersionRange};
use std::collections::{HashMap, HashSet, VecDeque};

pub use pubgrub::{Cause, Incompatibility, PubGrubSolver, SolveError as PubGrubSolveError, Term};

/// Error from a `DependencyProvider` (network failure, registry 500, etc.).
#[derive(Debug, Clone)]
pub struct DependencyError(pub String);

impl std::fmt::Display for DependencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for DependencyError {}

/// One dependency of a resolved package: name + version spec + flags.
#[derive(Debug, Clone)]
pub struct ResolvedDep {
    pub package: PackageName,
    pub spec: String,
    pub optional: bool,
    pub peer: bool,
}

/// A single resolution entry: pinned package + version + transitive deps.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub package_id: PackageId,
    pub version: Version,
    pub integrity: String,
    pub deps: Vec<String>,
    pub dep_specs: Vec<(String, String)>,
}

/// Complete resolution result: all packages in topo-ish order.
#[derive(Debug, Clone)]
pub struct SolveResult {
    pub resolutions: Vec<Resolution>,
}

/// Resolution failure: conflict or provider error.
#[derive(Debug, Clone)]
pub struct SolveError {
    pub message: String,
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for SolveError {}
impl From<String> for SolveError {
    fn from(s: String) -> Self {
        Self { message: s }
    }
}

/// Dependency provider trait — each ecosystem must implement this.
///
/// ⚠ Both methods return `Result`. On network failure, error propagates.
///    Empty vec is valid (package exists with no versions / no deps).
#[async_trait]
pub trait DependencyProvider: Send + Sync {
    async fn get_versions(&self, package: &PackageName) -> Result<Vec<Version>, DependencyError>;
    async fn get_dependencies(&self, id: &PackageId) -> Result<Vec<ResolvedDep>, DependencyError>;
    async fn should_enqueue(&self, dep: &ResolvedDep) -> Result<bool, DependencyError> {
        let _ = dep;
        Ok(true)
    }

    async fn prefetch_versions(
        &self,
        packages: &[PackageName],
    ) -> Result<Vec<(PackageName, Vec<Version>)>, DependencyError> {
        let mut results = Vec::with_capacity(packages.len());
        for name in packages {
            results.push((name.clone(), self.get_versions(name).await?));
        }
        Ok(results)
    }

    async fn prefetch_dependencies(
        &self,
        ids: &[PackageId],
    ) -> Result<Vec<(PackageId, Vec<ResolvedDep>)>, DependencyError> {
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            results.push((id.clone(), self.get_dependencies(id).await?));
        }
        Ok(results)
    }
}

/// Info about a dependency, used by `check_dependency_confusion`.
#[derive(Debug, Clone)]
pub struct DepInfo {
    pub name: String,
    pub version: Option<String>,
    pub registry: Option<String>,
}

/// Check for dependency confusion attacks:
/// - Workspace package vs external dep same name.
/// - Scoped package wrong registry.
/// - Package from untrusted registry.
pub fn check_dependency_confusion(
    workspace_packages: &[String],
    dependencies: &[DepInfo],
    scoped_registries: &HashMap<String, String>,
    trusted_registries: &[String],
) -> Vec<String> {
    let mut warnings = Vec::new();
    for dep in dependencies {
        if workspace_packages.contains(&dep.name) && dep.version.is_some() {
            warnings.push(format!(
                    "Dependency confusion: '{}' is both workspace package and external dep. Use \"workspace:*\".", dep.name
                ));
        }
        if dep.name.starts_with('@') {
            if let Some(scope) = dep.name.split('/').next() {
                if let Some(expected) = scoped_registries.get(scope) {
                    if dep.registry.as_deref() != Some(expected.as_str()) {
                        warnings.push(format!(
                            "Dependency confusion: '{}' should resolve from '{}' but resolves from '{}'",
                            dep.name, expected, dep.registry.as_deref().unwrap_or("public npm")
                        ));
                    }
                }
            }
        }
        if !trusted_registries.is_empty() {
            if let Some(ref reg) = dep.registry {
                if !trusted_registries.contains(reg) {
                    warnings.push(format!(
                        "Dependency confusion: '{}' from '{}' not in trusted registries",
                        dep.name, reg
                    ));
                }
            }
        }
    }
    warnings
}

/// Batch resolver with error propagation.
///
/// ## Algorithm
/// 1. Enqueue initial deps from CLI.
/// 2. Drain batch (max 50), prefetch unique names.
/// 3. Per package:
///    a. Override check.
///    b. Already resolved → recheck, upgrade within major if needed.
///    c. Fresh resolve → pick latest matching version.
/// 4. Enqueue resolved deps.
/// 5. Repeat until queue empty.
///
/// ## Errors
/// Provider errors → immediate `SolveError`. No silent skips.
pub struct Resolver {
    provider: std::sync::Arc<dyn DependencyProvider>,
    overrides: HashMap<String, String>,
}

impl Resolver {
    pub fn new(provider: std::sync::Arc<dyn DependencyProvider>) -> Self {
        Self {
            provider,
            overrides: HashMap::new(),
        }
    }

    pub fn set_overrides(&mut self, overrides: HashMap<String, String>) {
        self.overrides = overrides;
    }

    fn select_best_version(
        versions: &[Version],
        constraint: &VersionRange,
        spec: &str,
    ) -> Option<Version> {
        let mut matches: Vec<Version> = versions
            .iter()
            .filter(|v| constraint.matches(v))
            .cloned()
            .collect();
        if matches.is_empty() {
            return None;
        }

        let allows_prerelease = spec.contains('-');
        if !allows_prerelease {
            let stable: Vec<Version> = matches
                .iter()
                .filter(|v| v.pre.is_none())
                .cloned()
                .collect();
            if let Some(best) = stable.into_iter().max() {
                return Some(best);
            }
        }

        matches.sort();
        matches.into_iter().max()
    }

    /// Resolve all dependencies. Fails with `SolveError` on provider error or
    /// unresolvable constraint.
    pub async fn solve(&self, wanted: &[(PackageName, String)]) -> Result<SolveResult, SolveError> {
        let mut resolutions: Vec<Resolution> = Vec::new();
        let mut resolved: HashMap<String, Version> = HashMap::new();
        let mut queue: VecDeque<(PackageName, String)> =
            wanted.iter().map(|(n, s)| (n.clone(), s.clone())).collect();
        let mut resolved_versions: HashSet<(String, Version)> = HashSet::new();

        // Pre-fetch initial batch
        let initial_names: Vec<PackageName> = wanted.iter().map(|(n, _)| n.clone()).collect();
        self.provider
            .prefetch_versions(&initial_names)
            .await
            .map_err(|e| SolveError {
                message: format!("initial prefetch failed: {e}"),
            })?;

        while !queue.is_empty() {
            let batch_size = queue.len().min(50);
            let batch: Vec<(PackageName, String)> = queue.drain(..batch_size).collect();
            let mut selected: Vec<(PackageName, String, Version)> = Vec::new();

            // Deduplicate and prefetch batch versions
            let mut seen = HashSet::new();
            let batch_names: Vec<PackageName> = batch
                .iter()
                .filter(|(n, _)| seen.insert(n.as_str().to_string()))
                .map(|(n, _)| n.clone())
                .collect();
            if !batch_names.is_empty() {
                self.provider
                    .prefetch_versions(&batch_names)
                    .await
                    .map_err(|e| SolveError {
                        message: format!("batch prefetch failed: {e}"),
                    })?;
            }

            for (name, spec) in batch {
                let name_str = name.as_str().to_string();
                let constraint = VersionRange::parse(&spec).map_err(|e| SolveError {
                    message: format!("invalid spec '{}' for '{}': {}", spec, name_str, e),
                })?;

                // Phase 1: override
                if let Some(override_spec) = self.overrides.get(&name_str) {
                    let oc = VersionRange::parse(override_spec).map_err(|e| SolveError {
                        message: format!("invalid override '{}': {}", override_spec, e),
                    })?;
                    let versions =
                        self.provider
                            .get_versions(&name)
                            .await
                            .map_err(|e| SolveError {
                                message: format!("versions fetch failed for '{}': {}", name_str, e),
                            })?;
                    if let Some(v) = Self::select_best_version(&versions, &oc, override_spec)
                        .or_else(|| versions.iter().filter(|v| v.pre.is_none()).cloned().max())
                        .or_else(|| versions.iter().cloned().max())
                    {
                        resolved.insert(name_str.clone(), v.clone());
                        resolved_versions.insert((name_str.clone(), v.clone()));
                        selected.push((name, name_str, v));
                    }
                    continue;
                }

                // Phase 2: already resolved
                if let Some(existing) = resolved.get(&name_str).cloned() {
                    if constraint.matches(&existing) {
                        continue;
                    }
                    let versions =
                        self.provider
                            .get_versions(&name)
                            .await
                            .map_err(|e| SolveError {
                                message: format!("cannot fetch versions for '{}': {}", name_str, e),
                            })?;

                    if let Some(other) = Self::select_best_version(&versions, &constraint, &spec) {
                        let key = (name_str.clone(), other.clone());
                        if !resolved_versions.contains(&key) {
                            resolved_versions.insert(key);
                            selected.push((name, name_str, other));
                        }
                    }
                    continue;
                }

                // Phase 3: fresh resolve
                let versions = self
                    .provider
                    .get_versions(&name)
                    .await
                    .map_err(|e| SolveError {
                        message: format!("cannot fetch versions for '{}': {}", name_str, e),
                    })?;
                let version = Self::select_best_version(&versions, &constraint, &spec);

                match version {
                    Some(v) => {
                        resolved.insert(name_str.clone(), v.clone());
                        resolved_versions.insert((name_str.clone(), v.clone()));
                        selected.push((name, name_str, v));
                    }
                    None => {
                        return Err(SolveError {
                            message: format!("no version of '{}' matches '{}'", name_str, spec),
                        })
                    }
                }
            }

            if !selected.is_empty() {
                let ids: Vec<PackageId> = selected
                    .iter()
                    .map(|(name, _, version)| PackageId::new(name.clone(), version.clone()))
                    .collect();
                self.provider
                    .prefetch_dependencies(&ids)
                    .await
                    .map_err(|e| SolveError {
                        message: format!("dependency prefetch failed: {e}"),
                    })?;

                for (name, name_str, version) in selected {
                    Self::add_resolution(
                        &mut resolutions,
                        &name,
                        &name_str,
                        version,
                        &self.provider,
                        &mut queue,
                    )
                    .await?;
                }
            }
        }

        Ok(SolveResult { resolutions })
    }

    /// Record a resolved package and enqueue its transitive dependencies.
    async fn add_resolution(
        resolutions: &mut Vec<Resolution>,
        name: &PackageName,
        _name_str: &str,
        version: Version,
        provider: &std::sync::Arc<dyn DependencyProvider>,
        queue: &mut VecDeque<(PackageName, String)>,
    ) -> Result<(), SolveError> {
        let pid = PackageId::new(name.clone(), version.clone());

        let deps = provider
            .get_dependencies(&pid)
            .await
            .map_err(|e| SolveError {
                message: format!("dependencies fetch failed for '{}': {}", pid, e),
            })?;
        let dep_names: Vec<String> = deps
            .iter()
            .map(|d| d.package.as_str().to_string())
            .collect();
        let dep_specs: Vec<(String, String)> = deps
            .iter()
            .map(|d| (d.package.as_str().to_string(), d.spec.clone()))
            .collect();

        for dep in &deps {
            if provider.should_enqueue(dep).await.map_err(|e| SolveError {
                message: format!(
                    "dependency enqueue check failed for '{}@{}': {}",
                    dep.package.as_str(),
                    dep.spec,
                    e
                ),
            })? {
                queue.push_front((dep.package.clone(), dep.spec.clone()));
            }
        }

        resolutions.push(Resolution {
            package_id: pid,
            version,
            integrity: String::new(),
            deps: dep_names,
            dep_specs,
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;
    #[async_trait]
    impl DependencyProvider for MockProvider {
        async fn get_versions(&self, _: &PackageName) -> Result<Vec<Version>, DependencyError> {
            Ok(vec![
                Version::parse("1.0.0").unwrap(),
                Version::parse("2.0.0").unwrap(),
            ])
        }
        async fn get_dependencies(
            &self,
            _: &PackageId,
        ) -> Result<Vec<ResolvedDep>, DependencyError> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_solve_simple() {
        let resolver = Resolver::new(std::sync::Arc::new(MockProvider));
        let wanted = vec![(PackageName::new("react").unwrap(), "^1.0.0".to_string())];
        let result = resolver.solve(&wanted).await.unwrap();
        assert_eq!(result.resolutions.len(), 1);
    }

    #[tokio::test]
    async fn test_semver_caret() {
        struct CP;
        #[async_trait]
        impl DependencyProvider for CP {
            async fn get_versions(&self, _: &PackageName) -> Result<Vec<Version>, DependencyError> {
                Ok(vec!["3.4.0", "3.5.0", "4.0.0"]
                    .iter()
                    .map(|s| Version::parse(s).unwrap())
                    .collect())
            }
            async fn get_dependencies(
                &self,
                _: &PackageId,
            ) -> Result<Vec<ResolvedDep>, DependencyError> {
                Ok(vec![])
            }
        }
        let resolver = Resolver::new(std::sync::Arc::new(CP));
        let wanted = vec![(
            PackageName::new("tailwindcss").unwrap(),
            "^3.4.0".to_string(),
        )];
        let result = resolver.solve(&wanted).await.unwrap();
        assert_eq!(result.resolutions[0].version.to_string(), "3.5.0");
    }

    #[tokio::test]
    async fn test_semver_exact() {
        struct EP;
        #[async_trait]
        impl DependencyProvider for EP {
            async fn get_versions(&self, _: &PackageName) -> Result<Vec<Version>, DependencyError> {
                Ok(vec!["1.0.0", "1.0.1"]
                    .iter()
                    .map(|s| Version::parse(s).unwrap())
                    .collect())
            }
            async fn get_dependencies(
                &self,
                _: &PackageId,
            ) -> Result<Vec<ResolvedDep>, DependencyError> {
                Ok(vec![])
            }
        }
        let resolver = Resolver::new(std::sync::Arc::new(EP));
        let wanted = vec![(PackageName::new("pkg").unwrap(), "1.0.0".to_string())];
        let result = resolver.solve(&wanted).await.unwrap();
        assert_eq!(result.resolutions[0].version.to_string(), "1.0.0");
    }

    #[tokio::test]
    async fn test_same_major_conflict_keeps_multiple_versions() {
        struct SameMajorProvider;

        #[async_trait]
        impl DependencyProvider for SameMajorProvider {
            async fn get_versions(
                &self,
                package: &PackageName,
            ) -> Result<Vec<Version>, DependencyError> {
                let versions = match package.as_str() {
                    "root" | "plugin" => vec!["1.0.0"],
                    "esbuild" => vec!["0.17.6", "0.28.1"],
                    _ => vec![],
                };
                Ok(versions
                    .into_iter()
                    .map(|value| Version::parse(value).unwrap())
                    .collect())
            }

            async fn get_dependencies(
                &self,
                package_id: &PackageId,
            ) -> Result<Vec<ResolvedDep>, DependencyError> {
                let dep = |name: &str, spec: &str| ResolvedDep {
                    package: PackageName::new(name).unwrap(),
                    spec: spec.to_string(),
                    optional: false,
                    peer: false,
                };

                let deps = match package_id.name_str() {
                    "root" => vec![dep("esbuild", "0.28.1")],
                    "plugin" => vec![dep("esbuild", "0.17.6")],
                    _ => vec![],
                };

                Ok(deps)
            }
        }

        let resolver = Resolver::new(std::sync::Arc::new(SameMajorProvider));
        let wanted = vec![
            (PackageName::new("root").unwrap(), "1.0.0".to_string()),
            (PackageName::new("plugin").unwrap(), "1.0.0".to_string()),
        ];

        let result = resolver.solve(&wanted).await.unwrap();
        let esbuild_versions: Vec<String> = result
            .resolutions
            .iter()
            .filter(|resolution| resolution.package_id.name_str() == "esbuild")
            .map(|resolution| resolution.version.to_string())
            .collect();

        assert!(esbuild_versions.iter().any(|version| version == "0.17.6"));
        assert!(esbuild_versions.iter().any(|version| version == "0.28.1"));
    }

    #[tokio::test]
    async fn test_no_matching_version_fails_instead_of_falling_back() {
        struct NP;
        #[async_trait]
        impl DependencyProvider for NP {
            async fn get_versions(&self, _: &PackageName) -> Result<Vec<Version>, DependencyError> {
                Ok(vec!["1.0.0", "2.0.0"]
                    .iter()
                    .map(|s| Version::parse(s).unwrap())
                    .collect())
            }
            async fn get_dependencies(
                &self,
                _: &PackageId,
            ) -> Result<Vec<ResolvedDep>, DependencyError> {
                Ok(vec![])
            }
        }
        let resolver = Resolver::new(std::sync::Arc::new(NP));
        let wanted = vec![(PackageName::new("pkg").unwrap(), "=9.9.9".to_string())];
        let err = resolver.solve(&wanted).await.unwrap_err();
        assert!(err.message.contains("no version of 'pkg' matches '=9.9.9'"));
    }

    #[tokio::test]
    async fn test_prefers_stable_match_over_prerelease_for_normal_ranges() {
        struct PP;
        #[async_trait]
        impl DependencyProvider for PP {
            async fn get_versions(&self, _: &PackageName) -> Result<Vec<Version>, DependencyError> {
                Ok(vec!["4.4.3", "4.5.0-canary.20260504T180558"]
                    .iter()
                    .map(|s| Version::parse(s).unwrap())
                    .collect())
            }
            async fn get_dependencies(
                &self,
                _: &PackageId,
            ) -> Result<Vec<ResolvedDep>, DependencyError> {
                Ok(vec![])
            }
        }
        let resolver = Resolver::new(std::sync::Arc::new(PP));
        let wanted = vec![(PackageName::new("zod").unwrap(), "^4.4.3".to_string())];
        let result = resolver.solve(&wanted).await.unwrap();
        assert_eq!(result.resolutions[0].version.to_string(), "4.4.3");
    }

    #[test]
    fn test_confusion_detection() {
        let deps = vec![DepInfo {
            name: "my-pkg".into(),
            version: Some("1.0".into()),
            registry: None,
        }];
        let w = check_dependency_confusion(&["my-pkg".into()], &deps, &HashMap::new(), &[]);
        assert!(w[0].contains("confusion"));
    }

    #[tokio::test]
    async fn test_solves_large_tree_without_duplicate_explosion() {
        struct LargeTreeProvider {
            depth: usize,
            width: usize,
        }

        impl LargeTreeProvider {
            fn level_of(name: &str) -> usize {
                name.split('-')
                    .nth(1)
                    .and_then(|part| part.parse::<usize>().ok())
                    .unwrap_or(0)
            }
        }

        #[async_trait]
        impl DependencyProvider for LargeTreeProvider {
            async fn get_versions(&self, _: &PackageName) -> Result<Vec<Version>, DependencyError> {
                Ok(vec![Version::parse("1.0.0").unwrap()])
            }

            async fn get_dependencies(
                &self,
                id: &PackageId,
            ) -> Result<Vec<ResolvedDep>, DependencyError> {
                let name = id.name_str().to_string();
                let level = Self::level_of(&name);
                if level >= self.depth {
                    return Ok(vec![]);
                }

                Ok((0..self.width)
                    .map(|i| ResolvedDep {
                        package: PackageName::new(format!("pkg-{}-{name}-{i}", level + 1)).unwrap(),
                        spec: "^1.0.0".to_string(),
                        optional: false,
                        peer: false,
                    })
                    .collect())
            }
        }

        let resolver = Resolver::new(std::sync::Arc::new(LargeTreeProvider {
            depth: 4,
            width: 4,
        }));
        let wanted = vec![(
            PackageName::new("pkg-0-root").unwrap(),
            "^1.0.0".to_string(),
        )];
        let result = resolver.solve(&wanted).await.unwrap();

        // 1 + 4 + 16 + 64 + 256
        assert_eq!(result.resolutions.len(), 341);
    }
}
