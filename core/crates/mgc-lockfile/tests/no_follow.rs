#![allow(clippy::unwrap_used)]

use mgc_lockfile::{load_lockfile, read_lockfile_checked};
use tempfile::tempdir;

#[cfg(unix)]
#[test]
fn lockfile_readers_refuse_symlinks_instead_of_following_them() {
    let project = tempdir().unwrap();
    let outside = project.path().join("outside.lock");
    std::fs::write(&outside, "not a lockfile").unwrap();
    let linked = project.path().join("mgc.lock");
    std::os::unix::fs::symlink(&outside, &linked).unwrap();

    let checked = read_lockfile_checked(project.path()).unwrap_err();
    assert!(checked.to_string().contains("regular non-symlink file"));

    let loaded = load_lockfile(&linked).unwrap_err();
    assert!(loaded.to_string().contains("regular non-symlink file"));
}

#[test]
fn checked_lock_reader_reports_absence_without_following_paths() {
    let project = tempdir().unwrap();
    assert!(read_lockfile_checked(project.path()).unwrap().is_none());
}

/// Windows publication must work beyond the legacy MAX_PATH limit.
/// Publish Windows phải chạy được khi path dài hơn MAX_PATH cũ.
#[cfg(windows)]
#[test]
fn atomic_replace_handles_long_windows_paths() {
    let project = tempdir().unwrap();
    let mut directory = std::fs::canonicalize(project.path()).unwrap();
    for _ in 0..8 {
        directory = directory.join("package-store-long-component-1234567890");
    }
    std::fs::create_dir_all(&directory).unwrap();
    let staging = directory.join("staging.tmp");
    let destination = directory.join("package-marker.json");
    std::fs::write(&staging, b"first").unwrap();
    mgc_lockfile::atomic::atomic_replace_file(&staging, &destination).unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), b"first");
    std::fs::write(&staging, b"second").unwrap();
    mgc_lockfile::atomic::atomic_replace_file(&staging, &destination).unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), b"second");
    assert!(!staging.exists());
}
