//! Global Gate 1 — capability traits (phase 1, Tech Lead 2026-09-16).
//!
//! Phase 1 splits the monolithic `PackageAdapter` into per-capability traits
//! with fail-closed default bodies. Real methods migrate gradually: today the
//! registry-lifecycle methods (`resolve/fetch/install/write_manifest/audit`,
//! `add/remove/update`) live on their capability traits with `Unsupported`
//! defaults; orchestrator-level methods (`parse_manifest`, `list`,
//! `prepare_add`, `audit_fix`, dedupe prefs) stay on the `PackageAdapter`
//! facade so CLI dispatch is untouched.
//! Tách `PackageAdapter` đơn khối thành các capability trait với default
//! fail-closed. Method thật di chuyển dần: hôm nay các method registry-
//! lifecycle nằm trên trait capability tương ứng với default `Unsupported`;
//! method orchestrator-level ở lại trên facade `PackageAdapter` để dispatch
//! CLI không đổi.
//!
//! The capability MATRIX is no longer hand-written: every adapter declares
//! `CAPABILITIES` (its real claims) and the CLI exposes `mgc capabilities
//! --json`; the lifecycle matrix reads the binary instead of a table.
//! Matrix capability không còn viết tay: mỗi adapter khai báo `CAPABILITIES`
//! (đúng claim thật) và CLI expose `mgc capabilities --json`; lifecycle
//! matrix đọc từ binary thay vì bảng cứng.
//!
//! Enforcement: `mgc-types/tests/capabilities.rs` — a claimed capability
//! MUST override its probe (non-`Unsupported`); an unclaimed
//! `DependencyResolver` MUST answer `resolve` with `MgError::Unsupported`.
//! Ràng buộc: capability có claim PHẢI override probe (không
//! `Unsupported`); core không claim `DependencyResolver` PHẢI trả
//! `MgError::Unsupported` khi `resolve`.

use crate::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, ResolvedGraph, UpdatedPackage,
};
use crate::ecosystem::Ecosystem;
use crate::error::{MgError, MgResult};
use crate::manifest::Manifest;
use crate::package::{PackageId, PackageName, VersionRange};
use async_trait::async_trait;
use std::path::Path;

/// The 14 capability names of the Global Gate (kebab-case on the wire).
/// 14 tên capability của Global Gate (kebab-case khi serialize JSON).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    ProjectDetector,
    ScaffoldProvider,
    DependencyResolver,
    LockfileProvider,
    ArtifactFetcher,
    ContentStoreProvider,
    Materializer,
    LifecycleRunner,
    AuditProvider,
    OptimizerProvider,
    SimulatorProvider,
    DeviceProvider,
    DeployProvider,
    ModelRuntimeProvider,
}

impl Capability {
    /// All 14 capabilities, declaration order — used to validate claims.
    /// Cả 14 capability theo thứ tự khai báo — dùng để kiểm tra claim.
    pub const ALL: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::DependencyResolver,
        Capability::LockfileProvider,
        Capability::ArtifactFetcher,
        Capability::ContentStoreProvider,
        Capability::Materializer,
        Capability::LifecycleRunner,
        Capability::AuditProvider,
        Capability::OptimizerProvider,
        Capability::SimulatorProvider,
        Capability::DeviceProvider,
        Capability::DeployProvider,
        Capability::ModelRuntimeProvider,
    ];

    /// kebab-case wire name — mirrors the serde representation.
    /// Tên kebab-case trên wire — trùng biểu diễn serde.
    pub fn as_str(&self) -> &'static str {
        match self {
            Capability::ProjectDetector => "project-detector",
            Capability::ScaffoldProvider => "scaffold-provider",
            Capability::DependencyResolver => "dependency-resolver",
            Capability::LockfileProvider => "lockfile-provider",
            Capability::ArtifactFetcher => "artifact-fetcher",
            Capability::ContentStoreProvider => "content-store-provider",
            Capability::Materializer => "materializer",
            Capability::LifecycleRunner => "lifecycle-runner",
            Capability::AuditProvider => "audit-provider",
            Capability::OptimizerProvider => "optimizer-provider",
            Capability::SimulatorProvider => "simulator-provider",
            Capability::DeviceProvider => "device-provider",
            Capability::DeployProvider => "deploy-provider",
            Capability::ModelRuntimeProvider => "model-runtime-provider",
        }
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Fail-closed `Unsupported` error in the established message style
/// (`{core} core does not support '{capability}' yet: {guidance}`).
/// Lỗi `Unsupported` fail-closed đúng style message hiện hành.
pub fn unsupported_capability(
    core: &'static str,
    capability: &'static str,
    guidance: &str,
) -> MgError {
    MgError::Unsupported {
        core,
        capability,
        guidance: guidance.to_string(),
    }
}

