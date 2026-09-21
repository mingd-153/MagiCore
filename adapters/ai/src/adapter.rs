//! PackageAdapter implementation for AI cores.
//! Điều phối fail-closed dependency flow riêng khỏi framework detection.
//!
//! Global Gate 1 (2026-09-16): the registry-lifecycle surface that always
//! failed (resolve/fetch/install/add/remove/update) and the fake-success
//! write_manifest no-op are GONE — the fail-closed defaults from
//! `mgc_types::capabilities` answer now. AI deps flow through the CLI
//! uv/pip lane; the adapter keeps detection, listing, audit, and the
//! model-artifact scanner.
//! Global Gate 1: các method registry-lifecycle vốn luôn lỗi
//! (resolve/fetch/install/add/remove/update) và no-op write_manifest giả
//! thành công đã BỊ XÓA — default fail-closed từ
//! `mgc_types::capabilities` trả lời thay. Dep AI đi qua lane uv/pip của
//! CLI; adapter giữ detect, list, audit và scanner model-artifact.

use crate::framework::{AiFramework, detect_framework};
use async_trait::async_trait;
use mgc_types::adapter::{AuditReport, InstalledPackage, PackageAdapter};
use mgc_types::capabilities::{
    AuditProvider, Capability, CoreIdent, LifecycleRunner, OptimizerProvider, ProjectDetector,
    ScaffoldProvider,
};
use mgc_types::{Ecosystem, Manifest, MgError, MgResult, PackageId, PackageName, Version};
use std::path::{Path, PathBuf};

pub struct AiAdapter {
    pub framework: AiFramework,
}

impl AiAdapter {
    /// Capability manifest (Global Gate 1) — code-reality notes:
    /// - ProjectDetector: `detect_framework` (framework.rs) — real.
    /// - ScaffoldProvider: the `mgc create-ai` CLI scaffolder lane — real.
    /// - LifecycleRunner: the uv/pip install lifecycle runs through the
    ///   CLI per-core lane (cli/src/commands/core/install/ai.rs) — real.
    /// - OptimizerProvider: `mgc optimize` supports the ai core
    ///   (cli/src/commands/optimizer) — real.
    /// - AuditProvider: dependency scanners (pip-audit/cargo-audit/
    ///   govulncheck) + the model-artifact scanner — real.
    ///
    /// DependencyResolver/ArtifactFetcher/ContentStoreProvider are
    /// deliberately NOT claimed: mgc does not manage virtualenvs
    /// (adapters/ai/src/adapter.rs previously failed closed via
    /// `no_package_manager`) — the defaults answer now.
    /// Bảng capability (Global Gate 1) — ghi chú theo code thật; các
    /// capability registry KHÔNG được claim vì mgc không quản virtualenv.
    pub const CAPABILITIES: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::LifecycleRunner,
        Capability::OptimizerProvider,
        Capability::AuditProvider,
    ];
}

pub fn adapter_for(root: &Path) -> Option<AiAdapter> {
    let framework = detect_framework(root)?;
    Some(AiAdapter { framework })
}

impl CoreIdent for AiAdapter {
    fn core_id(&self) -> &'static str {
        "ai"
    }

    fn name(&self) -> &str {
        "ai"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Ai
    }
}

impl ProjectDetector for AiAdapter {
    fn can_handle(&self, project_root: &Path) -> bool {
        detect_framework(project_root).is_some()
    }
}

impl ScaffoldProvider for AiAdapter {
    /// Evidence: `mgc create-ai` scaffold lane (CLI create commands).
    /// Dẫn chứng: lane scaffold `mgc create-ai` (lệnh create của CLI).
    fn probe_scaffold(&self) -> MgResult<()> {
        Ok(())
    }
}

impl LifecycleRunner for AiAdapter {
    /// Evidence: the uv/pip install lifecycle (lock → sync) runs in the
    /// CLI per-core install lane (cli/src/commands/core/install/ai.rs).
    /// Dẫn chứng: lifecycle install uv/pip (lock → sync) chạy ở lane
    /// install per-core của CLI.
    fn probe_lifecycle_runner(&self) -> MgResult<()> {
        Ok(())
    }
}

impl OptimizerProvider for AiAdapter {
    /// Evidence: `mgc optimize` dispatches the ai core
    /// (cli/src/commands/optimizer — Ecosystem::Ai branch).
    /// Dẫn chứng: `mgc optimize` điều phối core ai (optimizer CLI).
    fn probe_optimizer(&self) -> MgResult<()> {
        Ok(())
    }
}

