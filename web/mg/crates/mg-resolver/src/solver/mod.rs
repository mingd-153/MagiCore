//! PubGrub-based dependency resolver

pub mod pubgrub;

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::path::PathBuf;

use mg_core::{Catalog, PackageId, PackageName, Version};
use async_trait::async_trait;

/// Information about a dependency for confusion checking.
#[derive(Debug, Clone)]
pub struct DepInfo {
    pub name: String,
    pub version: Option<String>,
    pub registry: Option<String>,
}

/// Check for dependency confusion vulnerabilities.
///
/// Detects when:
/// 1. A public package shadows an internal workspace package
/// 2. A scoped package resolves from the wrong registry
/// 3. A package resolves from an untrusted registry
pub fn check_dependency_confusion(
    workspace_packages: &[String],
    dependencies: &[DepInfo],
    scoped_registries: &HashMap<String, String>,
    trusted_registries: &[String],
) -> Vec<String> {
    let mut warnings = Vec::new();

    for dep in dependencies {
        let name = &dep.name;

        if workspace_packages.contains(name) {
            if let Some(version) = &dep.version {
                warnings.push(format!(
                    "Dependency confusion: '{}' is both a workspace package and an external dependency (version {}). \
                     Use \"workspace:*\" for workspace packages.",
                    name, version
                ));
            }
        }

        if name.starts_with('@') {
            if let Some(scope) = name.split('/').next() {
                if let Some(expected_registry) = scoped_registries.get(scope) {
                    if dep.registry.as_deref() != Some(expected_registry.as_str()) {
                        warnings.push(format!(
                            "Potential dependency confusion: '{}' should resolve from '{}' but is configured for '{}'",
                            name,
                            expected_registry,
                            dep.registry.as_deref().unwrap_or("public npm")
                        ));
                    }
                }
            }
        }

        if !trusted_registries.is_empty() {
            if let Some(ref registry) = dep.registry {
                if !trusted_registries.contains(registry) {
                    warnings.push(format!(
                        "Potential dependency confusion: '{}' resolves from '{}' which is not in the trusted registries list",
                        name, registry
                    ));
                }
            }
        }
    }

    warnings
}

#[derive(Clone)]
pub struct Resolver {
    provider: std::sync::Arc<dyn DependencyProvider>,
    catalogs: HashMap<String, Catalog>,
    overrides: HashMap<String, String>,
    workspace: Option<mg_workspace::Workspace>,
    lockfile_packages: Option<HashMap<String, String>>,
}

#[async_trait]
pub trait DependencyProvider: Send + Sync {
    async fn get_versions(&self, package: &PackageName) -> Vec<Version>;
    async fn get_dependencies(&self, package_id: &PackageId) -> Vec<ResolvedDep>;

    /// Batch prefetch metadata for multiple packages.
    /// Default implementation calls get_versions for each package sequentially.
    /// Implementations should override this to use concurrent HTTP fetches.
    async fn prefetch_versions(&self, packages: &[PackageName]) -> Vec<(PackageName, Vec<Version>)> {
        let mut results = Vec::with_capacity(packages.len());
        for name in packages {
            let versions = self.get_versions(name).await;
            results.push((name.clone(), versions));
        }
        results
    }

