//! PackageAdapter implementation for AI cores.
//! Điều phối fail-closed dependency flow riêng khỏi framework detection.

use crate::framework::{AiFramework, detect_framework};
use async_trait::async_trait;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mgc_types::{
    Ecosystem, Manifest, MgError, MgResult, PackageId, PackageName, ResolvedGraph, Version,
    VersionRange,
};
use std::path::{Path, PathBuf};

pub struct AiAdapter {
    pub framework: AiFramework,
}

pub fn adapter_for(root: &Path) -> Option<AiAdapter> {
    let framework = detect_framework(root)?;
    Some(AiAdapter { framework })
}

fn no_package_manager() -> MgResult<()> {
    Err(mgc_types::MgError::Other(
        "ai deps flow through pip (allowlist) — run `pip install -r requirements.txt` manually; mgc does not manage virtualenvs".to_string(),
    ))
}

#[async_trait]
impl PackageAdapter for AiAdapter {
    fn name(&self) -> &str {
        "ai"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Ai
    }

    fn can_handle(&self, project_root: &Path) -> bool {
        detect_framework(project_root).is_some()
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        let name = project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "ai".to_string());
        Ok(Manifest::new(&name, Ecosystem::Ai))
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
        no_package_manager()?;
        unreachable!()
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
        // AI two-layer aggregate (Tech Lead 2026-09-09 §3): dependency
        // audit (python/rust manifests when present) + model artifact
        // audit (pickle/safetensors/weights) — one merged report.
        // Aggregate hai lớp AI: dependency (manifest python/rust nếu có)
        // + model artifact (pickle/safetensors/weights) — một report gộp.
        let mut plan = mgc_audit::AuditPlan::new();

        let requirements = project_root.join("requirements.txt");
        if requirements.is_file() {
            let root = project_root.to_path_buf();
            plan.add_step(mgc_audit::ScanStep {
                ecosystem: "python",
                scanner: "pip-audit",
                run: Box::new(move || run_now(mgc_audit::scanners::audit_python(&root))),
            });
        }
        if project_root.join("Cargo.toml").is_file() {
            let root = project_root.to_path_buf();
            plan.add_step(mgc_audit::ScanStep {
                ecosystem: "rust",
                scanner: "cargo-audit",
                run: Box::new(move || run_now(mgc_audit::scanners::audit_rust(&root))),
            });
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
                    run: Box::new(move || model_artifact_report(&dir)),
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

    fn set_dedupe_pref(&self, _enabled: bool) {}

    fn set_existing_versions(&self, _versions: std::collections::HashMap<String, String>) {}
}

/// Drive a scanner future to completion inside a sync engine step —
/// the current scanners complete on first poll (subprocess work happens
/// in mgc-exec during that poll).
/// Chạy trọn future scanner trong bước engine sync — scanner hiện tại
/// hoàn tất ngay poll đầu (subprocess chạy trong mgc-exec lúc đó).
fn run_now<F>(fut: F) -> MgResult<AuditReport>
where
    F: std::future::Future<Output = MgResult<AuditReport>>,
{
    use futures_util::future::FutureExt;
    match Box::pin(fut).now_or_never() {
        Some(result) => result,
        None => Err(mgc_types::MgError::Other(
            "ai aggregate scanner requires async execution".to_string(),
        )),
    }
}

/// Convert the model-artifact audit (internal Finding format) into the
/// unified AuditReport — every High/Critical model finding becomes a
/// Vulnerability row so the aggregate and exit contract stay uniform.
/// Chuyển audit model-artifact (Finding nội bộ) sang AuditReport thống
/// nhất — mọi finding High/Critical của model thành dòng Vulnerability
/// để aggregate và exit contract giữ một chuẩn.
fn model_artifact_report(dir: &Path) -> MgResult<AuditReport> {
    use mgc_types::adapter::{Vulnerability, VulnerabilitySeverity};

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
