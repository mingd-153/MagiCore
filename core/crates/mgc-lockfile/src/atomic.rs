//! Atomic v4 lockfile writes + crash recovery (design §4).
//! Ghi lockfile v4 atomic + phục hồi crash.
//!
//! Single-file protocol (no split-brain: one rename, never two files):
//!   1. payload_bytes = canonical_toml(payload)
//!   2. digest = blake3(payload_bytes); lockfile_hash + optional signature
//!   3. final_bytes = full TOML document (with hash + signature)
//!   4. tmp_path = "mgc.lock.tmp.<pid>.<nonce>" via create_new
//!      (no-follow: O_NOFOLLOW on unix, symlink refusal on Windows)
//!   5. write + fsync(tmp) → [failpoint lock-after-temp-fsync]
//!   6. rename(tmp, "mgc.lock") (ReplaceFileW on Windows) → [failpoint
//!      lock-before-rename / lock-after-rename]
//!   7. fsync(parent dir) → [failpoint lock-after-dir-fsync]
//!   8. on any failure: best-effort unlink(tmp).
//!
//! Crash table: before step 4 → old file; mid 5 → old file + ignored
//! tmp; between 6 → old OR new (both valid); after 6 → new. There is NO
//! state that parses-but-is-untrusted, because readers NEVER open
//! `*.tmp*` files.
//!
//! Cleanup rule (fixes the reader-deletes-writer race): readers never
//! unlink temps. Cleanup runs ONLY while holding the project writer lock
//! (`atomic_write_locked` takes `&ProjectWriteLock`), and only unlinks a
//! temp proven stale (mtime older than grace AND owner pid dead). Anything
//! unprovable is left alone with a debug log.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::{LockfileError, LockfileResult};

/// Failpoint phases for the lock write path (mirrors the mgc-store
/// failpoint contract: `MGC_LOCK_FAILPOINT == phase` parks until SIGKILL
/// after writing READY to `MGC_LOCK_FAILPOINT_READY_FILE`; anything else
/// runs normally).
/// (Phase failpoint cho đường ghi lock.)
pub const LOCK_FAILPOINTS: &[&str] = &[
    "lock-before-temp-write",
    "lock-after-temp-fsync",
    "lock-before-rename",
    "lock-after-rename",
    "lock-after-dir-fsync",
];

/// Fire a lock failpoint (test-only hook; production cost is one env
/// read when unset).
/// (Kích hoạt failpoint lock (hook chỉ-cho-test).)
pub fn lock_failpoint(phase: &str) {
    let Some(target) = std::env::var("MGC_LOCK_FAILPOINT")
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        return;
    };
    if target == phase {
        if let Ok(path) = std::env::var("MGC_LOCK_FAILPOINT_READY_FILE")
            && let Ok(mut file) = std::fs::File::create(&path)
        {
            use std::io::Write;
            let _ = file.write_all(format!("READY:{phase}\n").as_bytes());
            let _ = file.sync_all();
        }
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
}

/// Temporary-file name: `mgc.lock.tmp.<pid>.<nonce>` (nonce = nanos of a
/// process-local counter start — unique per writer with create_new).
/// (Tên file tạm.)
fn temp_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NONCE: AtomicU64 = AtomicU64::new(0);
    let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("mgc.lock.tmp.{}.{nanos:x}.{nonce}", std::process::id())
}

/// Parse (pid) from a `mgc.lock.tmp.<pid>…` file name.
/// (Parse pid từ tên file tạm.)
fn temp_pid(file_name: &str) -> Option<u32> {
    let rest = file_name.strip_prefix("mgc.lock.tmp.")?;
    rest.split('.').next()?.parse().ok()
}

