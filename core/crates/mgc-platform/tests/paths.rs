#![allow(clippy::unwrap_used)]
//! Integration tests for mgc-platform paths — test riêng tại test/ (RULE §5)
use mgc_platform::paths::{GlobalPaths, ProjectPaths};
use std::process::Command;

#[test]
fn project_paths_computes_correctly() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = ProjectPaths::from_root(tmp.path());
    assert!(paths.patches_dir().ends_with(".magicore/patches"));
    assert!(
        paths
            .lock_signatures
            .ends_with(".magicore/lock-signatures.json")
    );
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
fn global_paths_honors_store_root_override() {
    let temp = tempfile::tempdir().unwrap();
    let store_root = temp.path().join("isolated-store");
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "global_paths_store_root_override_child",
            "--nocapture",
        ])
        .env("MAGICORE_STORE_ROOT", &store_root)
        .env("MGC_TEST_EXPECTED_STORE_ROOT", &store_root)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "store-root override child failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn global_paths_store_root_override_child() {
    let Some(expected) = std::env::var_os("MGC_TEST_EXPECTED_STORE_ROOT") else {
        return;
    };
    assert_eq!(
        GlobalPaths::new().unwrap().store,
        std::path::PathBuf::from(expected)
    );
}

#[test]
fn find_project_root_detects_mgc_toml() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("mgc.toml"), "").unwrap();
    let found = mgc_platform::paths::find_project_root(tmp.path()).unwrap();
    assert_eq!(found, tmp.path());
}
