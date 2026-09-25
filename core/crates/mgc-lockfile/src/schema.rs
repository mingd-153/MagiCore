//! Lockfile schema v3 structures — canonical unified dependency graph.
//! Cấu trúc schema lockfile v3 — đồ thị phụ thuộc hợp nhất canonical.
//!
//! v3 keeps full backward-compatible READ of v2 files: every new field has a
//! `serde(default)` so a v2 document parses unchanged (new fields fall back to
//! their default). Writing always emits the v3 shape.
//! v3 giữ đọc-école tương thích ngược với file v2: mọi field mới đều có
//! `serde(default)` nên file v2 parse nguyên vẹn (field mới về giá trị mặc
//! định). Ghi thì luôn xuất shape v3.

use serde::{Deserialize, Serialize};

use crate::ecosystem_tag::EcosystemTag;
use crate::{LockfileError, LockfileResult};

/// Canonical schema version emitted by this crate — Phiên bản schema canonical mà crate này ghi ra.
pub const LOCKFILE_SCHEMA_VERSION: &str = "3";

/// Provenance source kind: package pin imported from an ecosystem lockfile.
/// Nguồn gốc: pin package được nhập từ lockfile của hệ sinh thái.
pub const SOURCE_KIND_REGISTRY_IMPORT: &str = "registry-import";
/// Provenance source kind: resolved natively by the mgc resolver (Phase 2).
/// Nguồn gốc: resolve nội bộ bởi resolver của mgc (Phase 2).
pub const SOURCE_KIND_NATIVE_RESOLVE: &str = "native-resolve";
/// Provenance source kind: delegated to the ecosystem toolchain (audit-led).
/// Nguồn gốc: ủy thác cho toolchain hệ sinh thái (có audit).
pub const SOURCE_KIND_DELEGATED_TOOL: &str = "delegated-tool";

/// Dependency ownership: mgc resolves the package natively.
/// Quyền sở phụ thuộc: mgc tự resolve.
pub const OWNER_MGC_NATIVE: &str = "mgc-native";
/// Dependency ownership: delegated to the ecosystem toolchain.
/// Quyền sở phụ thuộc: ủy thác cho toolchain hệ sinh thái.
pub const OWNER_DELEGATED: &str = "delegated";
/// Dependency ownership: scaffold generated once, then user-owned.
/// Quyền sở phụ thuộc: scaffold sinh một lần, sau đó user sở hữu.
pub const OWNER_SCAFFOLD_ONLY: &str = "scaffold-only";
/// Dependency ownership: no resolution path for this ecosystem yet.
/// Quyền sở phụ thuộc: chưa có đường resolve cho hệ sinh thái này.
pub const OWNER_UNSUPPORTED: &str = "unsupported";

/// Lockfile v3 root structure — Cấu trúc root lockfile v3
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lockfile {
    /// Schema version — Phiên bản schema
    pub version: String,

    /// Metadata — Metadata
    pub metadata: LockfileMetadata,

    /// Root direct-dependency pins (workspace root → name@version edges).
    /// Imported from the source lockfile's root dependency set (deno
    /// workspace.dependencies, npm root "" entry, …). WITHOUT this field
    /// the root graph cannot be represented and importers resorted to
    /// self-edges — a graph-semantics lie (P0 finding #6, 2026-09-12).
    /// serde default keeps older v2 lockfiles (field absent) parsing.
    // Pin direct-dep của root (cạnh root → name@version). Nhập từ tập
    // dependency gốc của lockfile nguồn (deno workspace.dependencies,
    // entry root "" của npm…). Không có trường này thì graph root không
    // biểu diễn được và importer phải tự tạo self-edge — sai semantics
    // (P0 finding #6). serde default để lockfile v2 cũ (thiếu trường)
    // vẫn parse được.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub root_dependencies: Vec<String>,

    /// Package list — Danh sách package
    #[serde(rename = "package")]
    pub packages: Vec<Package>,

    /// Workspace topology: member cores and cross-core dependency edges
    /// (v3 — the unified graph spans all cores, not just web).
    /// Topology workspace: các core thành viên và cạnh phụ thuộc liên core
    /// (v3 — đồ thị hợp nhất phủ mọi core, không chỉ web).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceTopology>,

    /// Optimizer profile stamp recorded at lock time (e.g. "size", "perf").
    /// Nhãn profile tối ưu ghi tại thời điểm lock (vd "size", "perf").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimizer_profile: Option<String>,
}

