#![allow(clippy::unwrap_used)]
//! Integration tests for mgc-platform paths — test riêng tại test/ (RULE §5)
use mgc_platform::paths::{GlobalPaths, ProjectPaths};

#[test]
fn project_paths_computes_correctly() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = ProjectPaths::from_root(tmp.path());
    assert!(paths.patches_dir().ends_with(".magicore/patches"));
    assert!(paths
        .lock_signatures
        .ends_with(".magicore/lock-signatures.json"));
}

#[test]
fn ensure_dirs_creates_all() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = ProjectPaths::from_root(tmp.path());
    paths.ensure_dirs().unwrap();
    assert!(tmp.path().join(".magicore/patches").exists());
    // lock_signatures is a file, not dir
    assert!(tmp.path().join(".magicore").exists());
}

#[test]
fn global_paths_creates_patches_dir() {
    let paths = GlobalPaths::new().unwrap();
    assert!(paths.patches_dir().ends_with(".magicore/patches"));
}

#[test]
fn find_project_root_detects_mgc_toml() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("mgc.toml"), "").unwrap();
    let found = mgc_platform::paths::find_project_root(tmp.path()).unwrap();
    assert_eq!(found, tmp.path());
}