/// Core identity — shared by every capability trait so default bodies can
/// build a precise `Unsupported` error (`core_id` returns `&'static str`
/// because `MgError::Unsupported` stores static strings; a METHOD, not an
/// associated const, keeps the trait dyn-compatible for
/// `Arc<dyn PackageAdapter>`).
/// Danh tính core — mọi capability trait kế thừa để default body sinh lỗi
/// `Unsupported` chuẩn (`core_id` trả `&'static str` vì
/// `MgError::Unsupported` lưu chuỗi tĩnh; dùng METHOD thay associated const
/// để trait vẫn dyn-compatible cho `Arc<dyn PackageAdapter>`).
pub trait CoreIdent {
    /// Static core id as used by the CLI and the lifecycle matrix
    /// ("web", "lib", "ai", "app", "game", "iot", "clo", "cicd", "hardware").
    /// Id core tĩnh trùng với CLI và lifecycle matrix.
    fn core_id(&self) -> &'static str;

    /// Human-readable core name — moved verbatim from `PackageAdapter`.
    /// Tên core — di chuyển nguyên vẹn từ `PackageAdapter`.
    fn name(&self) -> &str;

    /// Ecosystem discriminator — moved verbatim from `PackageAdapter`.
    /// Ecosystem — di chuyển nguyên vẹn từ `PackageAdapter`.
    fn ecosystem(&self) -> Ecosystem;
}

/// Detects whether a project root belongs to this core.
/// Method thật của core (`can_handle`) — di chuyển nguyên vẹn.
pub trait ProjectDetector: CoreIdent {
    fn can_handle(&self, project_root: &Path) -> bool;
}

/// Resolves/edits the dependency set through mgc.
/// Note: some cores (game/iot/cloud) keep REAL toolchain-delegating
/// `add/remove/update` overrides while `resolve` stays fail-closed — they
/// deliberately do NOT claim this capability (the full surface is not real).
/// Resolver/edit tập dependency qua mgc. Lưu ý: một số core (game/iot/cloud)
/// giữ override `add/remove/update` thật (ủy quyền toolchain) trong khi
/// `resolve` vẫn fail-closed — các core đó CHỦ Ý không claim capability này
/// (bề mặt trait không thật hoàn toàn).
#[async_trait]
pub trait DependencyResolver: CoreIdent {
    /// Lightweight gate probe — never touches the network.
    /// Probe nhẹ cho gate — không bao giờ chạm network.
    fn probe_dependency_resolver(&self) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "resolve",
            "this core does not resolve a registry dependency graph; dependencies are managed by the core's own toolchain",
        ))
    }

    async fn resolve(&self, _manifest: &Manifest) -> MgResult<ResolvedGraph> {
        Err(unsupported_capability(
            self.core_id(),
            "resolve",
            "this core does not resolve a registry dependency graph; dependencies are managed by the core's own toolchain",
        ))
    }

    async fn add(
        &self,
        _project_root: &Path,
        _name: &PackageName,
        _range: Option<&VersionRange>,
        _opts: AddOptions,
    ) -> MgResult<PackageId> {
        Err(unsupported_capability(
            self.core_id(),
            "add",
            "this core does not manage dependencies through mgc; use the core's own toolchain workflow",
        ))
    }

    async fn remove(&self, _project_root: &Path, _name: &PackageName) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "remove",
            "this core does not manage dependencies through mgc; use the core's own toolchain workflow",
        ))
    }

    async fn update(
        &self,
        _project_root: &Path,
        _name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        Err(unsupported_capability(
            self.core_id(),
            "update",
            "this core has no mgc-managed update channel; use the core's own toolchain",
        ))
    }
}

/// Downloads resolved artifacts (registry tarballs / modules).
/// Tải artifact đã resolve (tarball registry / module).
#[async_trait]
pub trait ArtifactFetcher: CoreIdent {
    fn probe_artifact_fetcher(&self) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "fetch",
            "nothing for mgc to fetch; the core's toolchain owns downloads",
        ))
    }

    async fn fetch(&self, _graph: &ResolvedGraph) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "fetch",
            "nothing for mgc to fetch; the core's toolchain owns downloads",
        ))
    }
}

/// Installs the resolved graph (mgc content-store backed where available).
/// Cài graph đã resolve (qua content-store của mgc khi có).
#[async_trait]
pub trait ContentStoreProvider: CoreIdent {
    fn probe_content_store(&self) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "install",
            "this core has no mgc registry install; dependencies are materialized by the core's own workflow",
        ))
    }

    async fn install(
        &self,
        _graph: &ResolvedGraph,
        _project_root: &Path,
        _opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        Err(unsupported_capability(
            self.core_id(),
            "install",
            "this core has no mgc registry install; dependencies are materialized by the core's own workflow",
        ))
    }
}

