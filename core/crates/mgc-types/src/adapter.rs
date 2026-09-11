use crate::ecosystem::Ecosystem;
use crate::error::{MgError, MgResult};
use crate::manifest::Manifest;
use crate::package::{PackageId, PackageName, VersionRange};
use crate::version::Version;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AddOptions {
    pub dev: bool,
    pub optional: bool,
    pub peer: bool,
    pub exact: bool,
    pub no_save: bool,
    pub global: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PreparedAdd {
    pub id: PackageId,
    pub range: VersionRange,
}

/// Options controlling install behaviour.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct InstallOptions {
    /// Skip running lifecycle scripts (preinstall / install / postinstall).
    pub ignore_scripts: bool,
    /// Explicitly allow lifecycle scripts. Defaults to false for secure installs.
    pub allow_scripts: bool,
    /// Use flat hoisting layout instead of strict symlink virtual store.
    pub legacy_flat: bool,
    /// Fail fast if mgc.lock is missing or out-of-sync (CI mode).
    pub frozen: bool,
    /// Skip checking already-installed root packages. Only materialize what differs.
    /// Safe after add/remove when graph changed by exactly one leaf package.
    pub incremental: bool,
    /// Packages to force-install even when incremental mode is active.
    /// Only used when `incremental` is true.
    pub force_install: Vec<PackageId>,
    /// Prefer reusing installed versions (dedupe) instead of latest (02 §2.1).
    /// Opt-in — default off for safety.
    pub prefer_dedupe: bool,
    /// Re-link dangling symlinks in node_modules from the virtual store (02 §2.2).
    pub repair: bool,
    /// Offline install: resolve from lockfile + local cache only; any required
    /// network fetch fails closed instead of silently hitting the registry.
    /// Cài offline: chỉ dùng lockfile + cache local; thiếu cache thì fail rõ.
    pub offline: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct InstallSummary {
    pub added: Vec<PackageId>,
    pub bytes_from_cache: u64,
    pub duration_ms: u64,
    /// P0-6 (2026-09-11): cache accounting mode — HONEST labeling so a
    /// zero byte-count is never misread as "no cache". `mgc-store` =
    /// measured through MagiCore's content-addressable store;
    /// `delegated` = the native toolchain owns its cache (cargo/uv/go
    /// module cache) and mgc does not count its bytes.
    /// P0-6: chế độ tính cache — ghi nhãn TRUNG THỰC để byte-count = 0
    /// không bao giờ bị đọc nhầm "không có cache". `mgc-store` = đo qua
    /// content-addressable store của MagiCore; `delegated` = toolchain
    /// gốc giữ cache của nó (cargo/uv/go module cache) và mgc không
    /// đếm byte của nó.
    #[serde(default)]
    pub cache_mode: InstallCacheMode,
}

/// How the install pipeline accounted for cache bytes.
/// Cách pipeline install tính byte cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum InstallCacheMode {
    /// Measured via the mgc content-addressable store.
    /// Đo qua content-addressable store của mgc.
    #[default]
    MgCStore,
    /// Native toolchain cache owns the bytes (delegation, not mgc).
    /// Cache toolchain gốc giữ byte (ủy quyền, không phải mgc).
    Delegated,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstalledPackage {
    pub id: PackageId,
    pub path: PathBuf,
    pub integrity: Option<String>,
    pub is_direct: bool,
    pub is_dev: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdatedPackage {
    pub name: String,
    pub from_version: String,
    pub to_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VulnerabilitySeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl VulnerabilitySeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Info => "info",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "critical" => Self::Critical,
            "high" => Self::High,
            "medium" | "moderate" => Self::Medium,
            "low" => Self::Low,
            _ => Self::Info,
        }
    }
}

impl std::str::FromStr for VulnerabilitySeverity {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_str(s))
    }
}

impl From<&str> for VulnerabilitySeverity {
    fn from(s: &str) -> Self {
        Self::from_str(s)
    }
}

