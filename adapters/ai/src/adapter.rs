//! PackageAdapter implementation for AI cores.
//! Điều phối fail-closed dependency flow riêng khỏi framework detection.
//!
//! Global Gate 1 (2026-09-16, cập nhật 2026-09-23): python deps của ai
//! đi qua NATIVE PyPI pipeline (cùng engine lib/python — resolve →
//! verified fetch → CAS → mgc.lock, zero uv/pip spawn). Adapter nhúng
//! engine đó (`self.python_lane`); CLI add/remove/update đi qua mutation
//! gateway có journal, còn direct adapter mutators fail-closed.
//! Foreign-lock projects không được fallback sang toolchain package manager.
//! Global Gate 1: ai python deps ride the NATIVE PyPI pipeline (the
//! lib/python engine — resolve → verified fetch → CAS → mgc.lock, zero
//! uv/pip spawn). The adapter embeds that engine (`self.python_lane`);
//! CLI mutations pass through the journaled gateway, while direct adapter
//! mutators fail closed. Foreign-lock projects never fall back to a PM.

use crate::framework::{AiFramework, detect_framework};
use async_trait::async_trait;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    PreparedAdd,
};
use mgc_types::capabilities::LifecycleRunner;
use mgc_types::capabilities::{
    AuditProvider, Capability, CoreIdent, OptimizerProvider, ProjectDetector, ScaffoldProvider,
};
use mgc_types::{
    Ecosystem, Manifest, MgError, MgResult, PackageId, PackageName, ResolvedGraph, Version,
    VersionRange,
};
use std::path::Path;

pub struct AiAdapter {
    pub framework: AiFramework,
    /// Native PyPI lane — present ONLY for pyproject.toml projects, where
    /// python deps ride the lib/python engine (resolve → verified fetch →
    /// CAS → mgc.lock, zero uv/pip spawn). Existing uv.lock/requirements.lock
    /// projects have NO lane here until the user migrates lock ownership.
    /// (Lane PyPI native chỉ bật khi không chiếm lockfile toolchain.)
    pub python_lane: Option<mgc_lib_adapter::LibAdapter>,
}

/// Select the native PyPI lane only when MGC can own the lock lifecycle
/// without taking over an existing uv/pip lockfile. A project with an
/// existing foreign lock needs an explicit migration before ownership
/// changes. (Chỉ chọn lane PyPI native khi không chiếm lockfile toolchain.)
pub fn uses_native_python_lane(root: &Path) -> bool {
    mgc_lib_adapter::supports_native_python_project(root)
}

impl AiAdapter {
    /// Capability manifest (Global Gate 1) — code-reality notes:
    /// - ProjectDetector: `detect_framework` (framework.rs) — real.
    /// - ScaffoldProvider: the `mgc create-ai` CLI scaffolder lane — real.
    /// - LifecycleRunner: the install lifecycle runs through the CLI
    ///   per-core lane (cli/src/commands/core/install/ai.rs) — real.
    /// - OptimizerProvider: `mgc optimize` supports the ai core
    ///   (cli/src/commands/optimizer) — real.
    /// - AuditProvider: dependency scanners (pip-audit/cargo-audit/
    ///   govulncheck) + the model-artifact scanner — real.
    /// - CAPABILITIES_PYPI (pyproject.toml projects only):
    ///   DependencyResolver/LockfileProvider/ContentStoreProvider are real through the embedded native PyPI
    ///   engine; CLI add/remove/update edit the same MGC-owned manifest.
    ///
    /// Bảng capability (Global Gate 1) — ghi chú theo code thật;
    /// CAPABILITIES_PYPI (chỉ project pyproject.toml): resolve/lock/
    /// store THẬT qua engine PyPI native nhúng; CLI add/remove/update
    /// sửa manifest do MGC sở hữu. Virtualenv/interpreter không thuộc MGC.
    pub const CAPABILITIES: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::LifecycleRunner,
        Capability::OptimizerProvider,
        Capability::AuditProvider,
    ];

    /// PyPI lane (pyproject.toml only): registry capabilities claimed
    /// because the embedded lib/python engine owns them natively.
    /// (Lane PyPI (chỉ pyproject.toml): claim registry vì engine
    /// lib/python nhúng sở hữu chúng native.)
    pub const CAPABILITIES_PYPI: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::DependencyResolver,
        Capability::LockfileProvider,
        Capability::ArtifactFetcher,
        Capability::ContentStoreProvider,
        Capability::LifecycleRunner,
        Capability::OptimizerProvider,
        Capability::AuditProvider,
    ];
}

