//! Lockfile version migrations (v1 → v2, v2 → v3)
//! Migration phiên bản lockfile (v1 → v2, v2 → v3)

use crate::schema::{LOCKFILE_SCHEMA_VERSION, Provenance, SOURCE_KIND_REGISTRY_IMPORT};
use crate::{Lockfile, LockfileError, LockfileResult, Package};
use serde::{Deserialize, Serialize};

/// Lockfile v1 structure (legacy) — Cấu trúc lockfile v1 (legacy)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockfileV1 {
    pub version: String,
    #[serde(rename = "package")]
    pub packages: Vec<PackageV1>,
}

/// Package v1 structure (no integrity field) — Cấu trúc package v1 (không có integrity)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageV1 {
    pub name: String,
    pub version: String,
    pub resolved: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Detect lockfile version from TOML string — Phát hiện version lockfile từ chuỗi TOML
pub fn detect_lockfile_version(toml_str: &str) -> LockfileResult<u8> {
    // Try to parse version field
    let parsed: toml::Value = toml::from_str(toml_str)?;

    let version = parsed
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| LockfileError::ParseError("missing version field".to_string()))?;

    match version {
        "1" => Ok(1),
        "2" => Ok(2),
        "3" => Ok(3),
        _ => Err(LockfileError::ParseError(format!(
            "unknown version: {}",
            version
        ))),
    }
}

/// Migrate lockfile v1 to v2 — Migrate lockfile v1 sang v2
///
/// Emits the v2 shape on purpose (version pinned to "2") so the step stays
/// composable; `auto_upgrade_lockfile` chains it into `migrate_v2_to_v3`.
/// Cố ý xuất shape v2 (version ghim "2") để bước này ghép được;
/// `auto_upgrade_lockfile` sẽ nối tiếp bằng `migrate_v2_to_v3`.
pub fn migrate_v1_to_v2(lockfile_v1: LockfileV1) -> LockfileResult<Lockfile> {
    let mut lockfile_v2 = Lockfile::new();
    // Pin the intermediate schema version — the name of this function is a
    // contract (v1 → v2), the final bump happens in migrate_v2_to_v3.
    // Ghim version schema trung gian — tên hàm là hợp đồng (v1 → v2),
    // bước nâng cuối nằm ở migrate_v2_to_v3.
    lockfile_v2.version = "2".to_string();

    // Migrate packages
    for pkg_v1 in lockfile_v1.packages {
        let pkg_v2 = Package {
            name: pkg_v1.name.clone(),
            version: pkg_v1.version.clone(),
            resolved: pkg_v1.resolved.clone(),
            // L7 FIX: Use BLAKE3 for placeholder (still placeholder but cryptographic)
            // Production: download tarball and hash, or warn user to re-install
            integrity: format!("blake3-{}", blake3_placeholder_hash(&pkg_v1.resolved)),
            dependencies: pkg_v1.dependencies.clone(),
            // v3 fields keep their defaults (ecosystem=other, provenance=None);
            // migrate_v2_to_v3 fills provenance right after.
            // Field v3 giữ mặc định (ecosystem=other, provenance=None);
            // migrate_v2_to_v3 sẽ điền provenance ngay sau đó.
            ..Default::default()
        };

        lockfile_v2.add_package(pkg_v2);
    }

    Ok(lockfile_v2)
}

/// Migrate lockfile v2 to v3 — Migrate lockfile v2 sang v3
///
/// v2 files carry no ecosystem data, so after migration every package has
/// `ecosystem = other` (the serde default applied during the v2 parse) and
/// `provenance = registry-import` where provenance was empty. Ecosystem is
/// NOT force-reset: running the migration on a v3 lock must not clobber
/// existing ecosystem data.
/// File v2 không mang dữ liệu ecosystem, nên sau migration mọi package có
/// `ecosystem = other` (mặc định serde khi parse v2) và
/// `provenance = registry-import` ở chỗ provenance còn trống. KHÔNG ép
/// reset ecosystem: chạy migration trên lock v3 không được xoá dữ liệu
/// ecosystem sẵn có.
pub fn migrate_v2_to_v3(mut lockfile: Lockfile) -> Lockfile {
    lockfile.version = LOCKFILE_SCHEMA_VERSION.to_string();

    for pkg in &mut lockfile.packages {
        if pkg.provenance.is_none() {
            pkg.provenance = Some(Provenance {
                source_kind: SOURCE_KIND_REGISTRY_IMPORT.to_string(),
                tool: None,
                imported_from: None,
            });
        }
    }

    lockfile
}

/// Parse lockfile v1 from TOML string — Parse lockfile v1 từ chuỗi TOML
pub fn parse_lockfile_v1(toml_str: &str) -> LockfileResult<LockfileV1> {
    let lockfile_v1: LockfileV1 = toml::from_str(toml_str)?;

    if lockfile_v1.version != "1" {
        return Err(LockfileError::ParseError(format!(
            "expected version 1, got {}",
            lockfile_v1.version
        )));
    }

    Ok(lockfile_v1)
}

/// Auto-upgrade lockfile to the latest schema (v1/v2 → v3) — Tự động nâng cấp lockfile lên schema mới nhất (v1/v2 → v3)
pub fn auto_upgrade_lockfile(toml_str: &str) -> LockfileResult<Lockfile> {
    let version = detect_lockfile_version(toml_str)?;

    match version {
        1 => {
            let lockfile_v1 = parse_lockfile_v1(toml_str)?;
            // Chain both steps so old locks land on the canonical v3 graph.
            // Nối cả hai bước để lock cũ về đồ thị canonical v3.
            Ok(migrate_v2_to_v3(migrate_v1_to_v2(lockfile_v1)?))
        }
        2 => {
            let lockfile = crate::parser::parse_lockfile(toml_str)?;
            Ok(migrate_v2_to_v3(lockfile))
        }
        3 => crate::parser::parse_lockfile(toml_str),
        _ => Err(LockfileError::ParseError(format!(
            "unsupported version: {}",
            version
        ))),
    }
}

/// L7 FIX: BLAKE3 placeholder hash for migration (cryptographic, not DefaultHasher)
/// Still placeholder — production should warn user to re-install packages
fn blake3_placeholder_hash(s: &str) -> String {
    use mgc_crypto::blake3_signer::Blake3Hasher;
    let hash = Blake3Hasher::hash_string(s);
    hash.to_base64()
}
