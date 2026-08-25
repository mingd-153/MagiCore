//! PackageAdapter implementation for hardware add-ons.
//! Điều phối optimizer/bench cross-core ngoài registry package graph.

use crate::detection::manifest_is_any_mg;
use async_trait::async_trait;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mgc_types::{Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, Version};
use std::path::Path;

pub struct HardwareAdapter;

fn placeholder_id(name: &PackageName) -> PackageId {
    PackageId::new(name.clone(), Version::new(0, 1, 0))
}

#[async_trait]
impl PackageAdapter for HardwareAdapter {
    fn name(&self) -> &str {
        "hardware"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Hardware
    }

    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_any_mg(project_root)
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        let name = project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "hardware".to_string());
        Ok(Manifest::new(&name, Ecosystem::Hardware))
    }

    async fn write_manifest(&self, _project_root: &Path, _manifest: &Manifest) -> MgResult<()> {
        Ok(())
    }

    async fn resolve(&self, _manifest: &Manifest) -> MgResult<ResolvedGraph> {
        Ok(ResolvedGraph::default())
    }

    async fn fetch(&self, _graph: &ResolvedGraph) -> MgResult<()> {
        Ok(())
    }

    async fn install(
        &self,
        _graph: &ResolvedGraph,
        _project_root: &Path,
        _opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        Ok(InstallSummary::default())
    }

    async fn add(
        &self,
        _project_root: &Path,
        _name: &PackageName,
        _range: Option<&mgc_types::VersionRange>,
        _opts: AddOptions,
    ) -> MgResult<PackageId> {
        Err(mgc_types::MgError::Other(
            "hardware packages (optimizer/bench) are materialized by `mgc add-hardware <pkg>` — not via the registry".to_string(),
        ))
    }

    async fn remove(&self, _project_root: &Path, _name: &PackageName) -> MgResult<()> {
        Err(mgc_types::MgError::Other(
            "hardware packages do not go through the registry — remove the optimizer/bench folder manually"
                .to_string(),
        ))
    }

    async fn update(
        &self,
        _project_root: &Path,
        _name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        Ok(vec![])
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        let mut pkgs = Vec::new();
        for sub in ["optimizer", "bench"] {
            if project_root.join(sub).exists() {
                pkgs.push(InstalledPackage {
                    id: placeholder_id(&PackageName::new(sub)?),
                    path: project_root.join(sub),
                    integrity: None,
                    is_direct: true,
                    is_dev: false,
                });
            }
        }
        Ok(pkgs)
    }

    async fn audit(&self, _project_root: &Path) -> MgResult<AuditReport> {
        Ok(AuditReport::clean(0))
    }
}

pub fn adapter_for(root: &Path) -> Option<HardwareAdapter> {
    manifest_is_any_mg(root).then_some(HardwareAdapter)
}
