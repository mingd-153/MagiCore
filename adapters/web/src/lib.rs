use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use mg_adapter_base::{AddOptions, BaseAdapter};
use mg_types::{
    adapter::{
        AuditReport, InstallSummary, InstalledPackage, PackageAdapter, ResolvedGraph,
        ResolvedPackage, UpdatedPackage,
    },
    Ecosystem, MgResult, Manifest, PackageId, PackageName, Version, VersionRange,
};
use mg_resolver::{DependencyError, DependencyProvider, Resolver as CoreResolver, ResolvedDep};
use mg_store::ContentStore;
use serde::{Deserialize, Serialize};

pub mod native;

pub struct WebAdapter {
    registry_url: String,
    resolver: Arc<CoreResolver>,
    store: Option<ContentStore>,
}

impl WebAdapter {
    pub fn new() -> Self {
        let provider = Arc::new(NpmDependencyProvider::new("https://registry.npmjs.org"));
        Self { registry_url: "https://registry.npmjs.org".to_string(), resolver: Arc::new(CoreResolver::new(provider)), store: None }
    }
    pub fn with_registry(registry_url: String) -> Self {
        let provider = Arc::new(NpmDependencyProvider::new(&registry_url));
        Self { registry_url, resolver: Arc::new(CoreResolver::new(provider)), store: None }
    }
    pub fn with_store(mut self, store: ContentStore) -> Self { self.store = Some(store); self }
}
impl Default for WebAdapter { fn default() -> Self { Self::new() } }

#[async_trait]
impl BaseAdapter for WebAdapter {}

#[async_trait]
impl PackageAdapter for WebAdapter {
    fn name(&self) -> &str { "web" }
    fn ecosystem(&self) -> mg_types::ecosystem::Ecosystem { mg_types::ecosystem::Ecosystem::Web }
    fn can_handle(&self, project_root: &Path) -> bool { project_root.join("package.json").exists() }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        let pkg_path = project_root.join("package.json");
        if !pkg_path.exists() {
            return Err(mg_types::MgError::Other(format!("No package.json in '{}'. Run 'mg init --template web' first.", project_root.display())));
        }
        let pkg_json: PackageJson = serde_json::from_str(&std::fs::read_to_string(&pkg_path)?)?;
        let mut manifest = Manifest::new(&pkg_json.name, mg_types::ecosystem::Ecosystem::Web);
        manifest.version = Some(
            Version::parse(&pkg_json.version)
                .map_err(|_| mg_types::MgError::Other(format!("invalid version '{}' in package.json", pkg_json.version)))?
        );
        let parse_deps = |map: Option<std::collections::HashMap<String, String>>| -> MgResult<Vec<mg_types::DependencySpec>> {
            match map {
                Some(deps) => {
                    let mut out = Vec::with_capacity(deps.len());
                    for (name, range) in deps {
                        let pn = PackageName::new(name)?;
                        let vr = VersionRange::parse(&range)?;
                        out.push(mg_types::DependencySpec::new(pn, vr));
                    }
                    Ok(out)
                }
                None => Ok(vec![]),
            }
        };
        manifest.dependencies = parse_deps(pkg_json.dependencies)?;
        manifest.dev_dependencies = parse_deps(pkg_json.dev_dependencies)?;
        manifest.peer_dependencies = parse_deps(pkg_json.peer_dependencies)?;
        manifest.optional_dependencies = parse_deps(pkg_json.optional_dependencies)?;
        Ok(manifest)
    }

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        let to_map = |deps: &[mg_types::DependencySpec]| -> std::collections::HashMap<String, String> {
            deps.iter().map(|d| (d.name.as_str().to_string(), d.range.to_string())).collect()
        };
        let pkg = PackageJson {
            name: manifest.name.clone(),
            version: manifest.version.as_ref().map(|v| v.to_string()).unwrap_or_else(|| "0.1.0".into()),
            description: None,
            dependencies: if manifest.dependencies.is_empty() { None } else { Some(to_map(&manifest.dependencies)) },
            dev_dependencies: if manifest.dev_dependencies.is_empty() { None } else { Some(to_map(&manifest.dev_dependencies)) },
            peer_dependencies: if manifest.peer_dependencies.is_empty() { None } else { Some(to_map(&manifest.peer_dependencies)) },
            optional_dependencies: if manifest.optional_dependencies.is_empty() { None } else { Some(to_map(&manifest.optional_dependencies)) },
        };
        pkg.save(&project_root.join("package.json"))?;
        Ok(())
    }

    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph> {
        let mut wanted: Vec<(PackageName, String)> = manifest.all_dependencies().map(|d| (d.name.clone(), d.range.to_string())).collect();
        if wanted.is_empty() { return Ok(ResolvedGraph::empty()); }
        let result = self.resolver.solve(&wanted).await.map_err(|e| mg_types::MgError::DependencyConflict(e.message))?;
        let packages = result.resolutions.iter().map(|r| {
            let is_direct = manifest.find_dep(r.package_id.name().as_str()).is_some();
            let is_dev = manifest.dev_dependencies.iter().any(|d| d.name == *r.package_id.name());
            ResolvedPackage {
                id: r.package_id.clone(),
                integrity: r.integrity.clone(),
                tarball_url: String::new(),
                deps: r.deps.iter().filter_map(|d| PackageId::parse(d).ok()).collect(),
                direct: is_direct,
                dev: is_dev,
            }
        }).collect();
        Ok(ResolvedGraph { packages })
    }

    async fn fetch(&self, graph: &ResolvedGraph) -> MgResult<()> {
        let reg = native::npm_registry::NpmRegistry::new(&self.registry_url);
        for pkg in &graph.packages {
            let unscoped = pkg.id.name().unscoped();
            let url = format!("{}/{}/-/{}-{}.tgz", self.registry_url, pkg.id.name_str(), unscoped, pkg.id.version());
            let bytes = reg.download_tarball(&url).await
                .map_err(|e| mg_types::MgError::Network(format!("download failed for '{}': {}", pkg.id.name_str(), e)))?;
            if let Some(ref store) = self.store {
                store.import_bytes(&bytes).map_err(|e| mg_types::MgError::Store(e.to_string()))?;
            }
        }
        Ok(())
    }

    async fn install(&self, graph: &ResolvedGraph, project_root: &Path) -> MgResult<InstallSummary> {
        let start = std::time::Instant::now();
        let node_modules = project_root.join("node_modules");
        std::fs::create_dir_all(&node_modules)?;
        let mut summary = InstallSummary::default();
        for pkg in &graph.packages {
            let dir = node_modules.join(pkg.id.name().as_str());
            std::fs::create_dir_all(&dir)?;
            let meta = serde_json::json!({"name": pkg.id.name_str(), "version": pkg.id.version().to_string()});
            std::fs::write(dir.join("package.json"), serde_json::to_string_pretty(&meta).map_err(|e| mg_types::MgError::Other(e.to_string()))?)?;
            summary.added.push(pkg.id.clone());
        }
        summary.duration_ms = start.elapsed().as_millis() as u64;
        Ok(summary)
    }

    async fn add(&self, project_root: &Path, name: &PackageName, range: Option<&VersionRange>, dev: bool, optional: bool, peer: bool, exact: bool, no_save: bool, global: bool) -> MgResult<PackageId> {
        let opts = AddOptions { dev, optional, peer, exact, no_save, global };
        self.base_add(project_root, name, range, opts).await
    }
    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()> { self.base_remove(project_root, name).await }
    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> { self.base_list(project_root).await }
    async fn update(&self, project_root: &Path, name: Option<&PackageName>) -> MgResult<Vec<UpdatedPackage>> { self.base_update(project_root, name).await }
    async fn audit(&self, _: &Path) -> MgResult<AuditReport> { Ok(AuditReport::clean(0)) }
}

