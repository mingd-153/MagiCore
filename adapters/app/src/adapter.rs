//! PackageAdapter implementation for app cores.
//! Giữ orchestration app riêng khỏi phần detect language và SBOM.

use crate::language::{detect_language, manifest_is_app, AppLanguage};
use async_trait::async_trait;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mgc_types::{
    Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, Version, VersionRange,
};
use std::path::{Path, PathBuf};

pub struct AppAdapter {
    pub language: AppLanguage,
}

pub fn adapter_for(root: &Path) -> Option<AppAdapter> {
    let language = detect_language(root)?;
    Some(AppAdapter { language })
}

fn no_package_manager() -> MgResult<()> {
    Err(mgc_types::MgError::Other(
        "app dependencies flow through provider tooling — install with `mgc install` (flutter pub get / gradle / swift package resolve)".to_string(),
    ))
}

#[async_trait]
impl PackageAdapter for AppAdapter {
    fn name(&self) -> &str {
        "app"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::App
    }

    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_app(project_root)
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        crate::manifest::parse_manifest(self.language, project_root)
    }

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        crate::manifest::write_manifest(self.language, project_root, manifest)
    }

    async fn resolve(&self, _manifest: &Manifest) -> MgResult<ResolvedGraph> {
        Ok(ResolvedGraph::default())
    }

    async fn fetch(&self, _graph: &ResolvedGraph) -> MgResult<()> {
        Ok(())
    }

    async fn install(
        &self,
        graph: &ResolvedGraph,
        project_root: &Path,
        opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        // Use new install pipeline
        crate::install::run_install(self.language, graph, project_root, opts, None).await
    }

    async fn add(
        &self,
        _project_root: &Path,
        _name: &PackageName,
        _range: Option<&VersionRange>,
        _opts: AddOptions,
    ) -> MgResult<PackageId> {
        no_package_manager()?;
        unreachable!()
    }

    async fn remove(&self, _project_root: &Path, _name: &PackageName) -> MgResult<()> {
        no_package_manager()
    }

    async fn update(
        &self,
        _project_root: &Path,
        _name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        no_package_manager()?;
        unreachable!()
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        let manifest = self.parse_manifest(project_root).await?;
        Ok(manifest
            .all_dependencies()
            .map(|dep| InstalledPackage {
                id: PackageId::new(
                    dep.name.clone(),
                    dep.range
                        .satisfying_version()
                        .unwrap_or_else(|| Version::new(0, 1, 0)),
                ),
                path: PathBuf::new(),
                integrity: None,
                is_direct: true,
                is_dev: dep.dev,
            })
            .collect())
    }

    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport> {
        let manifest = self.parse_manifest(project_root).await?;
        Ok(AuditReport::clean(manifest.all_dependencies().count()))
    }

    fn set_dedupe_pref(&self, _enabled: bool) {}

    fn set_existing_versions(&self, _versions: std::collections::HashMap<String, String>) {}
}
