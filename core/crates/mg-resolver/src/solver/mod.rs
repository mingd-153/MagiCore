pub mod pubgrub;

use std::collections::{HashMap, HashSet, VecDeque};
use async_trait::async_trait;
use mg_types::{PackageId, PackageName, Version, VersionRange};

pub use pubgrub::{PubGrubSolver, Term, Incompatibility, Cause, SolveError as PubGrubSolveError};

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
    pub dep_specs: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct SolveResult {
    pub resolutions: Vec<Resolution>,
}

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
    fn from(s: String) -> Self { Self { message: s } }
}

#[async_trait]
pub trait DependencyProvider: Send + Sync {
    async fn get_versions(&self, package: &PackageName) -> Vec<Version>;
    async fn get_dependencies(&self, package_id: &PackageId) -> Vec<ResolvedDep>;

    async fn prefetch_versions(&self, packages: &[PackageName]) -> Vec<(PackageName, Vec<Version>)> {
        let mut results = Vec::with_capacity(packages.len());
        for name in packages {
            let versions = self.get_versions(name).await;
            results.push((name.clone(), versions));
        }
        results
    }

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
pub struct DepInfo {
    pub name: String,
    pub version: Option<String>,
    pub registry: Option<String>,
}

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
                    "Dependency confusion: '{name}' is both a workspace package and an external dependency (version {version}). \
                     Use \"workspace:*\" for workspace packages."
                ));
            }
        }

        if name.starts_with('@') {
            if let Some(scope) = name.split('/').next() {
                if let Some(expected) = scoped_registries.get(scope) {
                    if dep.registry.as_deref() != Some(expected.as_str()) {
                        warnings.push(format!(
                            "Potential dependency confusion: '{name}' should resolve from '{expected}' but is configured for '{}'",
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
                        "Potential dependency confusion: '{name}' resolves from '{registry}' which is not in the trusted registries list"
                    ));
                }
            }
        }
    }

    warnings
}

pub struct Resolver {
    provider: std::sync::Arc<dyn DependencyProvider>,
    overrides: HashMap<String, String>,
}

impl Resolver {
    pub fn new(provider: std::sync::Arc<dyn DependencyProvider>) -> Self {
        Self { provider, overrides: HashMap::new() }
    }

    pub fn set_overrides(&mut self, overrides: HashMap<String, String>) {
        self.overrides = overrides;
    }

    pub async fn solve(&self, wanted: &[(PackageName, String)]) -> Result<SolveResult, SolveError> {
        let mut resolutions: Vec<Resolution> = Vec::new();
        let mut resolved_versions: HashMap<String, Version> = HashMap::new();
        let mut queue: VecDeque<(PackageName, String)> = wanted.iter().map(|(n, s)| (n.clone(), s.clone())).collect();
        let mut resolved_majors: HashSet<(String, u64)> = HashSet::new();

        // Pre-fetch initial packages
        let initial_packages: Vec<PackageName> = wanted.iter().map(|(n, _)| n.clone()).collect();
        self.provider.prefetch_versions(&initial_packages).await;

        while !queue.is_empty() {
            let batch_size = queue.len().min(50);
            let batch: Vec<(PackageName, String)> = queue.drain(..batch_size).collect();

            // Prefetch unique names in batch
            let mut seen = HashSet::new();
            let batch_names: Vec<PackageName> = batch.iter()
                .filter(|(n, _)| seen.insert(n.as_str().to_string()))
                .map(|(n, _)| n.clone())
                .collect();
            if !batch_names.is_empty() {
                self.provider.prefetch_versions(&batch_names).await;
            }

            for (name, spec) in batch {
                let name_str = name.as_str().to_string();

                // Check override first
                if let Some(override_spec) = self.overrides.get(&name_str) {
                    let constraint = VersionRange::parse(override_spec).ok();
                    let all_versions = self.provider.get_versions(&name).await;
                    let version = constraint.as_ref()
                        .and_then(|c| all_versions.iter().filter(|v| c.matches(v)).max().cloned())
                        .or_else(|| all_versions.into_iter().max());

                    if let Some(version) = version {
                        Self::add_resolution(&mut resolutions, &mut resolved_versions, &mut resolved_majors,
                            &name, &name_str, version, &self.provider, &mut queue).await;
                    }
                    continue;
                }

                // Already resolved
                if let Some(existing) = resolved_versions.get(&name_str) {
                    if let Ok(c) = VersionRange::parse(&spec) {
                        if c.matches(existing) { continue; }

                        let all = self.provider.get_versions(&name).await;
                        let same_major = all.iter()
                            .filter(|v| c.matches(v) && v.major == existing.major)
                            .max().cloned();
                        if let Some(ref new_v) = same_major {
                            if new_v > existing {
                                if let Some(res) = resolutions.iter_mut().find(|r|
                                    r.package_id.name().as_str() == name_str && r.version.major == existing.major
                                ) {
                                    res.version = new_v.clone();
                                    res.package_id = PackageId::new(name.clone(), new_v.clone());
                                }
                                resolved_versions.insert(name_str.clone(), new_v.clone());
                            }
                            continue;
                        }

                        // Try different major
                        let best_other = all.iter().filter(|v| c.matches(v)).max().cloned();
                        if let Some(ref other) = best_other {
                            let major_key = (name_str.clone(), other.major);
                            if !resolved_majors.contains(&major_key) {
                                resolved_majors.insert(major_key);
                                Self::add_resolution(&mut resolutions, &mut resolved_versions, &mut resolved_majors,
                                    &name, &name_str, other.clone(), &self.provider, &mut queue).await;
                            }
                        }
                    }
                    continue;
                }

                // Fresh resolution
                let constraint = VersionRange::parse(&spec).ok();
                let all_versions = self.provider.get_versions(&name).await;

                if let Some(version) = constraint.as_ref()
                    .and_then(|c| all_versions.iter().filter(|v| c.matches(v)).max().cloned())
                    .or_else(|| all_versions.into_iter().max())
                {
                    Self::add_resolution(&mut resolutions, &mut resolved_versions, &mut resolved_majors,
                        &name, &name_str, version, &self.provider, &mut queue).await;
                }
            }
        }

        Ok(SolveResult { resolutions })
    }