/// Lockfile metadata — Metadata lockfile
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockfileMetadata {
    /// Generation timestamp (ISO 8601) — Timestamp tạo (ISO 8601)
    pub generated_at: String,

    /// Generator info (e.g., "mgc/1.0.0") — Thông tin generator
    pub generator: String,

    /// Lockfile self-hash (BLAKE3, excludes signature) — Hash tự thân lockfile (BLAKE3, loại trừ signature)
    pub lockfile_hash: String,

    /// Signer info (optional) — Thông tin người ký (tùy chọn)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signer: Option<SignerInfo>,

    /// Delegation audit snapshot (v3): one entry per (core, owner, tool)
    /// combination, synced from docs/specs/dependencyDelegationAudit.json.
    /// Ảnh chụp audit ủy thác (v3): mỗi entry cho một tổ hợp
    /// (core, owner, tool), đồng bộ từ docs/specs/dependencyDelegationAudit.json.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_ownership: Vec<OwnershipEntry>,
}

/// Signer information — Thông tin người ký
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignerInfo {
    /// Key ID (first 8 bytes of BLAKE3(pubkey)) — Key ID (8 bytes đầu BLAKE3(pubkey))
    pub key_id: String,

    /// Ed25519 public key (base64) — Khóa công khai Ed25519 (base64)
    pub public_key: String,

    /// Signing timestamp (ISO 8601) — Timestamp ký (ISO 8601)
    pub signed_at: String,
}

/// Package entry — Entry package
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Package {
    /// MGC core that owns this lock entry (`web`, `ai`, `app`, `lib`, …).
    /// Absent only on legacy/imported records whose owner cannot be proven.
    /// Core MagiCore sở hữu entry lock này; vắng mặt với lock cũ/import chưa
    /// đủ bằng chứng để gán quyền sở hữu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_core: Option<String>,

    /// Package name — Tên package
    pub name: String,

    /// Package version — Phiên bản package
    pub version: String,

    /// Resolved URL (tarball download) — URL đã resolve (download tarball)
    pub resolved: String,

    /// Integrity hash (SRI format: blake3-base64) — Hash integrity (định dạng SRI: blake3-base64)
    pub integrity: String,

    /// Direct dependencies — Dependencies trực tiếp
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Ecosystem this pin came from (v3; defaults to `other` for v2 data).
    /// Hệ sinh thái của pin (v3; mặc định `other` cho dữ liệu v2).
    #[serde(default, skip_serializing_if = "is_other_ecosystem")]
    pub ecosystem: EcosystemTag,

    /// Registry source URL / protocol id (e.g. "npm://registry.npmjs.org",
    /// "crates://sparse.crates.io") — registry nguồn (URL / protocol id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,

    /// Physical artifact reference in the CAS — Tham chiếu artifact vật lý trong CAS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactRef>,

    /// How this pin entered the lock — Pin này vào lock bằng đường nào.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,

    /// Platform/ABI/interpreter markers (e.g. "sys-win32", "py>=3.11", "abi3").
    /// Marker nền tảng/ABI/interpreter (vd "sys-win32", "py>=3.11", "abi3").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markers: Option<Vec<String>>,

    /// Extra features enabled for this package — Extra features bật cho package này.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extras: Option<Vec<String>>,

    /// Peer dependency names — Tên peer dependency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peers: Option<Vec<String>>,

    /// CAS object ref in mgc-store (e.g. "cas/files/blake3/ab/abcd…").
    /// Ref object CAS trong mgc-store (vd "cas/files/blake3/ab/abcd…").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_ref: Option<String>,

    /// Toolchain requirement (e.g. "rust 1.85", "python >=3.9").
    /// Yêu cầu toolchain (vd "rust 1.85", "python >=3.9").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<String>,

    /// Lifecycle scripts policy: "deny" | "audit" | "allow".
    /// Chính sách lifecycle script: "deny" | "audit" | "allow".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scripts_policy: Option<String>,
}

/// `skip_serializing_if` helper — ecosystem `other` is the v2-compatible
/// default and stays out of the serialized shape.
/// Helper `skip_serializing_if` — ecosystem `other` là mặc định tương thích
/// v2 và không xuất hiện trong shape serialize.
fn is_other_ecosystem(tag: &EcosystemTag) -> bool {
    matches!(tag, EcosystemTag::Other)
}

