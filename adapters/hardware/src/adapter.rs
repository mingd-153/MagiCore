//! PackageAdapter implementation for hardware add-ons.
//! Điều phối optimizer/bench cross-core ngoài registry package graph.
//!
//! Global Gate 1 (2026-09-16): the whole registry surface that always
//! failed (resolve/fetch/install/add/remove/update/write_manifest) is
//! GONE — the fail-closed `MgError::Unsupported` defaults from
//! `mgc_types::capabilities` answer now, with the same message style.
//! Hardware remains a scaffold/generator-only core: `mgc add-hardware`
//! materializes optimizer/bench templates outside the PackageAdapter.
//! Global Gate 1: toàn bộ mặt registry vốn luôn lỗi
//! (resolve/fetch/install/add/remove/update/write_manifest) đã BỊ XÓA —
//! default `MgError::Unsupported` fail-closed từ
//! `mgc_types::capabilities` trả lời cùng style message. Hardware vẫn là
//! core chỉ-scaffold/generator: `mgc add-hardware` materialize template
//! optimizer/bench ngoài PackageAdapter.

use crate::detection::manifest_is_any_mg;
use async_trait::async_trait;
use mgc_types::adapter::{AuditReport, InstalledPackage, PackageAdapter};
use mgc_types::capabilities::{
    AuditProvider, Capability, CoreIdent, OptimizerProvider, ProjectDetector,
};
use mgc_types::{Ecosystem, Manifest, MgResult, PackageId, PackageName, Version};
use std::path::Path;

pub struct HardwareAdapter;

impl HardwareAdapter {
    /// Capability manifest (Global Gate 1) — code-reality notes:
    /// - ProjectDetector: `manifest_is_any_mg` (any mgc.toml) — real.
    /// - OptimizerProvider: the optimizer/bench framework — template
    ///   materialization via `mgc add-hardware`/`create-hardware` and the
    ///   optimizer/bench listing below — real (mgc-owned generator).
    /// - AuditProvider: shared polyglot dispatch (sibling manifests get
    ///   real scans) — real.
    ///
    /// Everything registry-shaped is NOT claimed — every previous override
    /// returned an error; `list` only reports materialized templates.
    /// Bảng capability (Global Gate 1) — ghi chú theo code thật.
    pub const CAPABILITIES: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::OptimizerProvider,
        Capability::AuditProvider,
    ];
}

fn placeholder_id(name: &PackageName) -> PackageId {
    PackageId::new(name.clone(), Version::new(0, 1, 0))
}

impl CoreIdent for HardwareAdapter {
    fn core_id(&self) -> &'static str {
        "hardware"
    }

    fn name(&self) -> &str {
        "hardware"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Hardware
    }
}

impl ProjectDetector for HardwareAdapter {
    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_any_mg(project_root)
    }
}

impl OptimizerProvider for HardwareAdapter {
    /// Evidence: optimizer/bench packages are materialized by the mgc
    /// generator (`mgc add-hardware <pkg>` — the old add/remove guidance)
    /// and listed below; `mgc optimize`/`bench` run the mgc harness.
    /// Dẫn chứng: package optimizer/bench được materialize bởi generator
    /// mgc (`mgc add-hardware <pkg>`) và liệt kê bên dưới;
    /// `mgc optimize`/`bench` chạy harness của mgc.
    fn probe_optimizer(&self) -> MgResult<()> {
        Ok(())
    }
}

#[async_trait]
impl AuditProvider for HardwareAdapter {
    /// Evidence: shared-engine polyglot dispatch below (sibling Rust/Go
    /// manifests get real scans; pure HDL stays honestly unsupported).
    /// Dẫn chứng: dispatch polyglot bên dưới (manifest kề Rust/Go được
    /// quét thật; thuần HDL giữ unsupported trung thực).
    fn probe_audit_provider(&self) -> MgResult<()> {
        Ok(())
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

#[async_trait]
impl PackageAdapter for HardwareAdapter {
    fn capabilities(&self) -> &'static [Capability] {
        Self::CAPABILITIES
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        let name = project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "hardware".to_string());
        Ok(Manifest::new(&name, Ecosystem::Hardware))
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
}

pub fn adapter_for(root: &Path) -> Option<HardwareAdapter> {
    manifest_is_any_mg(root).then_some(HardwareAdapter)
}

// Unclaimed capabilities — empty impls inherit the fail-closed
// Unsupported probes/defaults from mgc_types::capabilities.
// Capability chưa claim — impl rỗng kế thừa probe/default fail-closed
// từ mgc_types::capabilities.
impl mgc_types::capabilities::ScaffoldProvider for HardwareAdapter {}
impl mgc_types::capabilities::DependencyResolver for HardwareAdapter {}
impl mgc_types::capabilities::ArtifactFetcher for HardwareAdapter {}
impl mgc_types::capabilities::ContentStoreProvider for HardwareAdapter {}
impl mgc_types::capabilities::LockfileProvider for HardwareAdapter {}
impl mgc_types::capabilities::LifecycleRunner for HardwareAdapter {}
impl mgc_types::capabilities::Materializer for HardwareAdapter {}
impl mgc_types::capabilities::SimulatorProvider for HardwareAdapter {}
impl mgc_types::capabilities::DeviceProvider for HardwareAdapter {}
impl mgc_types::capabilities::DeployProvider for HardwareAdapter {}
impl mgc_types::capabilities::ModelRuntimeProvider for HardwareAdapter {}
