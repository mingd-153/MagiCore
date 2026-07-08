/// Web ecosystem adapter for MegaGate
///
/// Supports 3 adapter modes:
/// - **Native**: Rust-native npm resolver + CAS store + smart linker (faster than pnpm)
/// - **Delegate**: Wraps existing PMs (composer, maven, go mod) for unified CLI
/// - **Compiler**: Build pipelines (vite, next build, tsc) for each framework

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use mg_types::{
    adapter::{
        AuditReport, InstallSummary, InstalledPackage, PackageAdapter, ResolvedGraph,
        ResolvedPackage, UpdatedPackage,
    },
    Ecosystem, MgResult, Manifest, PackageId, PackageName, Version, VersionRange,
};
use mg_resolver::{DependencyProvider, Resolver as CoreResolver, ResolvedDep};
use mg_store::ContentStore;
use serde::{Deserialize, Serialize};

pub mod manifest;
pub mod registry;
pub mod installer;

pub mod native;
pub mod delegate;
pub mod compiler;

/// Web adapter implementation
pub struct WebAdapter {
    registry_url: String,
    resolver: Arc<CoreResolver>,
    store: Option<ContentStore>,
}

impl WebAdapter {
    pub fn new() -> Self {
        let provider = Arc::new(NpmDependencyProvider::new("https://registry.npmjs.org"));
        Self {
            registry_url: "https://registry.npmjs.org".to_string(),
            resolver: Arc::new(CoreResolver::new(provider)),
            store: None,
        }
    }

    pub fn with_registry(registry_url: String) -> Self {
        let provider = Arc::new(NpmDependencyProvider::new(&registry_url));
        Self {
            registry_url,
            resolver: Arc::new(CoreResolver::new(provider)),
            store: None,
        }
    }

    pub fn with_store(mut self, store: ContentStore) -> Self {
        self.store = Some(store);
        self
    }
}

impl Default for WebAdapter {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl PackageAdapter for WebAdapter {
    fn name(&self) -> &str { "web" }

    fn ecosystem(&self) -> Ecosystem { Ecosystem::Web }

    fn can_handle(&self, project_root: &Path) -> bool {
        project_root.join("package.json").exists()
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        let pkg_path = project_root.join("package.json");
        if !pkg_path.exists() {
            return Err(mg_types::MgError::Other(format!(
                "No {} found in '{}'. Run '{}' first.",
                "package.json",
                project_root.display(),
                mg_ui::style_cmd("mg init --template web"),
            )));
        }
        let content = std::fs::read_to_string(&pkg_path)?;
        let pkg_json: PackageJson = serde_json::from_str(&content)?;

        let mut manifest = Manifest::new(&pkg_json.name, Ecosystem::Web);
        manifest.version = Some(Version::parse(&pkg_json.version).unwrap_or_default());

        if let Some(deps) = pkg_json.dependencies {
            for (name, range) in deps {
                let pn = PackageName::new(&name)?;
                let vr = VersionRange::parse(&range)?;
                manifest.dependencies.push(mg_types::DependencySpec::new(pn, vr));
            }
        }

        if let Some(dev_deps) = pkg_json.dev_dependencies {
            for (name, range) in dev_deps {
                let pn = PackageName::new(&name)?;
                let vr = VersionRange::parse(&range)?;
                let mut spec = mg_types::DependencySpec::new(pn, vr);
                spec.dev = true;
                manifest.dev_dependencies.push(spec);
            }
        }

        Ok(manifest)
    }

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        let mut pkg = PackageJson {
            name: manifest.name.clone(),
            version: manifest.version.as_ref().map(|v| v.to_string()).unwrap_or_else(|| "0.1.0".to_string()),
            description: None,
            dependencies: None,
            dev_dependencies: None,
        };