/// Physical artifact backing a pin: where the bytes live, how big, and how
/// they were verified — Tham chiếu artifact vật lý của pin: byte nằm đâu,
/// kích thước bao nhiêu, xác minh bằng cách nào.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRef {
    /// Artifact URL (registry tarball, git ref, …) — URL artifact (tarball registry, git ref, …).
    pub url: String,
    /// Artifact size in bytes (optional) — Kích thước artifact tính bằng byte (tùy chọn).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// BLAKE3 content hash (hex) — Hash nội dung BLAKE3 (hex).
    pub content_hash: String,
    /// Which mirror/endpoint the bytes were downloaded from — Byte được tải về từ mirror/endpoint nào.
    pub downloaded_from: String,
}

/// Provenance of a pin — how it entered mgc.lock — Nguồn gốc của pin — pin vào mgc.lock bằng đường nào.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    /// One of SOURCE_KIND_* ("registry-import" | "native-resolve" | "delegated-tool").
    /// Một trong các SOURCE_KIND_* ("registry-import" | "native-resolve" | "delegated-tool").
    pub source_kind: String,
    /// Producing tool version (e.g. "cargo 1.85") — Tool sinh ra (vd "cargo 1.85").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Source lockfile path for imports — Đường dẫn lockfile nguồn khi import.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_from: Option<String>,
}

/// Workspace topology across cores — Topology workspace liên core.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceTopology {
    /// Member cores (e.g. ["web", "ai", "app", "lib"]) — Các core thành viên.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,
    /// Cross-core dependency edges — Cạnh phụ thuộc liên core.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cross_core_edges: Vec<CrossEdge>,
}

/// One cross-core dependency edge — Một cạnh phụ thuộc liên core.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrossEdge {
    /// Consuming core — Core tiêu thụ.
    pub from_core: String,
    /// Providing core — Core cung cấp.
    pub to_core: String,
    /// Package name crossing the boundary — Tên package vượt ranh giới.
    pub package: String,
}

/// One dependency-ownership entry (delegation audit snapshot in the lock).
/// Một entry quyền sở hữu dependency (ảnh chụp audit ủy thác trong lock).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnershipEntry {
    /// Core this ownership record applies to — Core mà record sở hữu này áp dụng.
    pub core: String,
    /// One of OWNER_* ("mgc-native" | "delegated" | "scaffold-only" | "unsupported").
    /// Một trong các OWNER_* ("mgc-native" | "delegated" | "scaffold-only" | "unsupported").
    pub owner: String,
    /// Delegated tool (e.g. "cargo 1.85") — Tool được ủy thác (vd "cargo 1.85").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// When the delegation was documented (ISO 8601) — Thời điểm ghi nhận ủy thác (ISO 8601).
    pub documented_at: String,
}

/// Ledger finding shape in docs/specs/dependencyDelegationAudit.json —
/// only the fields the ownership adapter needs; serde ignores the rest.
/// Shape finding trong ledger docs/specs/dependencyDelegationAudit.json —
/// chỉ lấy field cần cho adapter sở hữu; serde bỏ qua phần còn lại.
#[derive(Debug, Deserialize)]
struct DelegationLedgerFinding {
    file: String,
    tool: String,
    status: String,
}

/// Ledger document shape — Shape tài liệu ledger.
#[derive(Debug, Deserialize)]
struct DelegationLedgerFile {
    generated_at: String,
    #[serde(default)]
    findings: Vec<DelegationLedgerFinding>,
}

