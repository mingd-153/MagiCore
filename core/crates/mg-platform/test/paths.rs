//! Integration tests for mg-platform paths — test riêng tại test/ (RULE §5)
use mg_platform::paths::{GlobalPaths, ProjectPaths};
use tempfile;

#[test]
fn project_paths_computes_correctly() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = ProjectPaths::from_root(tmp.path());
    assert!(paths.patches_dir().ends_with(".megagate/patches"));
    assert!(paths
        .lock_signatures
        .ends_with(".megagate/lock-signatures.json"));
}

#[test]
fn ensure_dirs_creates_all() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = ProjectPaths::from_root(tmp.path());
    paths.ensure_dirs().unwrap();
    assert!(tmp.path().join(".megagate/patches").exists());
    // lock_signatures is a file, not dir
    assert!(tmp.path().join(".megagate").exists());
}

#[test]
fn global_paths_creates_patches_dir() {
    let paths = GlobalPaths::new().unwrap();
    assert!(paths.patches_dir().ends_with(".megagate/patches"));
}

#[test]
fn find_project_root_detects_mg_toml() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("mg.toml"), "").unwrap();
    let found = mg_platform::paths::find_project_root(tmp.path()).unwrap();
    assert_eq!(found, tmp.path());
}