/// Writes the core's manifest/lockfile surface (moved from `PackageAdapter`).
/// Ghi manifest/lockfile của core — di chuyển từ `PackageAdapter`.
#[async_trait]
pub trait LockfileProvider: CoreIdent {
    fn probe_lockfile_provider(&self) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "write_manifest",
            "this core does not use mgc-written manifests; regenerate via `mgc create-<core>` or edit the manifest directly",
        ))
    }

    async fn write_manifest(&self, _project_root: &Path, _manifest: &Manifest) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "write_manifest",
            "this core does not use mgc-written manifests; regenerate via `mgc create-<core>` or edit the manifest directly",
        ))
    }
}

/// Runs the security scanners for the core's ecosystems.
/// Chạy scanner bảo mật cho các ecosystem của core.
#[async_trait]
pub trait AuditProvider: CoreIdent {
    fn probe_audit_provider(&self) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "audit",
            "no scanner implemented for this core",
        ))
    }

    async fn audit(&self, _project_root: &Path) -> MgResult<AuditReport> {
        Err(unsupported_capability(
            self.core_id(),
            "audit",
            "no scanner implemented for this core",
        ))
    }
}

/// Marker + probe: scaffolds new projects for the core's frameworks
/// (adapter scaffold modules and/or the CLI `mgc create-<core>` lane).
/// Marker + probe: scaffold project mới cho framework của core (module
/// scaffold của adapter và/hoặc lane CLI `mgc create-<core>`).
pub trait ScaffoldProvider: CoreIdent {
    fn probe_scaffold(&self) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "scaffold",
            "this core does not implement scaffolding",
        ))
    }
}

/// Marker + probe: runs the core's lifecycle (scripts / toolchain phases).
/// No method moves in phase 1 — lifecycle execution is orchestrator-level
/// (inside install / the CLI per-core lanes) and stays where it is.
/// Marker + probe: chạy lifecycle của core (script / pha toolchain). Phase 1
/// không di chuyển method nào — chạy lifecycle là cấp orchestrator (trong
/// install / lane CLI per-core) và giữ nguyên vị trí.
pub trait LifecycleRunner: CoreIdent {
    fn probe_lifecycle_runner(&self) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "lifecycle",
            "this core has no mgc-run lifecycle phase",
        ))
    }
}

/// Marker + probe: optimizes artifacts/benchmarks for the core.
/// Marker + probe: tối ưu artifact / benchmark cho core.
pub trait OptimizerProvider: CoreIdent {
    fn probe_optimizer(&self) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "optimizer",
            "this core has no optimizer/bench capability",
        ))
    }
}

/// Marker + probe: materializes the dependency tree layout (node_modules…).
/// Marker + probe: materialize layout cây dependency (node_modules…).
pub trait Materializer: CoreIdent {
    fn probe_materializer(&self) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "materialize",
            "this core does not materialize a dependency tree layout",
        ))
    }
}

/// Marker + probe: runs simulators for the core (phase-1: unclaimed by all).
/// Marker + probe: chạy simulator cho core (phase-1: chưa core nào claim).
pub trait SimulatorProvider: CoreIdent {
    fn probe_simulator(&self) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "simulate",
            "this core has no simulator capability",
        ))
    }
}

/// Marker + probe: flashes/talks to physical devices (phase-1: unclaimed).
/// Marker + probe: flash / nói chuyện với thiết bị thật (phase-1: chưa claim).
pub trait DeviceProvider: CoreIdent {
    fn probe_device(&self) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "device",
            "this core has no device capability",
        ))
    }
}

/// Marker + probe: deploys the project (cloud/CD target).
/// Marker + probe: deploy project (đích cloud/CD).
pub trait DeployProvider: CoreIdent {
    fn probe_deploy(&self) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "deploy",
            "this core has no deploy capability",
        ))
    }
}

/// Marker + probe: runs AI model runtimes (phase-1: unclaimed; the model
/// download/runtime machinery lives in the ai core's own modules).
/// Marker + probe: chạy runtime model AI (phase-1: chưa claim; cơ chế
/// download/runtime model nằm ở module riêng của core ai).
pub trait ModelRuntimeProvider: CoreIdent {
    fn probe_model_runtime(&self) -> MgResult<()> {
        Err(unsupported_capability(
            self.core_id(),
            "model-runtime",
            "this core has no model runtime capability",
        ))
    }
}