pub fn adapter_for(root: &Path) -> Option<AiAdapter> {
    let framework = detect_framework(root)?;
    // The lib python engine constructor is infallible for non-TS
    // languages (no web engine is built) — an error here is a logic bug,
    // so fail loudly rather than silently dropping the native engine.
    // (Constructor engine python của lib không thể lỗi với ngôn ngữ
    // non-TS — lỗi ở đây là bug logic, fail to rather than bỏ im lặng.)
    let python_lane = if uses_native_python_lane(root) {
        Some(
            mgc_lib_adapter::adapter_for_language(
                mgc_lib_adapter::LibLanguage::Python,
                root,
                None,
                None,
            )
            .expect("lib python engine construction is infallible"),
        )
    } else {
        None
    };
    Some(AiAdapter {
        framework,
        python_lane,
    })
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
    /// The AI core has CLI-level lifecycle phases. This marker does not
    /// claim dependency ownership; resolver/fetch/lock/store capabilities
    /// are dynamic and appear only for the native PyPI lane.
    /// (Core AI có lifecycle ở CLI; marker này không claim quyền quản lý
    /// dependency. Capability resolver/fetch/lock/store chỉ có ở lane PyPI.)
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
        if self.python_lane.is_some() {
            Self::CAPABILITIES_PYPI
        } else {
            Self::CAPABILITIES
        }
    }

    fn supports_native_update(&self) -> bool {
        self.python_lane.is_some()
    }

    fn manifest_identity(&self) -> Option<mgc_types::ManifestIdentity> {
        self.python_lane
            .as_ref()
            .map(|_| mgc_types::ManifestIdentity {
                core: "ai".to_string(),
                language: "python".to_string(),
                format: "pyproject.toml".to_string(),
                relpath: "pyproject.toml".to_string(),
            })
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        // PyPI lane: the embedded python engine parses pyproject.toml
        // REALLY (project name + deps). Non-lane projects (uv.lock /
        // requirements-only): the bare name-only manifest, as before.
        // (Lane PyPI: engine python nhúng parse pyproject.toml THẬT.
        // Không lane: manifest trần chỉ tên, như trước.)
        if let Some(py) = &self.python_lane {
            return py.parse_manifest(project_root).await;
        }
        let name = project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "ai".to_string());
        Ok(Manifest::new(&name, Ecosystem::Ai))
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        // PyPI lane: versions come from the engine's REAL resolution/lock
        // state. Foreign-lock projects are not represented as an empty or
        // guessed installed set; they remain unsupported until migrated.
        // (Lane PyPI: version lấy từ lock thật. Foreign-lock không bị giả
        // thành danh sách rỗng hoặc version đoán.)
        if let Some(py) = &self.python_lane {
            return py.list(project_root).await;
        }
        Err(mgc_types::capabilities::unsupported_capability(
            "ai",
            "list",
            "this project uses a foreign Python lock/manifest; migrate it to an MGC-owned pyproject before using native list",
        ))
    }

    /// PyPI lane: delegate to the embedded python engine — resolve-first
    /// (same contract as lib), so no fake-add ever persists. Non-lane:
    /// fail-closed until MGC owns the Python manifest.
    /// (Lane PyPI: chuẩn bị qua engine nhúng, chưa ghi file. Không lane:
    /// fail-closed tới khi MGC sở hữu manifest Python.)
    async fn prepare_add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PreparedAdd> {
        match &self.python_lane {
            Some(py) => py.prepare_add(project_root, name, range, opts).await,
            None => Err(mgc_types::capabilities::unsupported_capability(
                "ai",
                "prepare-add",
                "no pyproject.toml — native AI dependency edits require an MGC-owned pyproject; foreign lockfiles are unsupported",
            )),
        }
    }

    /// P0/F6: the embedded engine's gate arms from the same project root.
    /// (Cổng tuổi của engine nhúng nạp theo cùng project root.)
    fn arm_age_gate_for(&self, project_root: &Path) -> MgResult<()> {
        if let Some(py) = &self.python_lane {
            return py.arm_age_gate_for(project_root);
        }
        Ok(())
    }
}
// Native registry-lifecycle capabilities are forwarded to the embedded
// Python engine; unsupported project layouts fail closed.
// Engine Python nhúng xử lý lifecycle native; layout chưa hỗ trợ bị từ chối.
#[async_trait]
impl mgc_types::capabilities::DependencyResolver for AiAdapter {
    fn probe_dependency_resolver(&self) -> MgResult<()> {
        match &self.python_lane {
            Some(py) => py.probe_dependency_resolver(),
            None => Err(mgc_types::capabilities::unsupported_capability(
                "ai",
                "resolve",
                "no pyproject.toml — native PyPI resolution requires an MGC-owned pyproject; foreign lockfiles are unsupported until an explicit migration exists",
            )),
        }
    }

    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph> {
        match &self.python_lane {
            Some(py) => py.resolve(manifest).await,
            None => Err(mgc_types::capabilities::unsupported_capability(
                "ai",
                "resolve",
                "no pyproject.toml — ai has no native registry graph for this project",
            )),
        }
    }
}

