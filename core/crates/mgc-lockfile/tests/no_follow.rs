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
