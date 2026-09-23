//! Import legacy lockfiles to mgc.lock format
//! Chuyển đổi lockfile legacy (package-lock.json, pnpm-lock.yaml, yarn.lock, bun.lock)
//! sang mgc.lock schema v2 — parser dữ liệu thuần, không gọi/wrap PM nào.

use anyhow::Result;
use std::path::PathBuf;

/// Run `mgc import` in project directory.
/// Chuyển đổi lockfile cũ thành mgc.lock v2; có key mặc định trong keyring thì ký,
/// chưa có key phải có --allow-unsigned tường minh mới được ghi unsigned
/// (RULE §11: escape hatch phải lên tiếng + opt-in rõ). Mọi lỗi khác fail cứng.
/// Lock + signature publish như một transaction (rollback cả hai khi verify lỗi).
pub async fn run(project_dir: Option<PathBuf>, allow_unsigned: bool) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = project_dir.map_or(cwd, |dir| {
        mgc_config::project::ProjectConfig::find_project_root(&dir).unwrap_or(dir)
    });
    let root = mgc_config::project::ProjectConfig::find_project_root(&root).unwrap_or(root);

    // Writer lock FIRST (P0-3/P0-4-class): the legacy-lock parse below
    // is the authoritative input — parsing it pre-lock lets the source
    // change between parse and commit. Lock, then read.
    // (Lock trước, parse sau — input authoritative trong critical section.)
    let _guard = mgc_lockfile::project_lock::ProjectWriteLock::acquire(
        &root,
        crate::commands::core::shared::writer_lock_timeout(&root),
    )
    .map_err(|e| anyhow::anyhow!("import cannot acquire the project writer lock: {e}"))?;
    // Pending-journal protocol (P0-3): refuse over an unrestored
    // mutation journal instead of paving over it.
    // (Không commit đè lên journal chưa phục hồi.)
    crate::commands::core::shared::ensure_no_pending_remove_journal(&root, &_guard)?;

    let (mut lockfile, report) = mgc_lockfile::import_into_lockfile(&root)?;
    for warning in &report.warnings {
        mgc_ui::warning(warning);
    }
    // P0 finding #8 (2026-09-12): migration is BEST-EFFORT — every
    // refused record is surfaced; a skipped entry must NEVER be silent
    // (the report IS the loss ledger; "lossless" is never claimed).
    // P0 finding #8: migration là BEST-EFFORT — mọi record bị từ chối
    // đều hiển thị; entry bị skip KHÔNG BAO GIỜ im lặng (báo cáo chính
    // là sổ mất mát; không claim "lossless").
    for skipped in &report.skipped {
        mgc_ui::warning(&format!("skipped '{}': {}", skipped.key, skipped.reason));
    }
    // P0 finding #6: surface the reconstructed ROOT GRAPH so users can
    // see the root → dependency edges the import preserved.
    // P0 finding #6: hiển thị graph ROOT được dựng lại để user thấy các
    // cạnh root → dependency mà import giữ được.
    if !lockfile.root_dependencies.is_empty() {
        mgc_ui::info(&format!(
            "root dependencies preserved: {}",
            lockfile.root_dependencies.join(", ")
        ));
    }
    let lock_path = root.join("mgc.lock");
    let sig_path = lock_path.with_extension("lock.sig");

    // Snapshot pre-existing artifacts for transactional rollback (P0-2):
    // a failed sign/verify below restores BOTH files, never half a pair.
    // (Snapshot lock+sig — lỗi là rollback cả hai.)
    let prior_lock = std::fs::read(&lock_path).ok();
    let prior_sig = std::fs::read(&sig_path).ok();
    let rollback_artifacts = |lock_bytes: &Option<Vec<u8>>,
                              sig_bytes: &Option<Vec<u8>>|
     -> Result<()> {
        match lock_bytes {
            Some(bytes) => mgc_lockfile::atomic::atomic_write_locked(
                &_guard,
                &lock_path,
                bytes,
                std::time::Duration::from_secs(60),
            )
            .map_err(|e| anyhow::anyhow!("import rollback of mgc.lock failed: {e}")),
            None => std::fs::remove_file(&lock_path)
                .map_err(|e| anyhow::anyhow!("import rollback (remove new mgc.lock) failed: {e}")),
        }?;
        match sig_bytes {
            Some(bytes) => mgc_lockfile::atomic::atomic_write_locked(
                &_guard,
                &sig_path,
                bytes,
                std::time::Duration::from_secs(60),
            )
            .map_err(|e| anyhow::anyhow!("import rollback of signature failed: {e}")),
            None => {
                if sig_path.exists() {
                    std::fs::remove_file(&sig_path).map_err(|e| {
                        anyhow::anyhow!("import rollback (remove new signature) failed: {e}")
                    })?;
                }
                Ok(())
            }
        }
    };

    // Typed signing decision (P0-2): ONLY a missing key may take the
    // unsigned escape hatch, and ONLY with explicit --allow-unsigned.
    // Corrupt keyring/crypto/IO errors fail HARD — never downgrade.
    // (Chỉ thiếu key + opt-in mới unsigned — lỗi khác fail cứng.)
    let has_default_key = mgc_crypto::keyring::Keyring::init_if_not_exists()
        .map_err(|e| anyhow::anyhow!("import cannot access the signing keyring: {e}"))?
        .default_key()
        .is_some();
    let signed = if has_default_key {
        mgc_lockfile::sign_lockfile_with_default_key(&mut lockfile, &lock_path).map_err(|e| {
            anyhow::anyhow!("import signing failed (refusing unsigned downgrade): {e}")
        })?;
        true
    } else if allow_unsigned {
        mgc_ui::warning(
            "EXPLICIT --allow-unsigned: writing UNSIGNED mgc.lock (no default signing key) — run `mgc trust sign` after generating a key",
        );
        mgc_lockfile::atomic::atomic_write_locked(
            &_guard,
            &lock_path,
            mgc_lockfile::serialization::to_toml(&lockfile)?.as_bytes(),
            std::time::Duration::from_secs(60),
        )
        .map_err(|e| anyhow::anyhow!("import unsigned lock write failed: {e}"))?;
        false
    } else {
        return Err(anyhow::anyhow!(
            "import refused: no default signing key and --allow-unsigned not given (unsigned lockfiles are never the default) — generate a key (`mgc trust`) or re-run with --allow-unsigned"
        ));
    };

    // Self-check roundtrip: the signature must verify immediately; a
    // failure rolls back BOTH artifacts (never half a signed pair).
    // (Verify lỗi thì rollback cả lock lẫn sig.)
    if signed && let Err(e) = mgc_lockfile::load_and_verify_lockfile(&lock_path, &sig_path) {
        rollback_artifacts(&prior_lock, &prior_sig)?;
        return Err(anyhow::anyhow!(
            "post-write verification failed, rolled back: {e}"
        ));
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