struct NpmDependencyProvider { registry: native::npm_registry::NpmRegistry }
impl NpmDependencyProvider {
    fn new(url: &str) -> Self { Self { registry: native::npm_registry::NpmRegistry::new(url) } }
}
#[async_trait]
impl DependencyProvider for NpmDependencyProvider {
    async fn get_versions(&self, package: &PackageName) -> Result<Vec<Version>, DependencyError> {
        let meta = self.registry.fetch_metadata(package.as_str()).await
            .map_err(|e| DependencyError(format!("npm metadata fetch failed for '{}': {}", package.as_str(), e)))?;
        let mut v: Vec<Version> = meta.versions.keys().filter_map(|k| Version::parse(k).ok()).collect();
        v.sort();
        Ok(v)
    }
    async fn get_dependencies(&self, package_id: &PackageId) -> Result<Vec<ResolvedDep>, DependencyError> {
        let meta = self.registry.fetch_metadata(package_id.name_str()).await
            .map_err(|e| DependencyError(format!("npm metadata fetch failed for '{}': {}", package_id.name_str(), e)))?;
        let deps = meta.versions.get(&package_id.version().to_string())
            .and_then(|v| v.dependencies.as_ref())
            .map(|deps| {
                deps.iter()
                    .filter_map(|(k, v)| PackageName::new(k).ok()
                        .map(|pn| ResolvedDep { package: pn, spec: v.clone(), optional: false, peer: false }))
                    .collect()
            })
            .unwrap_or_default();
        Ok(deps)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageJson {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "devDependencies")] pub dev_dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "peerDependencies")] pub peer_dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "optionalDependencies")] pub optional_dependencies: Option<std::collections::HashMap<String, String>>,
}
impl PackageJson {
    pub fn new(name: String, version: String) -> Self { Self { name, version, description: None, dependencies: None, dev_dependencies: None, peer_dependencies: None, optional_dependencies: None } }
    pub fn load(path: &Path) -> Result<Self, anyhow::Error> { Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?) }
    pub fn save(&self, path: &Path) -> Result<(), anyhow::Error> { Ok(std::fs::write(path, serde_json::to_string_pretty(self)?)?) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_web_adapter() { assert_eq!(WebAdapter::new().registry_url, "https://registry.npmjs.org"); }
    #[test] fn test_package_json() { let p = PackageJson::new("t".into(), "1.0.0".into()); assert_eq!(p.name, "t"); }
    #[test] fn test_can_handle() {
        let dir = tempfile::tempdir().unwrap();
        PackageJson::new("t".into(), "1.0.0".into()).save(&dir.path().join("package.json")).unwrap();
        assert!(WebAdapter::new().can_handle(dir.path()));
    }
}
