#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[tokio::test]
async fn test_install_bevy() {
    let tmp = tmp();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname=\"game\"\nversion=\"0.1.0\"\n",
    )
    .unwrap();

    let (packages, _, verified) = install_dependencies(tmp.path()).await.unwrap();
    assert!(verified);
    assert!(!packages.is_empty());
}

#[tokio::test]
async fn test_install_no_cargo_toml() {
    let tmp = tmp();
    let result = install_dependencies(tmp.path()).await;
    assert!(result.is_err());
}
