//! mgc-platform/reflink.rs — copy-on-write file clones (reflink/clonefile)
//! (Clone copy-on-write: macOS APFS clonefile, Linux FICLONE, fallback gọi giảm dần)
//!
//! Chain: reflink → hardlink → copy (caller decides). This module only does
//! the reflink step and reports when the platform/fs cannot clone.

use std::io;
use std::path::Path;

/// Clone `source` to `target` as a copy-on-write reflink.
///
/// Returns:
/// - `Ok(())` — clone created (target must not pre-exist).
/// - `Err(NotSupported)` — platform/fs cannot reflink (caller falls back).
/// - `Err(other)` — real failure that should abort the operation.
#[cfg(target_os = "macos")]
pub fn reflink_clone(source: &Path, target: &Path) -> Result<(), ReflinkError> {
    use std::ffi::CString;
    let src = CString::new(source.as_os_str().as_encoded_bytes()).map_err(|_| {
        ReflinkError::Other(io::Error::new(
            io::ErrorKind::InvalidInput,
            "nul in source path",
        ))
    })?;
    let dst = CString::new(target.as_os_str().as_encoded_bytes()).map_err(|_| {
        ReflinkError::Other(io::Error::new(
            io::ErrorKind::InvalidInput,
            "nul in target path",
        ))
    })?;
    // clonefile(src, dst, 0): fails EXDEV across volumes, ENOTSUP on unsupported fs.
    let rc = unsafe { libc::clonefile(src.as_ptr(), dst.as_ptr(), 0) };
    if rc == 0 {
        Ok(())
    } else {
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EXDEV)
            | Some(libc::ENOTSUP)
            | Some(libc::EOPNOTSUPP)
            | Some(libc::ENOSYS) => Err(ReflinkError::NotSupported(err)),
            _ => Err(ReflinkError::Other(err)),
        }
    }
}

/// Clone `source` to `target` via FICLONE (reflink) on btrfs/xfs/ext4(feature).
///
/// FICLONE copies data lazily (copy-on-write); target may exist but must be
/// empty/truncated — caller should remove an existing target first.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn reflink_clone(source: &Path, target: &Path) -> Result<(), ReflinkError> {
    use std::os::unix::ffi::OsStrExt;
    let src = std::ffi::CString::new(source.as_os_str().as_bytes()).map_err(|_| {
        ReflinkError::Other(io::Error::new(
            io::ErrorKind::InvalidInput,
            "nul in source path",
        ))
    })?;
    let dst = std::ffi::CString::new(target.as_os_str().as_bytes()).map_err(|_| {
        ReflinkError::Other(io::Error::new(
            io::ErrorKind::InvalidInput,
            "nul in target path",
        ))
    })?;
    let src_fd = unsafe { libc::open(src.as_ptr(), libc::O_RDONLY) };
    if src_fd < 0 {
        return Err(ReflinkError::Other(io::Error::last_os_error()));
    }
    let dst_fd = unsafe {
        libc::open(
            dst.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o644,
        )
    };
    if dst_fd < 0 {
        let err = io::Error::last_os_error();
        unsafe { libc::close(src_fd) };
        return Err(ReflinkError::Other(err));
    }
    let rc = unsafe { libc::ioctl(dst_fd, libc::FICLONE as _, src_fd) };
    unsafe { libc::close(src_fd) };
    unsafe { libc::close(dst_fd) };
    if rc == 0 {
        Ok(())
    } else {
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(code)
                if code == libc::EXDEV
                    || code == libc::EOPNOTSUPP
                    || code == libc::ENOTSUP
                    || code == libc::EINVAL
                    || code == libc::ENOSYS =>
            {
                Err(ReflinkError::NotSupported(err))
            }
            _ => Err(ReflinkError::Other(err)),
        }
    }
}

#[cfg(not(unix))]
pub fn reflink_clone(_source: &Path, _target: &Path) -> Result<(), ReflinkError> {
    Err(ReflinkError::NotSupported(io::Error::new(
        io::ErrorKind::Unsupported,
        "reflink not supported on this platform",
    )))
}

/// Why a reflink clone could not be created.
#[derive(Debug)]
pub enum ReflinkError {
    /// Platform/fs cannot reflink — caller must fall back (hardlink/copy).
    NotSupported(io::Error),
    /// Real failure — abort the operation, do not fall back.
    Other(io::Error),
}

impl std::fmt::Display for ReflinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReflinkError::NotSupported(err) => {
                write!(f, "reflink not supported: {err}")
            }
            ReflinkError::Other(err) => write!(f, "reflink failed: {err}"),
        }
    }
}

impl std::error::Error for ReflinkError {}