/// Load the dependency-ownership snapshot from the delegation audit ledger.
/// Đồng bộ ảnh chụp quyền sở hữu dependency từ ledger audit ủy thác.
///
/// Maps ledger findings → deduplicated `OwnershipEntry` list:
/// - `delegated-documented` → `OWNER_DELEGATED`, `allowed` → `OWNER_MGC_NATIVE`;
///   any other status fails closed (unknown statuses must not be silently
///   certified).
/// - `core` is derived from the finding's file path (`adapters/<core>/…`,
///   `cli/src/commands/core/<op>/<core>.rs`); falls back to the first segment.
/// - File must exist (caller guards with `Path::exists`, per design
///   "nếu file tồn tại") — a missing file surfaces as `IoError` instead of
///   silently certifying an empty ledger.
///
/// Ánh xạ findings của ledger → danh sách `OwnershipEntry` đã khử trùng lặp:
/// - `delegated-documented` → `OWNER_DELEGATED`, `allowed` → `OWNER_MGC_NATIVE`;
///   status khác sẽ fail-closed (status lạ không được xác nhận âm thầm).
/// - `core` suy ra từ đường dẫn file của finding (`adapters/<core>/…`,
///   `cli/src/commands/core/<op>/<core>.rs`); fallback về segment đầu.
/// - File phải tồn tại (caller tự kiểm tra `Path::exists`, đúng design
///   "nếu file tồn tại") — file thiếu trả `IoError` thay vì xác nhận âm
///   thầm một ledger rỗng.
pub fn load_ownership_ledger(path: &std::path::Path) -> LockfileResult<Vec<OwnershipEntry>> {
    // Read + parse the ledger strictly — corrupt data must not become a
    // silently-empty ownership snapshot (fail-closed).
    // Đọc + parse ledger nghiêm ngặt — dữ liệu hỏng không được biến thành
    // ảnh chụp rỗng một cách im lặng (fail-closed).
    let content = std::fs::read_to_string(path)?;
    let ledger: DelegationLedgerFile = serde_json::from_str(&content)
        .map_err(|e| LockfileError::ParseError(format!("delegation ledger parse failed: {}", e)))?;

    // Dedupe on (core, owner, tool) — a ledger lists many findings per
    // combination; the lock snapshot keeps one entry per combination.
    // Khử trùng lặp theo (core, owner, tool) — ledger liệt kê nhiều finding
    // cho cùng tổ hợp; ảnh chụp trong lock giữ một entry mỗi tổ hợp.
    let mut seen: std::collections::BTreeSet<(String, String, Option<String>)> =
        std::collections::BTreeSet::new();
    let mut out: Vec<OwnershipEntry> = Vec::new();

    for finding in &ledger.findings {
        // Unknown statuses fail closed — they must never be certified as
        // a valid ownership state in the lock.
        // Status lạ fail-closed — không bao giờ được xác nhận là trạng
        // thái sở hữu hợp lệ trong lock.
        let owner = match finding.status.as_str() {
            "delegated-documented" => OWNER_DELEGATED,
            "allowed" => OWNER_MGC_NATIVE,
            other => {
                return Err(LockfileError::ParseError(format!(
                    "unknown delegation ledger status '{}': refusing to certify ownership",
                    other
                )));
            }
        };
        let core = derive_core_from_finding_path(&finding.file);
        let tool = Some(finding.tool.clone());
        if seen.insert((core.clone(), owner.to_string(), tool.clone())) {
            out.push(OwnershipEntry {
                core,
                owner: owner.to_string(),
                tool,
                documented_at: ledger.generated_at.clone(),
            });
        }
    }

    // Deterministic order regardless of ledger finding order — Thứ tự bất
    // biến bất kể thứ tự finding trong ledger.
    out.sort_by(|a, b| (&a.core, &a.tool).cmp(&(&b.core, &b.tool)));
    Ok(out)
}

