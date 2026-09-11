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
        // Fail-closed: hardware projects are scaffold-owned, not registry
        // manifests — writing a no-op success would fake capability.
        // Fail-closed: project hardware do scaffold quản lý, không phải
        // registry manifest — trả Ok giả là giả capability.
        Err(mgc_types::MgError::Unsupported {
            core: "hardware",
            capability: "write_manifest",
            guidance: "hardware projects do not use registry manifests; \
                       regenerate via `mgc create-hardware` or edit optimizer/bench files directly"
                .to_string(),
        })
    }

    async fn resolve(&self, _manifest: &Manifest) -> MgResult<ResolvedGraph> {
        // No dependency graph exists for hardware add-ons — fail closed.
        // Hardware add-on không có dependency graph — fail-closed.
        Err(mgc_types::MgError::Unsupported {
            core: "hardware",
            capability: "resolve",
            guidance: "hardware add-ons have no registry dependency graph; \
                       optimizer/bench templates are materialized by `mgc add-hardware <pkg>`"
                .to_string(),
        })
    }

    async fn fetch(&self, _graph: &ResolvedGraph) -> MgResult<()> {
        Err(mgc_types::MgError::Unsupported {
            core: "hardware",
            capability: "fetch",
            guidance: "nothing to fetch — hardware add-ons are templates, \
                       not registry packages"
                .to_string(),
        })
    }

    async fn install(
        &self,
        _graph: &ResolvedGraph,
        _project_root: &Path,
        _opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        Err(mgc_types::MgError::Unsupported {
            core: "hardware",
            capability: "install",
            guidance: "hardware add-ons are materialized by `mgc add-hardware <pkg>`; \
                       there is no registry install for this core"
                .to_string(),
        })
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
        // Fail-closed: no update channel exists for template add-ons.
        // Fail-closed: template add-on không có kênh update.
        Err(mgc_types::MgError::Unsupported {
            core: "hardware",
            capability: "update",
            guidance: "hardware add-ons have no update channel; \
                       re-run `mgc add-hardware <pkg>` to refresh templates"
                .to_string(),
        })
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

    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport> {
        // Shared-engine polyglot dispatch (P2): template packages carry
        // no dependency graph of their own, but sibling manifests (a
        // Rust tooling crate next to the HDL) get real scans; pure HDL
        // projects stay honestly unsupported.
        // Dispatch polyglot qua engine chung: template package không có
        // dependency graph riêng, nhưng manifest kề bên (crate Rust công
        // cụ cạnh HDL) được quét thật; project thuần HDL giữ trung thực
        // unsupported.
        mgc_audit::audit_polyglot(
            project_root,
            "hardware (template packages only — no scanner implemented yet)".to_string(),
        )
        .await
    }
}

pub fn adapter_for(root: &Path) -> Option<HardwareAdapter> {
    manifest_is_any_mg(root).then_some(HardwareAdapter)
}