impl VulnerabilitySeverity {
    /// Returns true if this severity is at least as severe as `other`.
    pub fn is_at_least(&self, other: &Self) -> bool {
        let rank = |s: &Self| match s {
            Self::Critical => 4,
            Self::High => 3,
            Self::Medium => 2,
            Self::Low => 1,
            Self::Info => 0,
        };
        rank(self) >= rank(other)
    }
}

impl std::fmt::Display for VulnerabilitySeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Vulnerability {
    pub package: PackageId,
    pub title: String,
    /// Raw severity string (for display / backward compat)
    pub severity: String,
    pub cve: String,
    /// Structured severity level.
    pub severity_level: VulnerabilitySeverity,
    /// Version range(s) that contain a fix, if known.
    pub patched_versions: Option<String>,
    /// Link to the advisory.
    pub url: Option<String>,
    /// Scanner that produced this finding (e.g. "cargo-audit", "osv").
    /// Scanner sinh ra finding này — giữ truy vết nguồn evidence.
    #[serde(default)]
    pub scanner: Option<String>,
    /// Ecosystem of the affected package (e.g. "rust", "python").
    /// Ecosystem của package dính finding.
    #[serde(default)]
    pub ecosystem: Option<String>,
    /// Evidence timestamp (RFC 3339) — when the scanner observed it.
    /// Thời điểm scanner quan sát finding (RFC 3339).
    #[serde(default)]
    pub evidence_at: Option<String>,
}

impl Vulnerability {
    /// Stamp provenance on a finding: scanner, ecosystem, and the
    /// observation time (UTC, RFC 3339) — evidence must travel WITH the
    /// finding so CI ingest can verify freshness (Tech Lead 2026-09-09).
    /// Gắn nguồn gốc cho finding: scanner, ecosystem, thời điểm quan sát
    /// (UTC, RFC 3339) — evidence đi CÙNG finding để CI kiểm tra độ tươi.
    pub fn with_evidence(
        mut self,
        scanner: impl Into<String>,
        ecosystem: impl Into<String>,
    ) -> Self {
        self.scanner = Some(scanner.into());
        self.ecosystem = Some(ecosystem.into());
        self.evidence_at = Some(now_rfc3339());
        self
    }
}