#[async_trait]
impl AuditProvider for AiAdapter {
    /// Evidence: the two-layer aggregate below (dependency scanners +
    /// model-artifact scanner). Dẫn chứng: aggregate hai lớp bên dưới
    /// (scanner dependency + scanner model-artifact).
    fn probe_audit_provider(&self) -> MgResult<()> {
        Ok(())
    }

    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport> {
        // Dependency lanes (R1): shared constructors — the same
        // detection rule every core uses. The python rule is the broad
        // shared one (requirements variants, pylock, uv.lock,
        // pyproject): recognized-but-unscannable manifests surface as
        // the scanner's honest Failed, never a hidden skip.
        // Lane dependency: constructor chung — cùng luật mọi core.
        let mut plan = mgc_audit::AuditPlan::new();
        for step in [
            mgc_audit::python_step(project_root),
            mgc_audit::rust_step(project_root),
            mgc_audit::go_step(project_root),
        ]
        .into_iter()
        .flatten()
        {
            plan.add_step(step);
        }

        // Model artifact layer: scan the AI model directory conventions.
        // Lớp model artifact: scan theo thư mục model quen thuộc của AI.
        for model_dir in ["models", "model", "artifacts", "checkpoints"] {
            let dir = project_root.join(model_dir);
            if dir.is_dir() {
                let dir = dir.clone();
                plan.add_step(mgc_audit::ScanStep {
                    ecosystem: "model-artifact",
                    scanner: "mgc-model-scanner",
                    run: Box::new(move || {
                        let dir = dir.clone();
                        Box::pin(async move { model_artifact_report(&dir) })
                    }),
                });
                break;
            }
        }

        if plan.is_empty() {
            let manifest = self.parse_manifest(project_root).await?;
            return Ok(AuditReport::unsupported_ecosystem(format!(
                "ai ({} dependencies not scanned — no scanner implemented yet)",
                manifest.all_dependencies().count()
            )));
        }

        plan.execute().await
    }
}

#[async_trait]
impl PackageAdapter for AiAdapter {
    fn capabilities(&self) -> &'static [Capability] {
        Self::CAPABILITIES
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        let name = project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "ai".to_string());
        Ok(Manifest::new(&name, Ecosystem::Ai))
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
// Unclaimed capabilities — empty impls inherit the fail-closed
// Unsupported probes/defaults from mgc_types::capabilities.
// Capability chưa claim — impl rỗng kế thừa probe/default fail-closed
// từ mgc_types::capabilities.
impl mgc_types::capabilities::DependencyResolver for AiAdapter {}
impl mgc_types::capabilities::ArtifactFetcher for AiAdapter {}
impl mgc_types::capabilities::ContentStoreProvider for AiAdapter {}
impl mgc_types::capabilities::LockfileProvider for AiAdapter {}
impl mgc_types::capabilities::Materializer for AiAdapter {}
impl mgc_types::capabilities::SimulatorProvider for AiAdapter {}
impl mgc_types::capabilities::DeviceProvider for AiAdapter {}
impl mgc_types::capabilities::DeployProvider for AiAdapter {}
impl mgc_types::capabilities::ModelRuntimeProvider for AiAdapter {}

/// Convert the model-artifact audit (internal Finding format) into the
/// unified AuditReport — every High/Critical model finding becomes a
/// Vulnerability row so the aggregate and exit contract stay uniform.
/// Chuyển audit model-artifact (Finding nội bộ) sang AuditReport thống
/// nhất — mọi finding High/Critical của model thành dòng Vulnerability
/// để aggregate và exit contract giữ một chuẩn.
fn model_artifact_report(dir: &Path) -> MgResult<AuditReport> {
    use mgc_types::adapter::{FindingClass, Vulnerability, VulnerabilitySeverity};

    let model = futures_util_replay(dir)?;
    let mut vulnerabilities = Vec::new();
    for finding in &model.findings {
        let severity_level = match finding.severity {
            crate::audit::Severity::Critical => VulnerabilitySeverity::Critical,
            crate::audit::Severity::High => VulnerabilitySeverity::High,
            crate::audit::Severity::Medium => VulnerabilitySeverity::Medium,
            crate::audit::Severity::Low => VulnerabilitySeverity::Low,
            crate::audit::Severity::Info => VulnerabilitySeverity::Info,
        };
        let file = finding
            .file_path
            .clone()
            .unwrap_or_else(|| dir.display().to_string());
        vulnerabilities.push(
            Vulnerability {
                package: PackageId::new(
                    PackageName::new(format!("model:{file}"))
                        .map_err(|e| MgError::Other(format!("invalid model label: {e}")))?,
                    Version::new(0, 0, 0),
                ),
                title: format!("{}: {}", finding.category, finding.message),
                severity: format!("{:?}", finding.severity).to_lowercase(),
                cve: format!("model-{}", finding.category),
                severity_level,
                patched_versions: None,
                url: None,
                scanner: None,
                ecosystem: None,
                evidence_at: None,
                finding_class: FindingClass::Artifact,
            }
            .with_evidence("mgc-model-scanner", "model-artifact"),
        );
    }

    Ok(AuditReport {
        packages_audited: model.scanned_files,
        vulnerability_count: vulnerabilities.len(),
        vulnerabilities,
        scanner_status: mgc_types::adapter::ScannerStatus::Available,
    })
}

/// Poll the async model audit once — same first-poll-complete contract
/// as the dependency scanners.
/// Poll audit model async đúng một lần — cùng hợp đồng poll-đầu-hoàn-
/// tất như các scanner dependency.
fn futures_util_replay(dir: &Path) -> MgResult<crate::audit::AuditReport> {
    use futures_util::future::FutureExt;
    match Box::pin(crate::audit::audit_model(dir)).now_or_never() {
        Some(result) => result,
        None => Err(mgc_types::MgError::Other(
            "model scanner requires async execution".to_string(),
        )),
    }
}
