//! Lockfile schema v4 — identity, naming, peer contexts, sources.
//! Identity, đặt tên, peer context, nguồn cho schema lockfile v4.
//!
//! v4 fixes three independent v3 defects (each alone justifies the major
//! bump): identity keyed by `(name, version)` cannot represent npm peer
//! instances or same-name-different-registry pins; the on-disk
//! `metadata.lockfile_hash` is always empty; and source/marker/target
//! decisions have nowhere to live.
//! (v4 sửa ba lỗi v3 độc lập (mỗi lỗi đủ để bump major): identity theo
//! `(name, version)` không biểu diễn được instance peer npm hay pin cùng
//! tên khác registry; `metadata.lockfile_hash` trên đĩa luôn rỗng; và
//! quyết định source/marker/target không có chỗ ghi.)

use serde::{Deserialize, Serialize};

use crate::ecosystem_tag::EcosystemTag;

/// Canonical v4 schema version — Phiên bản schema v4 canonical.
pub const LOCKFILE_SCHEMA_V4: &str = "4";

/// Fallback priority for the labeled "unknown" source: higher than any
/// real priority, low enough for TOML's i64-only integers.
/// (Priority cho nguồn "unknown": cao hơn mọi priority thật, đủ thấp cho
/// integer i64-only của TOML.)
pub const UNKNOWN_SOURCE_PRIORITY: u64 = 1_000_000_000;

/// Unique identity of ONE locked instance. Two entries sharing a
/// PackageKey are the same instance; anything else (peer set, features,
/// target, source) makes a different instance.
/// Danh tính duy nhất của MỘT instance đã lock. Hai entry cùng
/// PackageKey là một instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageKey {
    /// Ecosystem of the pin — Hệ sinh thái của pin.
    pub ecosystem: EcosystemTag,
    /// Canonical name (`canonical_name`) — Tên canonical.
    pub name: String,
    /// Canonical version string (`canonical_version`) — Chuỗi version canonical.
    pub version: String,
    /// Foreign key into the lockfile `[sources]` table (§5) — Khóa ngoại
    /// tới bảng `[sources]` của lockfile.
    pub source_id: String,
    /// What makes this instance distinct beyond name/version/source.
    #[serde(default)]
    pub variant: VariantKey,
}

/// Per-instance variation axis — Trục biến thể của từng instance.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VariantKey {
    /// Digest of the resolved peer set (`peer_context_digest`), None when
    /// the ecosystem has no peers — Digest của tập peer đã chốt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_context: Option<String>,
    /// Sorted enabled feature set (cargo features, extras) — Tập feature
    /// đã bật (đã sắp xếp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature_set: Option<Vec<String>>,
    /// Canonical target tuple (`target_tuple`) or None for
    /// platform-independent packages — Tuple target canonical hoặc None
    /// cho package không phụ thuộc nền tảng.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// Canonical target tuple — Tuple target canonical.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TargetTuple {
    /// Operating system (`linux`, `darwin`, `win32`) — Hệ điều hành.
    pub os: String,
    /// Architecture (`x86_64`, `aarch64`) — Kiến trúc.
    pub arch: String,
    /// ABI/libc (`gnu`, `musl`, `msvc`, …) — ABI/libc.
    pub abi: String,
}

impl TargetTuple {
    /// Parse `os-arch-abi` (exactly three dash-separated parts).
    /// Parse `os-arch-abi` (đúng ba phần cách bằng gạch nối).
    pub fn parse(raw: &str) -> Option<Self> {
        let mut parts = raw.split('-');
        let (os, arch, abi) = (parts.next()?, parts.next()?, parts.next()?);
        if os.is_empty() || arch.is_empty() || abi.is_empty() || parts.next().is_some() {
            return None;
        }
        Some(Self {
            os: os.to_string(),
            arch: arch.to_string(),
            abi: abi.to_string(),
        })
    }

    /// Canonical string form — Dạng chuỗi canonical.
    pub fn canonical(&self) -> String {
        format!("{}-{}-{}", self.os, self.arch, self.abi)
    }
}