/// Current UTC time, RFC 3339 — no chrono dependency (lockfile stays
/// stable); std::time + manual formatting is enough for second precision.
/// Thời điểm UTC hiện tại theo RFC 3339 — không thêm dependency chrono;
/// std::time + format thủ công đủ cho độ chính xác giây.
fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil-from-days algorithm (Howard Hinnant) — exact for all dates.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoy = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoy + era * 400;
    let doy = doe - (365 * yoy + yoy / 4 - yoy / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Public RFC 3339 UTC clock for output modules (JSON/SARIF/CycloneDX
/// timestamps) — one std-only time source across the workspace (Tech
/// Lead P1 2026-09-09), no chrono dependency.
/// Đồng hồ UTC RFC 3339 public cho các module output — một nguồn thời
/// gian std duy nhất toàn workspace, không thêm dependency chrono.
pub fn now_rfc3339_public() -> String {
    now_rfc3339()
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AuditReport {
    pub packages_audited: usize,
    pub vulnerability_count: usize,
    pub vulnerabilities: Vec<Vulnerability>,
    /// P0.6 FIX: Scanner availability status
    /// Indicates whether audit scanner was actually available and ran
    /// vs returning fake clean when scanner doesn't exist
    #[serde(default)]
    pub scanner_status: ScannerStatus,
}

/// Scanner availability state — trạng thái sẵn sàng của trình quét.
/// Distinguishes a clean audit from an unavailable scanner — không báo sạch giả khi thiếu scanner.
///
/// Unified contract (Tech Lead 2026-09-09 §1): five distinct states so CI
/// can tell "scanned clean" from "not scanned at all", "partially scanned",
/// "tool missing" and "scanner failed".
/// Hợp đồng thống nhất: 5 trạng thái riêng biệt để CI phân biệt "quét sạch",
/// "chưa quét", "quét một phần", "thiếu tool" và "scanner lỗi".
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(tag = "state", content = "data", rename_all = "snake_case")]
pub enum ScannerStatus {
    /// Scanner available, executed, and its output parsed successfully.
    /// Scanner có sẵn, đã chạy và output parse thành công.
    /// ONLY this variant + zero findings may ever print "clean".
    /// Chỉ biến thể này + zero finding mới được in "clean".
    #[default]
    Available,
    /// Some dependencies/files could not be scanned; strict CI must fail.
    /// Có dependency/file không scan được; CI strict phải fail.
    Partial {
        scanned: usize,
        skipped: usize,
        reasons: Vec<String>,
    },
    /// The runtime machine lacks the required tool; audit NOT performed.
    /// Máy thiếu tool cần thiết; audit KHÔNG được thực hiện.
    /// Carries install guidance (remediation) for the user.
    /// Kèm hướng dẫn cài đặt (remediation) cho user.
    ToolMissing { tool: String, remediation: String },
    /// Dev-only state: the ecosystem has no scanner implemented yet — never
    /// claim core completion while this is reachable.
    /// Trạng thái chỉ dùng khi phát triển: ecosystem chưa có scanner — không
    /// được claim core hoàn thành khi còn chạm biến thể này.
    UnsupportedEcosystem { ecosystem: String },
    /// Tool ran but errored (schema drift, network failure, malformed data).
    /// Tool chạy nhưng lỗi (đổi schema, lỗi mạng, dữ liệu malformed).
    Failed { scanner: String, reason: String },
}

/// One outdated dependency entry — version drift, NOT a CVE.
/// Một entry dependency cũ — lệch version, KHÔNG phải CVE.
/// `flutter pub outdated` reports dependency freshness; mixing it into
/// security findings would misrepresent audit results.
/// `flutter pub outdated` báo độ tươi dependency; trộn vào finding bảo mật
/// sẽ xuyên tạc kết quả audit.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutdatedDependency {
    pub package: PackageId,
    /// Latest version available on the registry.
    /// Version mới nhất trên registry.
    pub latest_version: Version,
}

/// Dependency freshness report — version drift across audited packages.
/// Báo cáo độ tươi dependency — lệch version trong các package đã kiểm tra.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DependencyHealthReport {
    /// Total packages checked (up to date + outdated).
    /// Tổng package đã kiểm tra (cả mới lẫn cũ).
    pub packages_checked: usize,
    pub outdated_count: usize,
    pub outdated: Vec<OutdatedDependency>,
}

impl AuditReport {
    pub fn clean(packages_audited: usize) -> Self {
        Self {
            packages_audited,
            vulnerability_count: 0,
            vulnerabilities: vec![],
            scanner_status: ScannerStatus::Available,
        }
    }

    /// Scanner executed successfully — full-coverage audit result.
    /// Scanner đã chạy thành công — kết quả audit phủ đầy đủ.
    pub fn scanned(packages_audited: usize, vulnerabilities: Vec<Vulnerability>) -> Self {
        Self {
            packages_audited,
            vulnerability_count: vulnerabilities.len(),
            vulnerabilities,
            scanner_status: ScannerStatus::Available,
        }
    }

    /// Tool missing on this machine — audit NOT performed (Tech Lead §1).
    /// Thiếu tool trên máy này — audit KHÔNG được thực hiện.
    pub fn tool_missing(tool: impl Into<String>, remediation: impl Into<String>) -> Self {
        Self {
            packages_audited: 0,
            vulnerability_count: 0,
            vulnerabilities: vec![],
            scanner_status: ScannerStatus::ToolMissing {
                tool: tool.into(),
                remediation: remediation.into(),
            },
        }
    }

    /// Dev-only: no scanner implemented for this ecosystem yet.
    /// Chỉ dùng khi phát triển: chưa có scanner cho ecosystem này.
    pub fn unsupported_ecosystem(ecosystem: impl Into<String>) -> Self {
        Self {
            packages_audited: 0,
            vulnerability_count: 0,
            vulnerabilities: vec![],
            scanner_status: ScannerStatus::UnsupportedEcosystem {
                ecosystem: ecosystem.into(),
            },
        }
    }

