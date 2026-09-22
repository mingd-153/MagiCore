//! Project-level writer lock — OS-backed exclusive lock, RAII (design §4.6).
//! Lock writer cấp project — lock độc quyền của OS, RAII.
//!
//! Authority is the KERNEL, not a TTL file: a live holder is never
//! robbed, a dead holder (even SIGKILL) releases automatically, and a
//! loser gets `LockBusy` — never last-writer-wins. `pid` metadata is
//! diagnostic only and never decides ownership.
//! (Quyền lực là KERNEL, không phải file TTL: holder sống không bao giờ
//! bị cướp, holder chết (kể cả SIGKILL) tự nhả, kẻ thua nhận `LockBusy` —
//! không bao giờ last-writer-wins.)

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fs2::FileExt;

use crate::{LockfileError, LockfileResult};

/// Guard file holding the project writer lock — Khóa writer project.
pub const WRITE_LOCK_FILE: &str = "lock.write";

/// RAII guard: dropping closes the file, which releases the OS lock
/// (even on unwind). Never release manually.
///
#[derive(Debug)]
pub struct ProjectWriteLock {
    _file: std::fs::File,
    guard_path: PathBuf,
}

impl ProjectWriteLock {
    /// Guard-file path for a project root (`.magicore/lock.write`).
    pub fn guard_path_for(project_root: &Path) -> PathBuf {
        project_root.join(".magicore").join(WRITE_LOCK_FILE)
    }

    /// Acquire the exclusive OS lock, waiting up to `timeout`. The
    /// CURRENT lock content is read only AFTER acquiring (observers).
    /// Timeout → `LockBusy`, never reclaim, never overwrite blindly.
    /// (Acquire lock OS độc quyền, chờ tối đa `timeout`. Hết timeout →
    /// `LockBusy`, không reclaim, không ghi đè mù.)
    pub fn acquire(project_root: &Path, timeout: Duration) -> LockfileResult<Self> {
        let guard_path = Self::guard_path_for(project_root);
        if let Some(parent) = guard_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                LockfileError::WriteFailed(format!(
                    "cannot create lock dir '{}': {e}",
                    parent.display()
                ))
            })?;
        }
        Self::refuse_link_swap(&guard_path)?;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(&guard_path).map_err(|e| {
            LockfileError::WriteFailed(format!(
                "cannot open lock guard '{}': {e}",
                guard_path.display()
            ))
        })?;
        let start = Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => {
                    return Ok(Self {
                        _file: file,
                        guard_path,
                    });
                }
                // Contention: wait out the timeout, then LockBusy (never
                // steal a live holder's lock).
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if start.elapsed() >= timeout {
                        return Err(LockfileError::LockBusy(format!(
                            "project lock held by a live process ('{}'); refusing to reclaim",
                            guard_path.display()
                        )));
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                // A real OS error (permissions, FS gone) is not
                // contention — fail immediately with the cause.
                Err(e) => {
                    return Err(LockfileError::WriteFailed(format!(
                        "cannot lock '{}': {e}",
                        guard_path.display()
                    )));
                }
            }
        }
    }

    /// Diagnostic path (never an ownership input).
    pub fn guard_path(&self) -> &Path {
        &self.guard_path
    }

    /// Refuse symlink/junction/reparse-point guard files: a swapped guard
    /// would move the lock somewhere the rival controls.
    /// (Từ chối file guard là symlink/junction/reparse-point.)
    fn refuse_link_swap(guard_path: &Path) -> LockfileResult<()> {
        let Ok(meta) = std::fs::symlink_metadata(guard_path) else {
            return Ok(());
        };
        if meta.file_type().is_symlink() {
            return Err(LockfileError::WriteFailed(format!(
                "refusing symlinked lock guard '{}'",
                guard_path.display()
            )));
        }
        #[cfg(windows)]
        {
            // Junctions and mount points surface as reparse points, not
            // symlinks, on Windows — both must be refused.
            // (Junction/mount point hiện là reparse point, không phải
            // symlink, trên Windows — từ chối cả hai.)
            if windows_reparse_point(guard_path) {
                return Err(LockfileError::WriteFailed(format!(
                    "refusing reparse-point lock guard '{}'",
                    guard_path.display()
                )));
            }
        }
        Ok(())
    }
}

/// True when the path is a symlink (all platforms) or a Windows
/// reparse point (junction/mount). Shared by journal/backup/lock writers
/// so every project writer refuses link-swapped paths the same way.
/// (Có phải symlink/reparse point không — mọi writer dùng chung.)
pub fn path_is_link_or_reparse(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        windows_reparse_point(path)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// True when the path carries FILE_ATTRIBUTE_REPARSE_POINT (junctions,
/// mount points, and non-symlink reparse data).
#[cfg(windows)]
fn windows_reparse_point(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, GetFileAttributesW, INVALID_FILE_ATTRIBUTES,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: nul-terminated buffer outlives the synchronous call; the
    // return is a plain attribute DWORD, no memory is shared.
    // (AN TOÀN: buffer nul-terminated sống qua lời gọi đồng bộ; trả về
    // DWORD attribute thường, không chia sẻ bộ nhớ.)
    #[allow(unsafe_code)]
    let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
    attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_REPARSE_POINT) != 0
}