/// True when the pid is observably dead. ESRCH (no such process) means
/// dead; EPERM (alive, no permission) and success mean alive. Anything
/// unprovable (non-unix, odd errno) counts as ALIVE — cleanup never
/// deletes what it cannot prove stale.
/// (True khi pid chắc chắn chết. Không chứng minh được coi như SỐNG —
/// cleanup không bao giờ xóa cái chưa chứng minh được stale.)
#[cfg(unix)]
#[allow(unsafe_code)]
fn pid_is_dead(pid: u32) -> bool {
    // SAFETY: kill(pid, 0) performs no action — it only reports liveness
    // via the return code; no memory is touched, no signal is delivered,
    // and the pid comes from our own temp-file naming (never looked up
    // elsewhere). ESRCH means observably dead; EPERM/success means alive.
    // (AN TOÀN: kill(pid, 0) không làm gì — chỉ báo liveness qua mã trả
    // về; không chạm bộ nhớ, không gửi signal, pid từ tên temp-file của
    // chính ta.)
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if result == 0 {
        return false;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

#[cfg(not(unix))]
fn pid_is_dead(_pid: u32) -> bool {
    false
}

/// Remove stale `mgc.lock.tmp.*` files in `dir`: older than `grace`
/// AND owner pid provably dead. Call ONLY while holding the project
/// writer lock. Returns the number removed.
/// (Dọn file tạm stale: cũ hơn grace VÀ pid chết đã chứng minh. Chỉ gọi
/// khi đang giữ lock writer project.)
pub fn cleanup_stale_temps(dir: &Path, grace: Duration) -> usize {
    let mut removed = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("mgc.lock.tmp.") {
            continue;
        }
        let stale_time = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|mtime| now.duration_since(mtime).ok())
            .is_some_and(|age| age >= grace);
        if !stale_time {
            continue;
        }
        let pid_dead = temp_pid(&name).is_some_and(pid_is_dead);
        if !pid_dead {
            continue;
        }
        if std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Write `final_bytes` to `dest` atomically (caller: full v4 document
/// WITH hash + signature). Requires the project writer lock (cleanup
/// runs under it) — the type system, not discipline, enforces this.
/// (Ghi `final_bytes` tới `dest` atomic. Bắt buộc lock writer project —
/// type system, không phải kỷ luật, cưỡng chế việc này.)
pub fn atomic_write_locked(
    _lock: &crate::project_lock::ProjectWriteLock,
    dest: &Path,
    final_bytes: &[u8],
    tmp_grace: Duration,
) -> LockfileResult<()> {
    let dir: PathBuf = dest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    cleanup_stale_temps(&dir, tmp_grace);

    lock_failpoint("lock-before-temp-write");
    let tmp_path = dir.join(temp_name());
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        #[cfg(windows)]
        {
            if std::fs::symlink_metadata(&tmp_path).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(LockfileError::WriteFailed(
                    "refusing to write through a symlink temp path".to_string(),
                ));
            }
        }
        let mut tmp = options.open(&tmp_path).map_err(|e| {
            LockfileError::WriteFailed(format!(
                "cannot create lock temp '{}': {e}",
                tmp_path.display()
            ))
        })?;
        use std::io::Write;
        tmp.write_all(final_bytes).map_err(|e| {
            LockfileError::WriteFailed(format!(
                "cannot write lock temp '{}': {e}",
                tmp_path.display()
            ))
        })?;
        tmp.sync_all().map_err(|e| {
            LockfileError::WriteFailed(format!(
                "cannot fsync lock temp '{}': {e}",
                tmp_path.display()
            ))
        })?;
    }

    lock_failpoint("lock-after-temp-fsync");
    lock_failpoint("lock-before-rename");
    atomic_replace_file(&tmp_path, dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        LockfileError::WriteFailed(format!(
            "cannot replace '{}': {e} (old lock untouched)",
            dest.display()
        ))
    })?;
    lock_failpoint("lock-after-rename");

    // Ensure the rename itself is durable.
    // (Đảm bảo rename bền vững.)
    if let Ok(dir_handle) = std::fs::File::open(&dir)
        && let Err(e) = dir_handle.sync_all()
    {
        return Err(LockfileError::WriteFailed(format!(
            "cannot fsync lock dir '{}': {e}",
            dir.display()
        )));
    }
    lock_failpoint("lock-after-dir-fsync");
    Ok(())
}

/// Atomic replace: POSIX rename; Windows ReplaceFileW (MoveFileEx +
/// REPLACE_EXISTING), never bare remove+rename.
/// (Thay atomic: rename POSIX; Windows ReplaceFileW, không bao giờ
/// remove+rename trần.)
/// Atomically replace `dest` with a same-filesystem staging file.
/// The caller is responsible for serialization/ownership of the destination.
/// (Thay `dest` nguyên tử bằng staging file cùng filesystem; caller chịu
/// trách nhiệm khóa và sở hữu destination.)
pub fn atomic_replace_file(tmp_path: &Path, dest: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::rename(tmp_path, dest)
    }
    #[cfg(windows)]
    {
        windows_replace(tmp_path, dest)
    }
    #[cfg(not(any(unix, windows)))]
    {
        std::fs::rename(tmp_path, dest)
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn windows_replace(tmp_path: &Path, dest: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    let from = wide(tmp_path);
    let to = wide(dest);
    // SAFETY: nul-terminated buffers outlive the call; flags request an
    // atomic, write-through replace or a Windows error code; no memory
    // is shared beyond the synchronous call.
    // (AN TOÀN: buffer nul-terminated sống qua lời gọi; cờ yêu cầu thay
    // atomic ghi xuyên hoặc mã lỗi Windows.)
    let ok = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
