//! `script_policy_tests.rs` — F5: a PRESENT but broken `[scripts]`
//! table must fail the install (hard error), never warn-and-ignore.
//! Missing file/table stays Ok(None) — only breakage fails.
//! Bảng `[scripts]` hỏng phải lỗi cứng; thiếu file vẫn Ok(None).

use crate::install::script_policy::load_file_scripts_policy;

fn tmp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-script-policy-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn broken_scripts_toml_fails_closed() {
    // P0/F5-RED: a syntactically broken mgc.toml must Err (a deny the
    // user believes is active must never be silently dropped).
    let dir = tmp("broken");
    std::fs::write(dir.join("mgc.toml"), "[scripts\npolicy = \"allow\"\n").unwrap();
    assert!(
        load_file_scripts_policy(&dir).is_err(),
        "broken [scripts] TOML must fail, not warn-and-ignore"
    );
}

#[test]
fn wrongly_typed_scripts_table_fails_closed() {
    // Present table with the wrong shape (deny must be a list) → Err.
    let dir = tmp("wrongtype");
    std::fs::write(dir.join("mgc.toml"), "[scripts]\ndeny = 42\n").unwrap();
    assert!(
        load_file_scripts_policy(&dir).is_err(),
        "wrongly-typed [scripts] table must fail"
    );
}

#[test]
fn missing_file_or_table_is_no_opinion() {
    // No file / no table → Ok(None): a missing policy must not break
    // installs.
    let bare = tmp("bare");
    assert!(load_file_scripts_policy(&bare).unwrap().is_none());
    let no_table = tmp("notable");
    std::fs::write(no_table.join("mgc.toml"), "name = \"x\"\n").unwrap();
    assert!(load_file_scripts_policy(&no_table).unwrap().is_none());
}

/// Nợ 2-RED: stale snapshot dirs (`.mgc-snap-*`, left by SIGKILLed
/// installs — TempDir never cleans on kill) must be swept at the next
/// install start. Safe: the project install lock forbids concurrent
/// installs in one project, so every prefixed dir here is garbage.
/// Thư mục snapshot cũ (SIGKILL) phải được quét dọn ở lần install sau.
#[test]
fn sweep_stale_snapshots_removes_only_prefixed_dirs() {
    use crate::install::sweep_stale_snapshots;
    let dir = tmp("sweep");
    std::fs::create_dir_all(dir.join(".mgc-snap-deadbeef")).unwrap();
    std::fs::write(dir.join(".mgc-snap-deadbeef").join("x"), b"stale").unwrap();
    std::fs::create_dir_all(dir.join("node_modules")).unwrap();
    std::fs::write(dir.join("mgc.toml"), "name = \"x\"\n").unwrap();
    let swept = sweep_stale_snapshots(&dir);
    assert_eq!(swept, 1, "one stale snapshot must go");
    assert!(!dir.join(".mgc-snap-deadbeef").exists());
    assert!(dir.join("node_modules").exists(), "real tree untouched");
    assert!(dir.join("mgc.toml").is_file(), "files untouched");
}