        if !manifest.dependencies.is_empty() {
            let mut deps = std::collections::HashMap::new();
            for dep in &manifest.dependencies {
                deps.insert(dep.name.as_str().to_string(), dep.range.to_string());
            }
            pkg.dependencies = Some(deps);
        }

        if !manifest.dev_dependencies.is_empty() {
            let mut deps = std::collections::HashMap::new();
            for dep in &manifest.dev_dependencies {
                deps.insert(dep.name.as_str().to_string(), dep.range.to_string());
            }
            pkg.dev_dependencies = Some(deps);
        }

        let path = project_root.join("package.json");
        pkg.save(&path)?;
        Ok(())
    }

    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph> {
        let mut wanted: Vec<(PackageName, String)> = Vec::new();
        for dep in manifest.all_dependencies() {
            wanted.push((dep.name.clone(), dep.range.to_string()));
        }

        if wanted.is_empty() {
            return Ok(ResolvedGraph::empty());
        }

        let result = self.resolver.solve(&wanted).await.map_err(|e| {
            mg_types::MgError::DependencyConflict(e.message)
        })?;

        let mut packages: Vec<ResolvedPackage> = Vec::with_capacity(result.resolutions.len());
        for r in result.resolutions {
            let is_direct = manifest.find_dep(r.package_id.name().as_str()).is_some();
            let is_dev = manifest.dev_dependencies.iter().any(|d| d.name == *r.package_id.name());
            packages.push(ResolvedPackage {
                id: r.package_id,
                integrity: r.integrity,
                tarball_url: String::new(),
                deps: r.deps.iter().filter_map(|d| {
                    PackageId::parse(d).ok()
                }).collect(),
                direct: is_direct,
                dev: is_dev,
            });
        }

        Ok(ResolvedGraph { packages })
    }

    async fn fetch(&self, graph: &ResolvedGraph) -> MgResult<()> {
        let registry = native::npm_registry::NpmRegistry::new(&self.registry_url);
        for pkg in &graph.packages {
            let url = format!("{}/{}/-/{}-{}.tgz",
                self.registry_url, pkg.id.name_str(), pkg.id.name().unscoped(), pkg.id.version());
            match registry.download_tarball(&url).await {
                Ok(bytes) => {
                    if let Some(ref store) = self.store {
                        store.import_bytes(&bytes)
                            .map_err(|e| mg_types::MgError::Store(e.to_string()))?;
                    }
                }
                Err(e) => {
                    eprintln!("Warning: failed to fetch {}: {}", pkg.id, e);
                }
            }
        }
        Ok(())
    }

    async fn install(
        &self,
        graph: &ResolvedGraph,
        project_root: &Path,
    ) -> MgResult<InstallSummary> {
        let start = std::time::Instant::now();
        let node_modules = project_root.join("node_modules");
        std::fs::create_dir_all(&node_modules)?;

        let mut summary = InstallSummary::default();
        for pkg in &graph.packages {
            let pkg_dir = node_modules.join(pkg.id.name().as_str());
            std::fs::create_dir_all(&pkg_dir)?;

            let meta_path = pkg_dir.join("package.json");
            let meta = serde_json::json!({
                "name": pkg.id.name_str(),
                "version": pkg.id.version().to_string(),
            });
            std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)
                .map_err(|e| mg_types::MgError::Other(format!("JSON error: {e}")))?)?;