/// Derive the owning core from a ledger finding's file path.
/// Suy ra core sở hữu từ đường dẫn file của finding trong ledger.
fn derive_core_from_finding_path(file: &str) -> String {
    let segments: Vec<&str> = file.split('/').collect();

    // adapters/<core>/… → <core> — adapters/<core>/… → <core>
    if segments.len() >= 2 && segments[0] == "adapters" {
        return segments[1].to_string();
    }

    // cli/src/commands/core/<op>/<core>.rs (hoặc <op>/test/<core>.rs) → <core>
    if segments.len() >= 5
        && segments[0] == "cli"
        && segments[1] == "src"
        && segments[2] == "commands"
        && segments[3] == "core"
        && let Some(last) = segments.last()
    {
        let stem = last.strip_suffix(".rs").unwrap_or(last);
        if stem != "mod" && stem != "lib" {
            return stem.to_string();
        }
    }

    // Other CLI sources belong to the cli tool itself — Source CLI khác thuộc về tool cli.
    if segments.first() == Some(&"cli") {
        return "cli".to_string();
    }

    // Fallback: first path segment — Fallback: segment đầu của đường dẫn.
    segments
        .first()
        .map(|s| (*s).to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Signature file structure (mgc.lock.sig) — Cấu trúc file signature
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignatureFile {
    /// Lockfile hash (blake3-base64) — Hash lockfile
    pub lockfile_hash: String,

    /// Ed25519 signature (base64) — Chữ ký Ed25519 (base64)
    pub signature: String,

    /// Key ID — Key ID
    pub key_id: String,

    /// Signing timestamp (ISO 8601) — Timestamp ký
    pub signed_at: String,
}

impl Lockfile {
    /// Create new empty lockfile (v3) — Tạo lockfile rỗng mới (v3)
    pub fn new() -> Self {
        Lockfile {
            version: LOCKFILE_SCHEMA_VERSION.to_string(),
            metadata: LockfileMetadata {
                generated_at: chrono::Utc::now().to_rfc3339(),
                generator: format!("mgc/{}", env!("CARGO_PKG_VERSION")),
                lockfile_hash: String::new(), // Will be computed later — Sẽ tính sau
                signer: None,
                dependency_ownership: Vec::new(),
            },
            root_dependencies: Vec::new(),
            packages: Vec::new(),
            workspace: None,
            optimizer_profile: None,
        }
    }

    /// Add package to lockfile — Thêm package vào lockfile
    pub fn add_package(&mut self, package: Package) {
        self.packages.push(package);
    }

    /// Get package by name — Lấy package theo tên
    pub fn get_package(&self, name: &str) -> Option<&Package> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// Every same-named package (multi-version locks are legitimate —
    /// a peer edge may resolve another version; frozen checks must
    /// any-match, never trust first-match order).
    /// (Mọi package cùng tên — lock đa-version hợp lệ.)
    pub fn get_packages(&self, name: &str) -> impl Iterator<Item = &Package> {
        self.packages.iter().filter(move |p| p.name == name)
    }

    /// Check if lockfile is signed — Kiểm tra lockfile đã ký chưa
    pub fn is_signed(&self) -> bool {
        self.metadata.signer.is_some()
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new()
    }
}

impl Package {
    /// Create new package entry — Tạo entry package mới
    pub fn new(name: String, version: String, resolved: String, integrity: String) -> Self {
        Package {
            name,
            version,
            resolved,
            integrity,
            // v3 fields default: ecosystem=other, provenance=None …
            // Field v3 về mặc định: ecosystem=other, provenance=None …
            ..Default::default()
        }
    }

    /// Add dependency — Thêm dependency
    pub fn add_dependency(&mut self, dep: String) {
        self.dependencies.push(dep);
    }
}

impl SignatureFile {
    /// Create new signature file — Tạo file signature mới
    pub fn new(lockfile_hash: String, signature: String, key_id: String) -> Self {
        SignatureFile {
            lockfile_hash,
            signature,
            key_id,
            signed_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

// Impl FromStr trait instead of custom from_str() method
impl std::str::FromStr for SignatureFile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Parse format:
        // lockfile_hash = "blake3-..."
        // signature = "ed25519-..."
        // key_id = "..."
        // signed_at = "..."

        let mut lockfile_hash = None;
        let mut signature = None;
        let mut key_id = None;
        let mut signed_at = None;

        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');

                match key {
                    "lockfile_hash" => lockfile_hash = Some(value.to_string()),
                    "signature" => signature = Some(value.to_string()),
                    "key_id" => key_id = Some(value.to_string()),
                    "signed_at" => signed_at = Some(value.to_string()),
                    _ => {} // Ignore unknown fields — Bỏ qua field không biết
                }
            }
        }

        Ok(SignatureFile {
            lockfile_hash: lockfile_hash.ok_or("missing lockfile_hash")?,
            signature: signature.ok_or("missing signature")?,
            key_id: key_id.ok_or("missing key_id")?,
            signed_at: signed_at.ok_or("missing signed_at")?,
        })
    }
}

// A9 FIX (same as Week 1): Impl Display instead of inherent to_string()
impl std::fmt::Display for SignatureFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "# MagiCore Lockfile Signature v2\n\
             # DO NOT EDIT — Generated by mgc trust sign\n\
             lockfile_hash = \"{}\"\n\
             signature = \"{}\"\n\
             key_id = \"{}\"\n\
             signed_at = \"{}\"\n",
            self.lockfile_hash, self.signature, self.key_id, self.signed_at
        )
    }
}