/// P0-3-RED: an unreadable trust DB must Err (fail-closed), never an
/// empty map that silently drops believed-active denies.
/// Trust DB không đọc được phải lỗi cứng, không map rỗng.
#[test]
fn trust_db_unreadable_fails_closed() {
    use crate::install::script_policy::load_trust_policies;
    // A layout whose DB path sits under a missing directory tree cannot
    // open — the loader must surface it, not default to empty.
    let missing = std::env::temp_dir().join(format!(
        "mgc-trust-broken-{}-no-such-dir",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&missing);
    let layout = mgc_store::Layout::new(missing.join("store"));
    assert!(
        load_trust_policies(&layout).is_err(),
        "unopenable trust DB must fail closed"
    );
}

/// P1-RED (sweep): a hostile `.mgc-snap-*` SYMLINK must not cause
/// deletion outside the project — and counts nothing.
/// Symlink `.mgc-snap-*` độc hại không được gây xóa ngoài project.
#[cfg(unix)]
#[test]
fn sweep_never_follows_hostile_snapshot_symlink() {
    use crate::install::sweep_stale_snapshots;
    let dir = tmp("sweep-link");
    let outside = tmp("sweep-victim");
    std::fs::write(outside.join("precious.txt"), b"keep").unwrap();
    std::os::unix::fs::symlink(&outside, dir.join(".mgc-snap-evil")).unwrap();
    let swept = sweep_stale_snapshots(&dir);
    assert_eq!(swept, 0, "symlink must not count as swept");
    assert!(
        outside.join("precious.txt").is_file(),
        "outside target must survive"
    );
    assert!(
        std::fs::symlink_metadata(dir.join(".mgc-snap-evil"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "the hostile link itself is left alone"
    );
}

/// P0-1-RED: pre-install lock bytes come back after failure; absent
/// prior means the failed write is removed (fail-closed, never silent).
/// Lock cũ về sau fail; chưa từng có thì xóa file đã ghi hỏng.
/// P0-2-RED: TreeBackup take/rollback/commit + Drop auto-restore; crash
/// recovery restores backups and sweeps staging litter.
/// Guard take/rollback/commit + Drop tự khôi phục; recovery dựng backup.
#[test]
fn restore_prior_lock_roundtrips() {
    use crate::install::restore_prior_lock_result;
    let dir = tmp("restore");
    let path = dir.join("mgc.lock");
    std::fs::write(&path, b"v1-bytes").unwrap();
    restore_prior_lock_result(&dir, &Some(b"v0-bytes".to_vec())).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"v0-bytes");
    restore_prior_lock_result(&dir, &None).unwrap();
    assert!(
        !path.exists(),
        "no prior lock means the failed write is removed"
    );
    restore_prior_lock_result(&dir, &None).unwrap();
}

#[test]
fn tree_backup_take_rollback_commit_cycle() {
    use crate::install::TreeBackup;
    let dir = tmp("guard");
    let live = dir.join("node_modules");
    std::fs::create_dir_all(live.join("pkg")).unwrap();
    std::fs::write(live.join("pkg").join("a.txt"), b"orig").unwrap();

    // take(): live moved aside atomically, fresh live recreated.
    let mut guard = TreeBackup::take(&live).unwrap();
    assert!(live.is_dir(), "fresh live dir recreated");
    assert!(!live.join("pkg").exists(), "fresh live starts empty");
    std::fs::write(live.join("new.txt"), b"partial").unwrap();

    // rollback(): pre-install tree back, byte-identical.
    guard.rollback().unwrap();
    assert_eq!(
        std::fs::read(live.join("pkg").join("a.txt")).unwrap(),
        b"orig"
    );
    assert!(!live.join("new.txt").exists());

    // commit(): backup gone, Drop becomes a no-op.
    let guard2 = TreeBackup::take(&live).unwrap();
    std::fs::write(live.join("b.txt"), b"new").unwrap();
    guard2.commit();
    assert!(live.join("b.txt").is_file(), "committed tree stands");
    assert!(!live.join("pkg").exists(), "old tree retired with backup");
}

#[test]
fn tree_backup_drop_auto_restores() {
    use crate::install::TreeBackup;
    let dir = tmp("guard-drop");
    let live = dir.join("node_modules");
    std::fs::create_dir_all(&live).unwrap();
    std::fs::write(live.join("keep.txt"), b"k").unwrap();
    {
        let _guard = TreeBackup::take(&live).unwrap();
        std::fs::write(live.join("junk.txt"), b"j").unwrap();
        // No commit, no rollback: Drop must restore.
    }
    assert_eq!(std::fs::read(live.join("keep.txt")).unwrap(), b"k");
    assert!(
        !live.join("junk.txt").exists(),
        "Drop restored pre-install tree"
    );
}

#[test]
fn recover_interrupted_install_restores_and_sweeps() {
    use crate::install::recover_interrupted_install;
    let dir = tmp("recover");
    let live = dir.join("node_modules");
    // Simulate SIGKILL mid-install: stale staging + backup, no live tree.
    let staging = dir.join(".mgc-stage-dead");
    std::fs::create_dir_all(&staging).unwrap();
    let backup = dir.join(".mgc-prev-1-2-3");
    std::fs::create_dir_all(backup.join("pkg")).unwrap();
    std::fs::write(backup.join("pkg").join("a.txt"), b"orig").unwrap();
    // Hostile symlink must be refused, never restored.
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc", dir.join(".mgc-prev-evil")).unwrap();

    let out = recover_interrupted_install(&dir, &live, &dir);
    assert!(out.restored_backup, "backup must restore");
    assert_eq!(out.removed_staging, 1, "staging litter swept");
    assert_eq!(
        std::fs::read(live.join("pkg").join("a.txt")).unwrap(),
        b"orig"
    );
    assert!(!staging.exists());
    // Second run is a no-op (idempotent).
    let out2 = recover_interrupted_install(&dir, &live, &dir);
    assert!(!out2.restored_backup);
    assert_eq!(out2.removed_staging, 0);
}

/// Disk-full / unwritable-destination plumbing (P0-store): write
/// failures surface as Err, never panic, never silent success — the
/// same io::Error path ENOSPC takes.
/// (Lỗi ghi trả Err, không panic, không thành công giả — cùng đường
/// io::Error mà ENOSPC đi.)
#[test]
fn lock_restore_write_failure_is_an_error_not_panic() {
    use crate::install::restore_prior_lock_result;
    let dir = tmp("nowrite");
    // A regular FILE as project root: join() yields file/mgc.lock and
    // every write fails deterministically on all platforms.
    // (File thường làm root: mọi write fail tất định mọi nền tảng.)
    let file_as_root = dir.join("not-a-dir");
    std::fs::write(&file_as_root, b"x").unwrap();
    assert!(
        restore_prior_lock_result(&file_as_root, &Some(b"v0".to_vec())).is_err(),
        "unwritable lock destination must Err"
    );
}

/// REVIEW: an EMPTY live dir carries no state — take() must skip it,
/// or a later rollback would restore emptiness over a fresh tree.
/// Dir trống không backup (rollback sau đó sẽ xóa cây mới).
#[test]
fn tree_backup_skips_empty_live_dir() {
    use crate::install::TreeBackup;
    let dir = tmp("guard-empty");
    let live = dir.join("node_modules");
    std::fs::create_dir_all(&live).unwrap();
    let guard = TreeBackup::take(&live).unwrap();
    // No backup taken: Drop must be a silent no-op, live untouched.
    std::fs::write(live.join("new.txt"), b"n").unwrap();
    drop(guard);
    assert_eq!(std::fs::read(live.join("new.txt")).unwrap(), b"n");
}
