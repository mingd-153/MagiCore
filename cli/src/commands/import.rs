//! Import legacy lockfiles to mg.lock format
//! Chuyển đổi lockfile legacy (package-lock.json, pnpm-lock.yaml, yarn.lock, bun.lock) sang định dạng mg.lock
//! 
//! FIXME: Temporarily disabled during lockfile v2 migration (Week 6)
//! Will be restored in V1.0.1 with v2 schema support

use anyhow::Result;

/// Run `mg import` in project directory.
/// Chuyển đổi lockfile cũ thành mg.lock chuẩn xác và an toàn.
pub async fn run(_project_dir: Option<std::path::PathBuf>) -> Result<()> {
    anyhow::bail!(
        "mg import is temporarily disabled during lockfile v2 migration.\n\
         This command will be restored in V1.0.1 with full v2 schema support."
    );
}