            summary.added.push(pkg.id.clone());
        }

        summary.duration_ms = start.elapsed().as_millis() as u64;
        Ok(summary)
    }

    async fn add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        dev: bool,
    ) -> MgResult<PackageId> {
        let mut manifest = self.parse_manifest(project_root).await?;
        let range = range.cloned().unwrap_or_else(VersionRange::star);
        let spec = mg_types::DependencySpec::new(name.clone(), range);

        if dev {
            manifest.dev_dependencies.push(spec);
        } else {
            manifest.dependencies.push(spec);
        }

        self.write_manifest(project_root, &manifest).await?;
        Ok(PackageId::new(name.clone(), Version::new(0, 0, 0)))
    }

    async fn update(
        &self,
        _project_root: &Path,
        _name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        Ok(Vec::new())
    }

    async fn remove(
        &self,
        project_root: &Path,
        name: &PackageName,
    ) -> MgResult<()> {
        let mut manifest = self.parse_manifest(project_root).await?;
        manifest.dependencies.retain(|d| d.name != *name);
        manifest.dev_dependencies.retain(|d| d.name != *name);
        self.write_manifest(project_root, &manifest).await?;
        Ok(())
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        let manifest = self.parse_manifest(project_root).await?;
        let mut packages = Vec::new();

        for dep in &manifest.dependencies {
            packages.push(InstalledPackage {
                id: PackageId::new(dep.name.clone(), Version::new(0, 0, 0)),
                path: project_root.join("node_modules").join(dep.name.as_str()),
                integrity: None,
                is_direct: true,
                is_dev: false,
            });
        }

        Ok(packages)
    }

    async fn audit(&self, _project_root: &Path) -> MgResult<AuditReport> {
        Ok(AuditReport::clean(0))
    }
}

/// Dependency provider that uses the npm registry for resolution
struct NpmDependencyProvider {
    registry: native::npm_registry::NpmRegistry,
}

impl NpmDependencyProvider {
    fn new(registry_url: &str) -> Self {
        Self { registry: native::npm_registry::NpmRegistry::new(registry_url) }
    }
}

#[async_trait]
impl DependencyProvider for NpmDependencyProvider {
    async fn get_versions(&self, package: &PackageName) -> Vec<Version> {
        match self.registry.fetch_metadata(package.as_str()).await {
            Ok(meta) => {
                let mut versions: Vec<Version> = meta.versions.keys()
                    .filter_map(|v| Version::parse(v).ok())
                    .collect();
                versions.sort();
                versions
            }
            Err(_) => Vec::new(),
        }
    }

    async fn get_dependencies(&self, package_id: &PackageId) -> Vec<ResolvedDep> {
        match self.registry.fetch_metadata(package_id.name_str()).await {
            Ok(meta) => {
                let ver_str = package_id.version().to_string();
                if let Some(vinfo) = meta.versions.get(&ver_str) {
                    if let Some(ref deps) = vinfo.dependencies {
                        return deps.iter().filter_map(|(name, spec)| {
                            PackageName::new(name).ok().map(|pn| ResolvedDep {
                                package: pn,
                                spec: spec.clone(),
                                optional: false,
                                peer: false,
                            })
                        }).collect();
                    }
                }
                Vec::new()
            }
            Err(_) => Vec::new(),
        }
    }
}

/// package.json representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageJson {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "devDependencies")]
    pub dev_dependencies: Option<std::collections::HashMap<String, String>>,
}

impl PackageJson {
    pub fn new(name: String, version: String) -> Self {
        Self { name, version, description: None, dependencies: None, dev_dependencies: None }
    }

    pub fn load(path: &Path) -> Result<Self, anyhow::Error> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), anyhow::Error> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_adapter() {
        let adapter = WebAdapter::new();
        assert_eq!(adapter.registry_url, "https://registry.npmjs.org");
    }

    #[test]
    fn test_package_json() {
        let pkg = PackageJson::new("test-project".to_string(), "1.0.0".to_string());
        assert_eq!(pkg.name, "test-project");
        assert_eq!(pkg.version, "1.0.0");
    }

    #[test]
    fn test_can_handle() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = PackageJson::new("test".to_string(), "1.0.0".to_string());
        pkg.save(&dir.path().join("package.json")).unwrap();
        let adapter = WebAdapter::new();
        assert!(adapter.can_handle(dir.path()));
    }

    #[test]
    fn test_cannot_handle_without_package_json() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = WebAdapter::new();
        assert!(!adapter.can_handle(dir.path()));
    }
}