    async fn add_resolution(
        resolutions: &mut Vec<Resolution>,
        resolved_versions: &mut HashMap<String, Version>,
        resolved_majors: &mut HashSet<(String, u64)>,
        name: &PackageName,
        name_str: &str,
        version: Version,
        provider: &std::sync::Arc<dyn DependencyProvider>,
        queue: &mut VecDeque<(PackageName, String)>,
    ) {
        let package_id = PackageId::new(name.clone(), version.clone());
        resolved_versions.insert(name_str.to_string(), version.clone());
        resolved_majors.insert((name_str.to_string(), version.major));

        let deps = provider.get_dependencies(&package_id).await;
        let dep_names: Vec<String> = deps.iter().map(|d| d.package.as_str().to_string()).collect();
        let dep_specs: Vec<(String, String)> = deps.iter()
            .map(|d| (d.package.as_str().to_string(), d.spec.clone()))
            .collect();

        for dep in &deps {
            queue.push_front((dep.package.clone(), dep.spec.clone()));
        }

        resolutions.push(Resolution {
            package_id,
            version,
            integrity: String::new(),
            deps: dep_names,
            dep_specs,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl DependencyProvider for MockProvider {
        async fn get_versions(&self, _package: &PackageName) -> Vec<Version> {
            vec![Version::parse("1.0.0").unwrap(), Version::parse("2.0.0").unwrap()]
        }

        async fn get_dependencies(&self, _package_id: &PackageId) -> Vec<ResolvedDep> {
            vec![]
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
        struct CaretProvider;
        #[async_trait]
        impl DependencyProvider for CaretProvider {
            async fn get_versions(&self, _p: &PackageName) -> Vec<Version> {
                let mut v = vec![
                    Version::parse("3.4.0").unwrap(),
                    Version::parse("3.5.0").unwrap(),
                    Version::parse("4.0.0").unwrap(),
                ];
                v.sort();
                v
            }
            async fn get_dependencies(&self, _id: &PackageId) -> Vec<ResolvedDep> { vec![] }
        }

        let resolver = Resolver::new(std::sync::Arc::new(CaretProvider));
        let wanted = vec![(PackageName::new("tailwindcss").unwrap(), "^3.4.0".to_string())];
        let result = resolver.solve(&wanted).await.unwrap();
        assert_eq!(result.resolutions[0].version.to_string(), "3.5.0");
    }

    #[tokio::test]
    async fn test_semver_exact() {
        struct ExactProvider;
        #[async_trait]
        impl DependencyProvider for ExactProvider {
            async fn get_versions(&self, _p: &PackageName) -> Vec<Version> {
                vec![Version::parse("1.0.0").unwrap(), Version::parse("1.0.1").unwrap()]
            }
            async fn get_dependencies(&self, _id: &PackageId) -> Vec<ResolvedDep> { vec![] }
        }

        let resolver = Resolver::new(std::sync::Arc::new(ExactProvider));
        let wanted = vec![(PackageName::new("pkg").unwrap(), "1.0.0".to_string())];
        let result = resolver.solve(&wanted).await.unwrap();
        assert_eq!(result.resolutions[0].version.to_string(), "1.0.0");
    }

    #[test]
    fn test_dependency_confusion_detection() {
        let workspace = vec!["my-pkg".to_string()];
        let deps = vec![DepInfo {
            name: "my-pkg".to_string(),
            version: Some("1.0.0".to_string()),
            registry: Some("https://registry.npmjs.org".to_string()),
        }];
        let warnings = check_dependency_confusion(&workspace, &deps, &HashMap::new(), &[]);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("Dependency confusion"));
    }
}