/// Structured dependency edge — Cạnh phụ thuộc có cấu trúc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    /// Full key of the target entry — Khóa đầy đủ của entry đích.
    pub target_key: PackageKey,
    /// Originally declared range — Range gốc được khai báo.
    pub range: String,
    /// Edge kind — Loại cạnh.
    pub kind: EdgeKind,
    /// Where the edge came from — Cạnh từ đâu ra.
    pub origin: EdgeOrigin,
    /// Evaluated marker (PEP 508 / cfg expression) — Marker đã evaluate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
}

/// Dependency edge kind — Loại cạnh phụ thuộc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    Normal,
    Dev,
    Optional,
    Peer,
}

/// Edge origin — Nguồn gốc cạnh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeOrigin {
    /// Declared in the project manifest — Khai báo trong manifest project.
    Manifest,
    /// Transitive edge from another locked entry — Cạnh bắc cầu từ entry
    /// đã lock khác.
    Transitive {
        /// Key of the entry carrying this edge — Khóa entry mang cạnh này.
        from_key: Box<PackageKey>,
    },
}

/// A registry/index source — Một nguồn registry/index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    /// Stable id referenced by `PackageKey.source_id` — Id ổn định mà
    /// `PackageKey.source_id` trỏ tới.
    pub id: String,
    /// Canonical base URL (no trailing slash, lowercase scheme+host).
    pub url: String,
    /// Ecosystem this source serves — Hệ sinh thái nguồn này phục vụ.
    pub ecosystem: EcosystemTag,
    /// Lower number wins among equal-specificity claims — Số nhỏ thắng
    /// giữa các claim cùng độ đặc hiệu.
    #[serde(default)]
    pub priority: u64,
    /// Name patterns this source claims (`*`, `corp-*`, `@corp/*`,
    /// exact names) — Pattern tên nguồn này tuyên bố sở hữu.
    #[serde(default)]
    pub claims: Vec<String>,
    /// Private/internal source (never overridden by public ones).
    #[serde(default)]
    pub trusted: bool,
    /// Exact hosts allowed when `trusted` (transport guard §5.3.1).
    #[serde(default)]
    pub allow_hosts: Vec<String>,
    /// Private CIDRs allowed when `trusted` — Dải CIDR private cho phép.
    #[serde(default)]
    pub allow_cidrs: Vec<String>,
    /// URL schemes allowed when `trusted` (default HTTPS only).
    #[serde(default)]
    pub allow_protocols: Vec<String>,
}

/// Inline signature block (single-file lock, no sidecar) — Khối chữ ký
/// nhúng (lock một file, không sidecar).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureBlock {
    /// Signature algorithm (`ed25519`) — Thuật toán chữ ký.
    pub algorithm: String,
    /// Key id (first 8 bytes of BLAKE3(pubkey), hex) — Id khóa.
    pub key_id: String,
    /// Base64 public key (offline verification without a keyring).
    pub public_key: String,
    /// Digest of the canonical payload (`blake3-<base64>`) — Digest của
    /// payload canonical.
    pub digest: String,
    /// Signing timestamp (out-of-payload) — Thời điểm ký.
    pub signed_at: String,
    /// The signature (`ed25519-<base64>`) — Chữ ký.
    pub signature: String,
}

/// Canonical package name per ecosystem.
/// - npm: lowercase.
/// - PyPI: PEP 503 (`[-_.]+` → `-`, lowercase).
/// - NuGet: lowercase.
/// - everything else: trimmed as-is.
///
/// Tên canonical theo ecosystem.
pub fn canonical_name(ecosystem: EcosystemTag, raw: &str) -> String {
    let trimmed = raw.trim();
    match ecosystem {
        EcosystemTag::Web | EcosystemTag::NuGet => trimmed.to_ascii_lowercase(),
        EcosystemTag::Python => {
            let mut out = String::with_capacity(trimmed.len());
            let mut last_dash = false;
            for ch in trimmed.chars() {
                if ch == '-' || ch == '_' || ch == '.' {
                    if !last_dash {
                        out.push('-');
                    }
                    last_dash = true;
                } else {
                    for lower in ch.to_lowercase() {
                        out.push(lower);
                    }
                    last_dash = false;
                }
            }
            out
        }
        _ => trimmed.to_string(),
    }
}

