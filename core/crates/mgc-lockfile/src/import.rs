//! Legacy lockfile migration parser (Phase 1A).
//!
//! FIXME(V1.0.1): Full module disabled pending lockfile v2 migration.
//! Only detect_legacy_lockfiles remains active for migration hints.

use std::path::Path;

pub const NPM_LOCKFILE: &str = "package-lock.json";
pub const PNPM_LOCKFILE: &str = "pnpm-lock.yaml";
pub const YARN_LOCKFILE: &str = "yarn.lock";
pub const BUN_LOCKFILE: &str = "bun.lock";

pub const ALL: [&str; 4] = [NPM_LOCKFILE, PNPM_LOCKFILE, YARN_LOCKFILE, BUN_LOCKFILE];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyLockfile {
    pub file_name: &'static str,
    pub path: std::path::PathBuf,
}

/// Detect supported legacy lockfiles without importing them.
/// Migration must be explicit; install callers should use this only for hints.
pub fn detect_legacy_lockfiles(project_root: &Path) -> Vec<LegacyLockfile> {
    ALL.iter()
        .filter_map(|name| {
            let path = project_root.join(name);
            path.exists().then_some(LegacyLockfile {
                file_name: name,
                path,
            })
        })
        .collect()
}

/// Phát hiện nguy cơ Trust-Downgrade: Dự án đã có `mgc.lock` (bảo mật cao, có checksum & chữ ký BLAKE3)
/// nhưng lại xuất hiện file lockfile cũ chưa được đồng bộ hoặc bị dev dùng tool cũ ghi đè.
pub fn check_trust_downgrade_risk(project_root: &Path) -> Option<Vec<&'static str>> {
    let mgc_lock = project_root.join("mgc.lock");
    if !mgc_lock.exists() {
        return None;
    }
    let legacy = detect_legacy_lockfiles(project_root);
    if legacy.is_empty() {
        None
    } else {
        Some(legacy.into_iter().map(|l| l.file_name).collect())
    }
}
