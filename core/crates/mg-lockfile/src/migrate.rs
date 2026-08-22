//! Lockfile v1 → v2 migration
//! Migration lockfile v1 → v2

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
        _ => Err(LockfileError::ParseError(format!(
            "unknown version: {}",
            version
        ))),
    }
}

/// Migrate lockfile v1 to v2 — Migrate lockfile v1 sang v2
pub fn migrate_v1_to_v2(lockfile_v1: LockfileV1) -> LockfileResult<Lockfile> {
    let mut lockfile_v2 = Lockfile::new();
    
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
        };
        
        lockfile_v2.add_package(pkg_v2);
    }
    
    Ok(lockfile_v2)
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

/// Auto-upgrade lockfile (v1 → v2) — Tự động nâng cấp lockfile
pub fn auto_upgrade_lockfile(toml_str: &str) -> LockfileResult<Lockfile> {
    let version = detect_lockfile_version(toml_str)?;
    
    match version {
        1 => {
            let lockfile_v1 = parse_lockfile_v1(toml_str)?;
            migrate_v1_to_v2(lockfile_v1)
        }
        2 => crate::parser::parse_lockfile(toml_str),
        _ => Err(LockfileError::ParseError(format!(
            "unsupported version: {}",
            version
        ))),
    }
}

/// L7 FIX: BLAKE3 placeholder hash for migration (cryptographic, not DefaultHasher)
/// Still placeholder — production should warn user to re-install packages
fn blake3_placeholder_hash(s: &str) -> String {
    use mg_crypto::blake3_signer::Blake3Hasher;
    let hash = Blake3Hasher::hash_string(s);
    hash.to_base64()
}