/// Canonical version string: `1.2` → `1.2.0`, `1` → `1.0.0`; longer
/// versions and pre-release/build suffixes pass through unchanged.
/// Chuỗi version canonical.
pub fn canonical_version(raw: &str) -> String {
    let trimmed = raw.trim();
    let (core, suffix) = match trimmed.find(['-', '+']) {
        Some(idx) => trimmed.split_at(idx),
        None => (trimmed, ""),
    };
    let parts: Vec<&str> = core.split('.').collect();
    let normalized = match parts.as_slice() {
        [major] => format!("{major}.0.0"),
        [major, minor] => format!("{major}.{minor}.0"),
        _ => core.to_string(),
    };
    format!("{normalized}{suffix}")
}

/// Digest of a resolved peer set: `blake3(canonical_join(sorted(
/// [(peer_name, resolved_version), …])))`, hex-encoded. The lock also
/// stores the canonical `peer_contexts` table so verify/replay/audit
/// recompute the digest instead of trusting it blindly.
/// Digest của tập peer đã chốt. Lock còn lưu bảng `peer_contexts`
/// canonical để verify tính lại, không tin mù.
pub fn peer_context_digest(peers: &[(String, String)]) -> String {
    use mgc_crypto::blake3_signer::Blake3Hasher;
    let mut sorted: Vec<(&str, &str)> = peers
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    sorted.sort();
    let joined = sorted
        .iter()
        .map(|(n, v)| format!("{n}@{v}"))
        .collect::<Vec<_>>()
        .join(",");
    Blake3Hasher::hash_string(&joined).to_hex()
}

/// Claim specificity rank (higher wins; catch-all `*` is rank 0 and only
/// ever a fallback) — Hạng đặc hiệu của claim (cao thắng; `*` hạng 0 và
/// chỉ là fallback).
/// - 4: exact name (no wildcard).
/// - 3: scoped prefix (`@corp/*`).
/// - 2: infix/prefix wildcard (`corp-*`, `foo-*-bar`).
/// - 1: suffix wildcard (`*-corp`).
/// - 0: catch-all `*`.
///
/// Multiple wildcards degrade to the LOWEST matching rank (conservative).
pub fn claim_specificity(pattern: &str) -> u8 {
    if pattern == "*" {
        return 0;
    }
    if !pattern.contains('*') {
        return 4;
    }
    let stars = pattern.matches('*').count();
    if stars > 1 {
        // Multiple wildcards degrade to the lowest rank (conservative).
        // (Nhiều wildcard hạ về hạng thấp nhất (thận trọng).)
        return 1;
    }
    let looks_scoped_prefix = pattern.starts_with('@') && pattern.ends_with("/*");
    let looks_suffix = pattern.starts_with("*-");
    if looks_scoped_prefix {
        3
    } else if looks_suffix {
        1
    } else {
        // Single infix/prefix wildcard (`corp-*`, `foo-*-bar`).
        2
    }
}

/// Match a package name against a claim pattern (`*` = any run).
/// Khớp tên package với pattern claim.
pub fn claim_matches(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == name;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut rest = name;
    if !pattern.starts_with('*') {
        let Some(tail) = rest.strip_prefix(parts[0]) else {
            return false;
        };
        rest = tail;
    }
    for part in &parts[1..parts.len() - 1] {
        let Some(idx) = rest.find(part) else {
            return false;
        };
        rest = &rest[idx + part.len()..];
    }
    if !pattern.ends_with('*') {
        return rest.ends_with(parts[parts.len() - 1]);
    }
    true
}

/// Source-selection errors — Lỗi chọn nguồn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceSelectionError {
    /// Two different sources claim the package at the same specificity —
    /// AmbiguousSource. Hai nguồn khác nhau tuyên bố cùng độ đặc hiệu.
    AmbiguousSource { package: String },
    /// Several catch-all sources share the lowest priority —
    /// AmbiguousFallback. Nhiều nguồn fallback cùng priority thấp nhất.
    AmbiguousFallback { package: String },
    /// A public source would override a trusted specific claim —
    /// PublicOverridePrivate. Nguồn public đè claim specific tin cậy.
    PublicOverridePrivate { package: String, source: String },
}

