//! mgc-platform/fs_semaphore.rs — OS-aware filesystem write concurrency control.
//!
//! Inspired by Deno's `MAX_CONCURRENT_FS_WRITES` (libs/npm_cache/tarball.rs:33-37).
//! Filesystem operations (open/write/close per file) dominate extraction time,
//! and on macOS APFS has an internal global mutex that causes heavy lock contention
//! during highly parallel filesystem write bursts.
//!
//! By limiting concurrent disk I/O to 4 on macOS while allowing 128 on Linux/Windows,
//! we decouple CPU-bound decompression from disk I/O and prevent kernel lock stalls.

use std::sync::Arc;
use tokio::sync::Semaphore;

/// Maximum number of concurrent filesystem write operations during extraction.
/// macOS (APFS): 4 concurrent writes to prevent mutex contention.
/// Linux (ext4/btrfs/xfs) & Windows (NTFS): 128 concurrent writes.
#[cfg(target_os = "macos")]
pub const MAX_CONCURRENT_FS_WRITES: usize = 4;

#[cfg(not(target_os = "macos"))]
pub const MAX_CONCURRENT_FS_WRITES: usize = 128;

/// Returns a shared static semaphore tuned for the current operating system's filesystem.
pub fn global_fs_write_semaphore() -> Arc<Semaphore> {
    static SEMAPHORE: std::sync::OnceLock<Arc<Semaphore>> = std::sync::OnceLock::new();
    SEMAPHORE
        .get_or_init(|| {
            let limit = std::env::var("MAGICORE_MAX_CONCURRENT_FS_WRITES")
                .ok()
                .and_then(|v| v.trim().parse::<usize>().ok())
                .filter(|&v| v > 0)
                .unwrap_or(MAX_CONCURRENT_FS_WRITES);
            Arc::new(Semaphore::new(limit))
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fs_write_semaphore_limit() {
        let sem = global_fs_write_semaphore();
        #[cfg(target_os = "macos")]
        assert_eq!(sem.available_permits(), 4);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(sem.available_permits(), 128);
    }
}