#[async_trait]
impl mgc_types::capabilities::LockfileProvider for AiAdapter {
    fn probe_lockfile_provider(&self) -> MgResult<()> {
        match &self.python_lane {
            Some(py) => py.probe_lockfile_provider(),
            None => Err(mgc_types::capabilities::unsupported_capability(
                "ai",
                "write_manifest",
                "no pyproject.toml — no mgc-written manifest surface for this ai project",
            )),
        }
    }

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        match &self.python_lane {
            Some(py) => py.write_manifest(project_root, manifest).await,
            None => Err(mgc_types::capabilities::unsupported_capability(
                "ai",
                "write_manifest",
                "no pyproject.toml — no mgc-written manifest surface for this ai project",
            )),
        }
    }
}

#[async_trait]
impl mgc_types::capabilities::ArtifactFetcher for AiAdapter {
    fn probe_artifact_fetcher(&self) -> MgResult<()> {
        match &self.python_lane {
            Some(py) => py.probe_artifact_fetcher(),
            None => Err(mgc_types::capabilities::unsupported_capability(
                "ai",
                "fetch",
                "no pyproject.toml — nothing for the native PyPI engine to fetch",
            )),
        }
    }

    async fn fetch(&self, graph: &ResolvedGraph) -> MgResult<()> {
        match &self.python_lane {
            Some(py) => py.fetch(graph).await,
            None => Err(mgc_types::capabilities::unsupported_capability(
                "ai",
                "fetch",
                "no pyproject.toml — nothing for the native PyPI engine to fetch",
            )),
        }
    }
}

#[async_trait]
impl mgc_types::capabilities::ContentStoreProvider for AiAdapter {
    fn probe_content_store(&self) -> MgResult<()> {
        match &self.python_lane {
            Some(py) => py.probe_content_store(),
            None => Err(mgc_types::capabilities::unsupported_capability(
                "ai",
                "install",
                "no pyproject.toml — native PyPI install requires an MGC-owned pyproject; foreign lockfiles are unsupported",
            )),
        }
    }

    async fn install(
        &self,
        graph: &ResolvedGraph,
        project_root: &Path,
        opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        match &self.python_lane {
            Some(py) => py.install(graph, project_root, opts).await,
            None => Err(mgc_types::capabilities::unsupported_capability(
                "ai",
                "install",
                "no pyproject.toml — native PyPI install requires an MGC-owned pyproject; foreign lockfiles are unsupported",
            )),
        }
    }
}

// Unclaimed capabilities — empty impls inherit the fail-closed
// Unsupported probes/defaults from mgc_types::capabilities.
// Capability chưa claim — impl rỗng kế thừa probe/default fail-closed
// từ mgc_types::capabilities.
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