    /// Batch prefetch dependencies for multiple packages.
    async fn prefetch_dependencies(&self, package_ids: &[PackageId]) -> Vec<(PackageId, Vec<ResolvedDep>)> {
        let mut results = Vec::with_capacity(package_ids.len());
        for id in package_ids {
            let deps = self.get_dependencies(id).await;
            results.push((id.clone(), deps));
        }
        results
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedDep {
    pub package: PackageName,
    pub spec: String,
    pub optional: bool,
    pub peer: bool,
}

#[derive(Debug, Clone)]
pub struct Resolution {
    pub package_id: PackageId,
    pub version: Version,
    pub integrity: String,
    pub deps: Vec<String>,
    /// Dep specs: (name, version_spec) for each dependency
    pub dep_specs: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct SolveResult {
    pub resolutions: Vec<Resolution>,
}

/// Information about a workspace member for resolver integration.
#[derive(Debug, Clone)]
pub struct WorkspaceMemberInfo {
    pub name: String,
    pub path: PathBuf,
    pub version: String,
}

/// Workspace information for workspace-aware resolution.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceInfo {
    pub members: HashMap<String, WorkspaceMemberInfo>,
}

impl WorkspaceInfo {
    pub fn new(members: Vec<WorkspaceMemberInfo>) -> Self {
        Self {
            members: members.into_iter().map(|m| (m.name.clone(), m)).collect(),
        }
    }

    pub fn find(&self, name: &str) -> Option<&WorkspaceMemberInfo> {
        self.members.get(name)
    }

    pub fn is_workspace_package(&self, name: &str) -> bool {
        self.members.contains_key(name)
    }
}

impl Resolver {
    pub fn new(provider: std::sync::Arc<dyn DependencyProvider>) -> Self {
        Self {
            provider,
            catalogs: HashMap::new(),
            overrides: HashMap::new(),
            workspace: None,
            lockfile_packages: None,
        }
    }

    /// Sets the catalogs for version pinning.
    pub fn set_catalogs(&mut self, catalogs: HashMap<String, Catalog>) {
        self.catalogs = catalogs;
    }

    /// Sets the dependency overrides.
    pub fn set_overrides(&mut self, overrides: HashMap<String, String>) {
        self.overrides = overrides;
    }

    /// Stores a workspace reference for workspace-aware resolution.
    pub fn set_workspace(&mut self, workspace: mg_workspace::Workspace) {
        self.workspace = Some(workspace);
    }

    /// Sets pre-resolved packages from lockfile for fast resolution.
    pub fn set_lockfile_packages(&mut self, packages: HashMap<String, String>) {
        self.lockfile_packages = Some(packages);
    }

    /// Resolves a package version from a named catalog.
    pub fn resolve_catalog(&self, name: &str, catalog_name: &str) -> Result<Version, SolveError> {
        let catalog = self.catalogs.get(catalog_name).ok_or_else(|| SolveError {
            message: format!("catalog '{}' not found", catalog_name),
        })?;
        let version_str = catalog.get(name).ok_or_else(|| SolveError {
            message: format!("package '{}' not found in catalog '{}'", name, catalog_name),
        })?;
        Version::parse(version_str).map_err(|e| SolveError {
            message: format!("invalid version '{}' in catalog: {}", version_str, e),
        })
    }

    /// Resolves dependencies while considering workspace packages.
    /// If a wanted package matches a workspace member, it uses the Workspace protocol.
    pub async fn resolve_with_workspace(
        &self,
        wanted: &[(PackageName, String)],
        workspace: &WorkspaceInfo,
    ) -> Result<SolveResult, SolveError> {
        let mut resolutions = Vec::new();
        let mut remaining = Vec::new();

        for (name, spec) in wanted {
            if let Some(member) = workspace.find(name.as_str()) {
                let version = Version::parse(&member.version).map_err(|e| SolveError {
                    message: format!("invalid workspace version: {}", e),
                })?;
                resolutions.push(Resolution {
                    package_id: PackageId::new(name.clone(), version.clone()),
                    version,
                    integrity: "workspace".to_string(),
                    deps: Vec::new(),
                    dep_specs: Vec::new(),
                });
            } else {
                remaining.push((name.clone(), spec.clone()));
            }
        }

        if !remaining.is_empty() {
            let result = self.solve(&remaining).await?;
            resolutions.extend(result.resolutions);
        }

        Ok(SolveResult { resolutions })
    }

    pub async fn solve(&self, wanted: &[(PackageName, String)]) -> Result<SolveResult, SolveError> {
        use mg_core::VersionRange;

        let mut resolutions: Vec<Resolution> = Vec::new();
        let mut resolved_versions: HashMap<String, Version> = HashMap::new();
        let mut resolved_majors: HashSet<(String, u64)> = HashSet::new();
        let mut queue: VecDeque<(PackageName, String)> = wanted.iter().map(|(n, s)| (n.clone(), s.clone())).collect();

        while !queue.is_empty() {
            // Collect a batch from the front of the queue for parallel prefetch
            let batch_size = queue.len().min(50);
            let batch: Vec<(PackageName, String)> = queue.drain(..batch_size).collect();

            // Prefetch unique package names in the batch
            let mut seen = HashSet::new();
            let batch_names: Vec<PackageName> = batch.iter()
                .filter(|(n, _)| seen.insert(n.as_str().to_string()))
                .map(|(n, _)| n.clone())
                .collect();
            if !batch_names.is_empty() {
                self.provider.prefetch_versions(&batch_names).await;
            }

            // Process each package in the batch
            for (name, spec) in batch {
                let name_str = name.as_str().to_string();

                // If already resolved, check if existing version satisfies this new constraint.
                // If major differs, add a NEW resolution (pnpm-style multi-version).
                if let Some(existing_version) = resolved_versions.get(&name_str) {
                if let Ok(ref c) = VersionRange::parse(&spec) {
                    if c.contains(existing_version) {
                        continue;
                    }
                    // Try to find a version within the same major first
                    let all_versions = self.provider.get_versions(&name).await;
                    let same_major = all_versions.iter()
                        .filter(|v| c.contains(v) && v.major == existing_version.major)
                        .max()
                        .cloned();
                    if let Some(ref new_version) = same_major {
                        if new_version > existing_version {
                            if let Some(res) = resolutions.iter_mut().find(|r| {
                                r.package_id.name().as_str() == name_str
                                    && r.version.major == existing_version.major
                            }) {
                                res.version = new_version.clone();
                                res.package_id = PackageId::new(name.clone(), new_version.clone());
                            }
                            resolved_versions.insert(name_str.clone(), new_version.clone());
                        }
                        continue;
                    }
                    // No same-major version satisfies the new constraint.
                    // Try a different major version and add as NEW resolution.
                    let best_other = all_versions.iter()
                        .filter(|v| c.contains(v))
                        .max()
                        .cloned();
                    if let Some(ref other_version) = best_other {
                        let major_key = (name_str.clone(), other_version.major);
                        if !resolved_majors.contains(&major_key) {
                            resolved_majors.insert(major_key);
                            let other_id = PackageId::new(name.clone(), other_version.clone());
                            let deps = self.provider.get_dependencies(&other_id).await;
                            let dep_names: Vec<String> = deps.iter().map(|d| d.package.as_str().to_string()).collect();
                            let dep_specs: Vec<(String, String)> = deps.iter()
                                .map(|d| (d.package.as_str().to_string(), d.spec.clone()))
                                .collect();
                            for dep in &deps {
                                queue.push_front((dep.package.clone(), dep.spec.clone()));
                            }
                            resolutions.push(Resolution {
                                package_id: other_id,
                                version: other_version.clone(),
                                integrity: String::new(),
                                deps: dep_names,
                                dep_specs,
                            });
                        }
                    }
                }
                continue;
            }

            // Check lockfile first for pre-resolved packages
            if let Some(ref lockfile_pkgs) = self.lockfile_packages {
                if let Some(lockfile_version) = lockfile_pkgs.get(name.as_str()) {
                    if let Ok(version) = Version::parse(lockfile_version) {
                        resolved_versions.insert(name_str.clone(), version.clone());
                        resolved_majors.insert((name_str, version.major));
                        resolutions.push(Resolution {
                            package_id: PackageId::new(name.clone(), version.clone()),
                            version,
                            integrity: String::new(),
                            deps: Vec::new(),
                            dep_specs: Vec::new(),
                        });
                        continue;
                    }
                }
            }

            let constraint = VersionRange::parse(&spec).ok();
            let all_versions = self.provider.get_versions(&name).await;

            let matched_version = if let Some(ref c) = constraint {
                all_versions.iter().filter(|v| c.contains(v)).max().cloned()
            } else {
                all_versions.into_iter().max()
            };

            if let Some(version) = matched_version {
                let package_id = PackageId::new(name.clone(), version.clone());
                resolved_versions.insert(name_str.clone(), version.clone());
                resolved_majors.insert((name_str.clone(), version.major));

                let deps = self.provider.get_dependencies(&package_id).await;
                let dep_names: Vec<String> = deps.iter().map(|d| d.package.as_str().to_string()).collect();
                let dep_specs: Vec<(String, String)> = deps.iter()
                    .map(|d| (d.package.as_str().to_string(), d.spec.clone()))
                    .collect();

                for dep in &deps {
                    queue.push_front((dep.package.clone(), dep.spec.clone()));
                }

                resolutions.push(Resolution {
                    package_id: package_id.clone(),
                    version: version.clone(),
                    integrity: String::new(),
                    deps: dep_names,
                    dep_specs,
                });
            }
        }
        }

        Ok(SolveResult { resolutions })
    }
}

/// Resolves a workspace protocol dependency specifier to a concrete Resolution.
/// Supports `workspace:*`, `workspace:^`, and `workspace:~` specifiers.
/// Returns `None` if the package is not a workspace member or the spec is unsupported.
pub fn resolve_workspace_dep(
    name: &str,
    spec: &str,
    workspace: &mg_workspace::Workspace,
) -> Option<Resolution> {
    match spec {
        "workspace:*" | "workspace:^" | "workspace:~" => {
            let member = workspace.find_member(name)?;
            let version_str = member.package_json.version.as_ref()?;
            let version = Version::parse(version_str).ok()?;
            let package_name = PackageName::new(name).ok()?;

            Some(Resolution {
                package_id: PackageId::new(package_name, version.clone()),
                version,
                integrity: "workspace".to_string(),
                deps: Vec::new(),
                dep_specs: Vec::new(),
            })
        }
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct SolveError {
    pub message: String,
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SolveError {}

impl From<String> for SolveError {
    fn from(s: String) -> Self {
        Self { message: s }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    struct MockProvider;

    #[async_trait]
    impl DependencyProvider for MockProvider {
        async fn get_versions(&self, _package: &PackageName) -> Vec<Version> {
            vec![
                Version::parse("1.0.0").unwrap(),
                Version::parse("2.0.0").unwrap(),
            ]
        }

        async fn get_dependencies(&self, _package_id: &PackageId) -> Vec<ResolvedDep> {
            vec![]
        }
    }

    #[test]
    fn test_set_catalogs() {
        let provider = MockProvider;
        let mut resolver = Resolver::new(std::sync::Arc::new(provider));
        let mut catalogs = HashMap::new();
        let mut catalog = Catalog::default();
        catalog.set("react", "18.2.0");
        catalogs.insert("default".to_string(), catalog);
        resolver.set_catalogs(catalogs);
        let version = resolver.resolve_catalog("react", "default");
        assert!(version.is_ok());
        assert_eq!(version.unwrap().to_string(), "18.2.0");
    }

    #[test]
    fn test_resolve_catalog_not_found() {
        let provider = MockProvider;
        let resolver = Resolver::new(std::sync::Arc::new(provider));
        let result = resolver.resolve_catalog("nonexistent", "default");
        assert!(result.is_err());
    }

    #[test]
    fn test_workspace_info() {
        let info = WorkspaceInfo::new(vec![WorkspaceMemberInfo {
            name: "my-pkg".to_string(),
            path: PathBuf::from("/workspace/packages/my-pkg"),
            version: "1.0.0".to_string(),
        }]);
        assert!(info.is_workspace_package("my-pkg"));
        assert!(!info.is_workspace_package("other"));
    }

    #[test]
    fn test_solve_simple() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let resolver = Resolver::new(std::sync::Arc::new(MockProvider));
        let wanted = vec![(PackageName::new("react").unwrap(), "^1.0.0".to_string())];

        let result = rt.block_on(resolver.solve(&wanted)).unwrap();
        assert_eq!(result.resolutions.len(), 1);
    }

    #[test]
    fn test_semver_constraint_caret() {
        struct CaretProvider;
        #[async_trait]
        impl DependencyProvider for CaretProvider {
            async fn get_versions(&self, _package: &PackageName) -> Vec<Version> {
                let mut v = vec![
                    Version::parse("3.4.0").unwrap(),
                    Version::parse("3.4.1").unwrap(),
                    Version::parse("3.5.0").unwrap(),
                    Version::parse("4.0.0").unwrap(),
                    Version::parse("4.3.2").unwrap(),
                    Version::parse("6.0.0").unwrap(),
                    Version::parse("8.1.3").unwrap(),
                ];
                v.sort();
                v
            }
            async fn get_dependencies(&self, _package_id: &PackageId) -> Vec<ResolvedDep> {
                vec![]
            }
        }

        let rt = tokio::runtime::Runtime::new().unwrap();
        let resolver = Resolver::new(std::sync::Arc::new(CaretProvider));

        let wanted = vec![(PackageName::new("tailwindcss").unwrap(), "^3.4.0".to_string())];
        let result = rt.block_on(resolver.solve(&wanted)).unwrap();
        assert_eq!(result.resolutions.len(), 1);
        assert_eq!(result.resolutions[0].version.to_string(), "3.5.0");

        let wanted2 = vec![(PackageName::new("vite").unwrap(), "^6.0.0".to_string())];
        let result2 = rt.block_on(resolver.solve(&wanted2)).unwrap();
        assert_eq!(result2.resolutions.len(), 1);
        assert_eq!(result2.resolutions[0].version.to_string(), "6.0.0");

        let wanted3 = vec![(PackageName::new("vite").unwrap(), "^5.0.0".to_string())];
        let result3 = rt.block_on(resolver.solve(&wanted3)).unwrap();
        assert_eq!(result3.resolutions.len(), 0);
    }

    #[test]
    fn test_semver_constraint_exact() {
        struct ExactProvider;
        #[async_trait]
        impl DependencyProvider for ExactProvider {
            async fn get_versions(&self, _package: &PackageName) -> Vec<Version> {
                vec![
                    Version::parse("1.0.0").unwrap(),
                    Version::parse("1.0.1").unwrap(),
                    Version::parse("2.0.0").unwrap(),
                ]
            }
            async fn get_dependencies(&self, _package_id: &PackageId) -> Vec<ResolvedDep> {
                vec![]
            }
        }

        let rt = tokio::runtime::Runtime::new().unwrap();
        let resolver = Resolver::new(std::sync::Arc::new(ExactProvider));

        let wanted = vec![(PackageName::new("pkg").unwrap(), "1.0.0".to_string())];
        let result = rt.block_on(resolver.solve(&wanted)).unwrap();
        assert_eq!(result.resolutions.len(), 1);
        assert_eq!(result.resolutions[0].version.to_string(), "1.0.0");

        let wanted2 = vec![(PackageName::new("pkg").unwrap(), "~1.0.0".to_string())];
        let result2 = rt.block_on(resolver.solve(&wanted2)).unwrap();
        assert_eq!(result2.resolutions.len(), 1);
        assert_eq!(result2.resolutions[0].version.to_string(), "1.0.1");
    }

    proptest! {
        #[test]
        fn proptest_resolve_nonexistent_package(name in "[a-z]{3,10}", major in 0u64..5, minor in 0u64..5, patch in 0u64..5) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let provider = MockProvider;
            let resolver = Resolver::new(std::sync::Arc::new(provider));
            let spec = format!("{}.{}.{}", major, minor, patch);
            let wanted = vec![
                (PackageName::new(&name).unwrap_or_else(|_| PackageName::new("pkg").unwrap()), spec),
            ];
            let result = rt.block_on(resolver.solve(&wanted));
            if let Ok(ref sol) = result {
                for res in &sol.resolutions {
                    let _ = res;
                }
            }
        }
    }
}
