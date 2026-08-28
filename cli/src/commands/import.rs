//! Import legacy lockfiles to mgc.lock format
//! Chuyển đổi lockfile legacy (package-lock.json, pnpm-lock.yaml, yarn.lock, bun.lock)
//! sang mgc.lock schema v2 — parser dữ liệu thuần, không gọi/wrap PM nào.

use anyhow::Result;
use std::path::PathBuf;

/// Run `mgc import` in project directory.
/// Chuyển đổi lockfile cũ thành mgc.lock v2; có key mặc định trong keyring thì ký,
/// chưa có → ghi unsigned kèm cảnh báo rõ (RULE §11: escape hatch phải lên tiếng).
pub async fn run(project_dir: Option<PathBuf>) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = project_dir.map_or(cwd, |dir| {
        mgc_config::project::ProjectConfig::find_project_root(&dir).unwrap_or(dir)
    });
    let root = mgc_config::project::ProjectConfig::find_project_root(&root).unwrap_or(root);

    let (mut lockfile, report) = mgc_lockfile::import_into_lockfile(&root)?;
    for warning in &report.warnings {
        mgc_ui::warning(warning);
    }
    let lock_path = root.join("mgc.lock");

    let signed = match mgc_lockfile::sign_lockfile_with_default_key(&mut lockfile, &lock_path) {
        Ok(()) => true,
        Err(e) => {
            // Không có key → ghi unsigned + cảnh báo (không im lặng)
            mgc_ui::warning(&format!(
                "writing UNSIGNED mgc.lock (no default signing key: {e}) — run `mgc trust sign` after generating a key"
            ));
            mgc_lockfile::write_lockfile(&lockfile, &lock_path)?;
            false
        }
    };

    // Self-check roundtrip: chữ ký ghi ra phải đọc-lại-verify được ngay
    if signed {
        mgc_lockfile::load_and_verify_lockfile(&lock_path, &lock_path.with_extension("lock.sig"))
            .map_err(|e| anyhow::anyhow!("post-write verification failed: {e}"))?;
    }

    mgc_ui::success(&format!(
        "Imported {} packages from {} into mgc.lock{}",
        report.packages,
        report.source_file,
        if signed { " (signed)" } else { " (unsigned)" }
    ));

    // Cảnh báo trust-downgrade: legacy file vẫn còn nằm cạnh mgc.lock mới
    if let Some(remaining) = mgc_lockfile::check_trust_downgrade_risk(&root) {
        mgc_ui::warning(&format!(
            "legacy lockfile(s) still present alongside the new mgc.lock: {} — consider removing them to avoid confusion",
            remaining.join(", ")
        ));
    }

    Ok(())
}