impl std::fmt::Display for SourceSelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AmbiguousSource { package } => write!(
                f,
                "ambiguous source for '{package}': multiple sources claim it at equal specificity"
            ),
            Self::AmbiguousFallback { package } => write!(
                f,
                "ambiguous fallback for '{package}': multiple catch-all sources share the lowest priority"
            ),
            Self::PublicOverridePrivate { package, source } => write!(
                f,
                "public source '{source}' would override a trusted claim for '{package}'"
            ),
        }
    }
}

impl std::error::Error for SourceSelectionError {}

/// Specificity-first source selection (§5.3): exact/specific claims beat
/// catch-all fallbacks; equal-specificity conflicts across sources fail;
/// public sources never override trusted specific claims.
/// Chọn nguồn theo độ đặc hiệu trước (§5.3).
pub fn select_source<'a>(
    package: &str,
    sources: &'a [SourceRef],
) -> Result<&'a SourceRef, SourceSelectionError> {
    let mut matches: Vec<(&'a SourceRef, u8)> = Vec::new();
    for source in sources {
        let mut best: Option<u8> = None;
        for pattern in &source.claims {
            if claim_matches(pattern, package) {
                let rank = claim_specificity(pattern);
                best = Some(best.map_or(rank, |b: u8| b.max(rank)));
            }
        }
        if let Some(rank) = best {
            matches.push((source, rank));
        }
    }
    if matches.is_empty() {
        // Sole fallback road: lowest priority number wins.
        // (Đường fallback duy nhất: số priority nhỏ nhất thắng.)
        return sources.iter().min_by_key(|s| s.priority).ok_or(
            SourceSelectionError::AmbiguousFallback {
                package: package.to_string(),
            },
        );
    }
    let max_spec = matches.iter().map(|(_, r)| *r).max().unwrap_or(0);
    if max_spec == 0 {
        let top: Vec<&&SourceRef> = matches.iter().map(|(s, _)| s).collect();
        let min_priority = top.iter().map(|s| s.priority).min().unwrap_or(u64::MAX);
        let winners: Vec<&&SourceRef> = top
            .into_iter()
            .filter(|s| s.priority == min_priority)
            .collect();
        if winners.len() > 1 {
            return Err(SourceSelectionError::AmbiguousFallback {
                package: package.to_string(),
            });
        }
        let chosen = winners[0];
        // Fallback-level anti-confusion: a winning UNTRUSTED catch-all
        // loses to ANY trusted match — specificity-first already
        // guarantees no trusted source claims this package above rank 0,
        // so falling back to public while a trusted source matches is a
        // misconfiguration, failed loudly instead of resolved quietly.
        // (Chống nhầm lẫn ở tầng fallback: catch-all không-tin-cậy thắng
        // vẫn thua BẤT KỲ match tin-cậy nào — specificity-first đã đảm
        // bảo không có nguồn tin cậy nào claim package này trên hạng 0.)
        if !chosen.trusted
            && sources.iter().any(|s| {
                s.trusted && s.id != chosen.id && s.claims.iter().any(|p| claim_matches(p, package))
            })
        {
            return Err(SourceSelectionError::PublicOverridePrivate {
                package: package.to_string(),
                source: chosen.id.clone(),
            });
        }
        return Ok(chosen);
    }
    let mut top: Vec<&SourceRef> = matches
        .iter()
        .filter(|(_, r)| *r == max_spec)
        .map(|(s, _)| *s)
        .collect();
    top.sort_by_key(|s| s.id.clone());
    top.dedup_by_key(|s| s.id.clone());
    if top.len() > 1 {
        return Err(SourceSelectionError::AmbiguousSource {
            package: package.to_string(),
        });
    }
    // Specific branch needs no override check: the winner IS the maximum
    // specificity, so no trusted source can claim this package above it
    // (equal rank would have tied into AmbiguousSource above).
    // (Nhánh specific không cần check override: người thắng ĐÃ là độ đặc
    // hiệu tối đa.)
    Ok(top[0])
}
