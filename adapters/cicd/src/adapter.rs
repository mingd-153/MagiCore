//! PackageAdapter implementation for CI/CD cores.
//! Điều phối dependency flow fail-closed riêng khỏi provider detection.
//!
//! Global Gate 1 (2026-09-16): the whole registry-lifecycle surface that
//! always failed (resolve/fetch/install/write_manifest/add/remove/update)
//! is GONE — the fail-closed defaults from `mgc_types::capabilities`
//! answer with the same `MgError::Unsupported` style. CI/CD keeps
//! detection, listing, and its own audit lane.
//! Global Gate 1: toàn bộ mặt registry-lifecycle vốn luôn lỗi
//! (resolve/fetch/install/write_manifest/add/remove/update) đã BỊ XÓA —
//! default fail-closed trả lời cùng style `MgError::Unsupported`. Cicd
//! giữ detect, list và lane audit riêng.

use crate::provider::{CicdProvider, detect_provider, manifest_is_cicd};
use async_trait::async_trait;
use mgc_types::adapter::{AuditReport, InstalledPackage, PackageAdapter};
use mgc_types::capabilities::{
    AuditProvider, Capability, CoreIdent, ProjectDetector, ScaffoldProvider,
};
use mgc_types::{Ecosystem, Manifest, MgResult, PackageId, Version};
use std::path::{Path, PathBuf};

pub struct CicdAdapter {
    pub provider: CicdProvider,
}

impl CicdAdapter {
    /// Capability manifest (Global Gate 1) — code-reality notes:
    /// - ProjectDetector: `manifest_is_cicd`/`detect_provider` — real.
    /// - ScaffoldProvider: `mgc create-cicd <provider>` (the scaffold lane
    ///   the old write_manifest guidance pointed at) — real.
    /// - AuditProvider: the github-actions policy lane + polyglot engine
    ///   (audit body below) — real.
    ///
    /// DependencyResolver/ArtifactFetcher/ContentStoreProvider/
    /// LockfileProvider are NOT claimed — pipeline files are hand-owned
    /// and `mgc deploy` (dry-run default) is the CLI lane, not the
    /// adapter surface.
    /// Bảng capability (Global Gate 1) — ghi chú theo code thật.
    pub const CAPABILITIES: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::AuditProvider,
    ];
}

pub fn adapter_for(root: &Path) -> Option<CicdAdapter> {
    let provider = detect_provider(root)?;
    Some(CicdAdapter { provider })
}

impl CoreIdent for CicdAdapter {
    fn core_id(&self) -> &'static str {
        "cicd"
    }

    fn name(&self) -> &str {
        "cicd"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Cicd
    }
}

impl ProjectDetector for CicdAdapter {
    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_cicd(project_root)
    }
}

impl ScaffoldProvider for CicdAdapter {
    /// Evidence: `mgc create-cicd <provider>` scaffold lane.
    /// Dẫn chứng: lane scaffold `mgc create-cicd <provider>`.
    fn probe_scaffold(&self) -> MgResult<()> {
        Ok(())
    }
}

#[async_trait]
impl AuditProvider for CicdAdapter {
    /// Evidence: the github-actions policy lane + polyglot engine below.
    /// Dẫn chứng: lane policy github-actions + engine polyglot bên dưới.
    fn probe_audit_provider(&self) -> MgResult<()> {
        Ok(())
    }

    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport> {
        // P2 2026-09-10: cicd owns its TARGET-ecosystem lane — GitHub
        // Actions SHA-pinning/permissions/injection policy — plus the
        // polyglot engine for sibling dependency manifests.
        // Cicd có lane ecosystem ĐÍCH — policy SHA-pinning/permissions/
        // injection của GitHub Actions — cộng engine polyglot cho
        // manifest dependency kề.
        let mut plan = mgc_audit::plan_for_shared_manifests(project_root)?;
        let has_workflows = project_root.join(".github").join("workflows").is_dir();
        if has_workflows {
            let root = project_root.to_path_buf();
            plan.add_step(mgc_audit::ScanStep {
                ecosystem: "cicd/github-actions",
                scanner: "github-actions-policy",
                run: Box::new(move || {
                    let root = root.clone();
                    Box::pin(async move { mgc_audit::scanners::audit_github_actions(&root) })
                }),
            });
        }
        let manifest = self.parse_manifest(project_root).await?;
        let label = format!(
            "cicd ({} dependencies not scanned — no scanner implemented yet)",
            manifest.all_dependencies().count()
        );
        if plan.is_empty() {
            return Ok(mgc_types::adapter::AuditReport::unsupported_ecosystem(
                label,
            ));
        }
        plan.execute().await
    }
}

#[async_trait]
impl PackageAdapter for CicdAdapter {
    fn capabilities(&self) -> &'static [Capability] {
        Self::CAPABILITIES
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        let name = project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "ci".to_string());
        Ok(Manifest::new(&name, Ecosystem::Cicd))
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
}

impl CicdAdapter {
    pub fn provider(&self) -> &'static str {
        self.provider.as_str()
    }
}

// Unclaimed capabilities — empty impls inherit the fail-closed
// Unsupported probes/defaults from mgc_types::capabilities.
// Capability chưa claim — impl rỗng kế thừa probe/default fail-closed
// từ mgc_types::capabilities.
impl mgc_types::capabilities::DependencyResolver for CicdAdapter {}
impl mgc_types::capabilities::ArtifactFetcher for CicdAdapter {}
impl mgc_types::capabilities::ContentStoreProvider for CicdAdapter {}
impl mgc_types::capabilities::LockfileProvider for CicdAdapter {}
impl mgc_types::capabilities::LifecycleRunner for CicdAdapter {}
impl mgc_types::capabilities::OptimizerProvider for CicdAdapter {}
impl mgc_types::capabilities::Materializer for CicdAdapter {}
impl mgc_types::capabilities::SimulatorProvider for CicdAdapter {}
impl mgc_types::capabilities::DeviceProvider for CicdAdapter {}
impl mgc_types::capabilities::DeployProvider for CicdAdapter {}
impl mgc_types::capabilities::ModelRuntimeProvider for CicdAdapter {}
