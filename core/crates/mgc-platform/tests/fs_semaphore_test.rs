#![cfg(test)]
#![allow(clippy::unwrap_used)]

// Auto-migrated from core/crates/mgc-platform/src/fs_semaphore.rs
use mgc_platform::*;

#[test]
fn test_fs_write_semaphore_limit() {
    let sem = global_fs_write_semaphore();
    #[cfg(target_os = "macos")]
    assert_eq!(sem.available_permits(), 4);
    #[cfg(not(target_os = "macos"))]
    assert_eq!(sem.available_permits(), 128);
}
