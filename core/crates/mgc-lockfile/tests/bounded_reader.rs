//! Bounded, no-follow reads for ecosystem lock inputs — kiểm tra đọc lock an toàn.

use mgc_lockfile::read_bounded_regular_file;
use std::fs;
use tempfile::tempdir;

#[test]
fn bounded_reader_accepts_regular_file_within_limit() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("packages.lock.json");
    fs::write(&path, b"{\"dependencies\":{}} ").unwrap();

    let bytes = read_bounded_regular_file(&path, 64, "package lock").unwrap();

    assert_eq!(bytes, b"{\"dependencies\":{}} ");
}

#[test]
fn bounded_reader_rejects_file_over_limit() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("oversized.lock");
    fs::write(&path, b"12345").unwrap();

    let error = read_bounded_regular_file(&path, 4, "package lock").unwrap_err();

    assert!(error.to_string().contains("too large"));
}

#[test]
fn bounded_reader_rejects_overflowing_limit() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("small.lock");
    fs::write(&path, b"x").unwrap();

    let error = read_bounded_regular_file(&path, u64::MAX, "package lock").unwrap_err();

    assert!(error.to_string().contains("limit is too large"));
}

#[cfg(unix)]
#[test]
fn bounded_reader_rejects_symlink() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("outside.lock");
    let link = dir.path().join("packages.lock.json");
    fs::write(&target, b"{}\n").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let error = read_bounded_regular_file(&link, 64, "package lock").unwrap_err();

    assert!(error.to_string().contains("regular non-symlink"));
    assert_eq!(fs::read(target).unwrap(), b"{}\n");
}

#[test]
fn bounded_reader_rejects_directory() {
    let dir = tempdir().unwrap();

    let error = read_bounded_regular_file(dir.path(), 64, "package lock").unwrap_err();

    assert!(error.to_string().contains("regular non-symlink"));
}
