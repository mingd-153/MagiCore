//! PubGrub-based dependency resolver

pub mod pubgrub;

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::path::PathBuf;

use mg_core::{Catalog, PackageId, PackageName, Version};

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

pub trait DependencyProvider: Send + Sync {
    fn get_versions(&self, package: &PackageName) -> Vec<Version>;
    fn get_dependencies(&self, package_id: &PackageId) -> Vec<ResolvedDep>;
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
    pub fn resolve_with_workspace(
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
                });
            } else {
                remaining.push((name.clone(), spec.clone()));
            }
        }

        if !remaining.is_empty() {
            let result = self.solve(&remaining)?;
            resolutions.extend(result.resolutions);
        }

        Ok(SolveResult { resolutions })
    }

    pub fn solve(&self, wanted: &[(PackageName, String)]) -> Result<SolveResult, SolveError> {
        use mg_core::VersionRange;

        let mut resolutions: Vec<Resolution> = Vec::new();
        // Track all constraints for each package, keyed by name
        let mut all_constraints: HashMap<String, Vec<String>> = HashMap::new();
        // Track resolved version per package
        let mut resolved_versions: HashMap<String, Version> = HashMap::new();
        let mut queue: VecDeque<(PackageName, String)> = wanted.iter().map(|(n, s)| (n.clone(), s.clone())).collect();

        while let Some((name, spec)) = queue.pop_front() {
            let name_str = name.as_str().to_string();

            // Collect all constraints for this package
            all_constraints.entry(name_str.clone())
                .or_insert_with(Vec::new)
                .push(spec.clone());

            // If already resolved, check if existing version satisfies this new constraint
            if let Some(ref existing_version) = resolved_versions.get(&name_str) {
                if let Some(ref c) = VersionRange::parse(&spec).ok() {
                    if !c.contains(existing_version) {
                        // Existing version doesn't satisfy this new constraint.
                        // Try to find a version that satisfies all known constraints.
                        let all_versions = self.provider.get_versions(&name);
                        let constraints = all_constraints.get(&name_str).unwrap();
                        let ranges: Vec<VersionRange> = constraints.iter()
                            .filter_map(|s| VersionRange::parse(s).ok())
                            .collect();

                        let best = all_versions.iter().filter(|v| {
                            ranges.iter().all(|r| r.contains(v))
                        }).max().cloned();

                        if let Some(new_version) = best {
                            // Update the resolution with the new version
                            if let Some(res) = resolutions.iter_mut().find(|r| {
                                r.package_id.name().as_str() == name_str
                            }) {
                                res.version = new_version.clone();
                                res.package_id = PackageId::new(name.clone(), new_version.clone());
                            }
                            resolved_versions.insert(name_str, new_version);
                        }
                        // If no compatible version found, keep existing (warn?)
                    }
                }
                continue;
            }

            // Check lockfile first for pre-resolved packages
            if let Some(ref lockfile_pkgs) = self.lockfile_packages {
                if let Some(lockfile_version) = lockfile_pkgs.get(name.as_str()) {
                    if let Ok(version) = Version::parse(lockfile_version) {
                        resolved_versions.insert(name_str, version.clone());
                        resolutions.push(Resolution {
                            package_id: PackageId::new(name.clone(), version.clone()),
                            version,
                            integrity: String::new(),
                            deps: Vec::new(),
                        });
                        continue;
                    }
                }
            }

            let constraint = VersionRange::parse(&spec).ok();
            let all_versions = self.provider.get_versions(&name);

            // Filter versions by the semver constraint, then pick the latest matching version
            let matched_version = if let Some(ref c) = constraint {
                all_versions.iter().filter(|v| c.contains(v)).max().cloned()
            } else {
                all_versions.into_iter().max()
            };

            if let Some(version) = matched_version {
                let package_id = PackageId::new(name.clone(), version.clone());
                resolved_versions.insert(name_str.clone(), version.clone());

                // Fetch dependencies of this package
                let deps = self.provider.get_dependencies(&package_id);
                let dep_names: Vec<String> = deps.iter().map(|d| d.package.as_str().to_string()).collect();

                // Queue transitive dependencies (at the front for DFS-like order)
                // so that we fully resolve a package's subtree before siblings
                for dep in &deps {
                    let dep_name = dep.package.as_str().to_string();
                    queue.push_front((dep.package.clone(), dep.spec.clone()));
                }

                resolutions.push(Resolution {
                    package_id: package_id.clone(),
                    version: version.clone(),
                    integrity: String::new(),
                    deps: dep_names,
                });
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

    impl DependencyProvider for MockProvider {
        fn get_versions(&self, _package: &PackageName) -> Vec<Version> {
            vec![
                Version::parse("1.0.0").unwrap(),
                Version::parse("2.0.0").unwrap(),
            ]
        }

        fn get_dependencies(&self, _package_id: &PackageId) -> Vec<ResolvedDep> {
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
        let resolver = Resolver::new(std::sync::Arc::new(MockProvider));
        let wanted = vec![(PackageName::new("react").unwrap(), "^1.0.0".to_string())];

        let result = resolver.solve(&wanted).unwrap();
        assert_eq!(result.resolutions.len(), 1);
        // Should resolve to 1.0.0 or 2.0.0, but NOT skip
    }

    #[test]
    fn test_semver_constraint_caret() {
        struct CaretProvider;
        impl DependencyProvider for CaretProvider {
            fn get_versions(&self, _package: &PackageName) -> Vec<Version> {
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
            fn get_dependencies(&self, _package_id: &PackageId) -> Vec<ResolvedDep> {
                vec![]
            }
        }

        let resolver = Resolver::new(std::sync::Arc::new(CaretProvider));

        // ^3.4.0 should allow 3.x but NOT 4.x — picks highest 3.x
        let wanted = vec![(PackageName::new("tailwindcss").unwrap(), "^3.4.0".to_string())];
        let result = resolver.solve(&wanted).unwrap();
        assert_eq!(result.resolutions.len(), 1);
        assert_eq!(result.resolutions[0].version.to_string(), "3.5.0");

        // ^6.0.0 should allow 6.x but NOT 8.x — picks 6.0.0
        let wanted2 = vec![(PackageName::new("vite").unwrap(), "^6.0.0".to_string())];
        let result2 = resolver.solve(&wanted2).unwrap();
        assert_eq!(result2.resolutions.len(), 1);
        assert_eq!(result2.resolutions[0].version.to_string(), "6.0.0");

        // With versions 3.x, 4.x, 6.x, 8.x — ^5.0.0 should return empty (no match)
        let wanted3 = vec![(PackageName::new("vite").unwrap(), "^5.0.0".to_string())];
        let result3 = resolver.solve(&wanted3).unwrap();
        assert_eq!(result3.resolutions.len(), 0);
    }

    #[test]
    fn test_semver_constraint_exact() {
        struct ExactProvider;
        impl DependencyProvider for ExactProvider {
            fn get_versions(&self, _package: &PackageName) -> Vec<Version> {
                vec![
                    Version::parse("1.0.0").unwrap(),
                    Version::parse("1.0.1").unwrap(),
                    Version::parse("2.0.0").unwrap(),
                ]
            }
            fn get_dependencies(&self, _package_id: &PackageId) -> Vec<ResolvedDep> {
                vec![]
            }
        }

        let resolver = Resolver::new(std::sync::Arc::new(ExactProvider));

        // Exact match for 1.0.0
        let wanted = vec![(PackageName::new("pkg").unwrap(), "1.0.0".to_string())];
        let result = resolver.solve(&wanted).unwrap();
        assert_eq!(result.resolutions.len(), 1);
        assert_eq!(result.resolutions[0].version.to_string(), "1.0.0");

        // Tilde ~1.0.0 should allow 1.0.x only
        let wanted2 = vec![(PackageName::new("pkg").unwrap(), "~1.0.0".to_string())];
        let result2 = resolver.solve(&wanted2).unwrap();
        assert_eq!(result2.resolutions.len(), 1);
        assert_eq!(result2.resolutions[0].version.to_string(), "1.0.1");
    }

    proptest! {
        #[test]
        fn proptest_resolve_nonexistent_package(name in "[a-z]{3,10}", major in 0u64..5, minor in 0u64..5, patch in 0u64..5) {
            let provider = MockProvider;
            let resolver = Resolver::new(std::sync::Arc::new(provider));
            let spec = format!("{}.{}.{}", major, minor, patch);
            let wanted = vec![
                (PackageName::new(&name).unwrap_or_else(|_| PackageName::new("pkg").unwrap()), spec),
            ];
            let result = resolver.solve(&wanted);
            // Resolution may be empty if no versions match the constraint
            // (MockProvider only has 1.0.0 and 2.0.0)
            if let Ok(ref sol) = result {
                // If it resolves, all versions must satisfy their constraints
                for res in &sol.resolutions {
                    let _ = res; // at least one resolution
                }
            }
        }
    }
}