    /// Scanner ran but failed (schema drift, network, malformed output).
    /// Scanner chạy nhưng lỗi (đổi schema, mạng, output malformed).
    pub fn scanner_failed(scanner: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            packages_audited: 0,
            vulnerability_count: 0,
            vulnerabilities: vec![],
            scanner_status: ScannerStatus::Failed {
                scanner: scanner.into(),
                reason: reason.into(),
            },
        }
    }

    /// Clean ONLY when the scanner actually ran (Available) and found
    /// nothing. Any non-Available state is NOT clean — a public API caller
    /// must never read "clean" from an unverified report (Tech Lead
    /// P0-1 2026-09-09).
    /// Chỉ sạch khi scanner THẬT SỰ chạy (Available) và không tìm thấy gì.
    /// Mọi trạng thái khác KHÔNG sạch — caller ngoài không được đọc
    /// "sạch" từ report chưa xác thực.
    pub fn is_clean(&self) -> bool {
        matches!(self.scanner_status, ScannerStatus::Available)
            && self.vulnerabilities.is_empty()
            && self.vulnerability_count == 0
    }

    /// P0.6 FIX: Check if scanner actually ran
    pub fn scanner_available(&self) -> bool {
        matches!(self.scanner_status, ScannerStatus::Available)
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ResolvedGraph {
    pub packages: Vec<ResolvedPackage>,
}

impl ResolvedGraph {
    pub fn empty() -> Self {
        Self { packages: vec![] }
    }

    pub fn len(&self) -> usize {
        self.packages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResolvedPackage {
    pub id: PackageId,
    pub integrity: String,
    pub tarball_url: String,
    /// Regular (non-peer) dependencies.
    pub deps: Vec<PackageId>,
    /// Peer dependencies — resolved to actual graph members.
    /// Populated during the resolution phase so that the layout
    /// materializer does not need a secondary disk read of package.json.
    #[serde(default)]
    pub peer_deps: Vec<PackageId>,
    pub direct: bool,
    pub dev: bool,
}

#[async_trait]
pub trait PackageAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn ecosystem(&self) -> Ecosystem;
    fn can_handle(&self, project_root: &Path) -> bool;

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest>;
    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()>;
    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph>;
    async fn fetch(&self, graph: &ResolvedGraph) -> MgResult<()>;
    async fn install(
        &self,
        graph: &ResolvedGraph,
        project_root: &Path,
        opts: InstallOptions,
    ) -> MgResult<InstallSummary>;
    async fn add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PackageId>;
    async fn prepare_add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PreparedAdd> {
        let exact = opts.exact;
        let mut dry_opts = opts;
        dry_opts.no_save = true;
        let id = self.add(project_root, name, range, dry_opts).await?;
        let saved_range = match range {
            Some(range) if exact => {
                let raw = range
                    .as_str()
                    .trim_start_matches('^')
                    .trim_start_matches('~');
                VersionRange::parse(raw)?
            }
            Some(range) => range.clone(),
            None => VersionRange::star(),
        };
        Ok(PreparedAdd {
            id,
            range: saved_range,
        })
    }
    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()>;
    async fn update(
        &self,
        project_root: &Path,
        name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>>;
    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>>;
    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport>;

    /// T5 audit --fix: re-resolve the given vulnerable packages to a newer
    /// version and rewrite the lockfile ONLY when re-resolution succeeds
    /// (fail-closed — a failed resolve leaves manifest + lockfile untouched).
    /// Returns the number of packages bumped. Default: unsupported.
    async fn audit_fix(&self, _project_root: &Path, _vulnerable: &[PackageId]) -> MgResult<usize> {
        Err(MgError::Other(
            "audit --fix is not supported for this core".to_string(),
        ))
    }

    /// Enable dedupe preference (reuse installed versions) for the next resolve.
    /// Default no-op — opt-in per install (02 §2.1).
    fn set_dedupe_pref(&self, _enabled: bool) {}

    /// Provide already-installed versions (from lockfile) for dedupe resolution.
    /// Default no-op.
    fn set_existing_versions(&self, _versions: std::collections::HashMap<String, String>) {}
}
