#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[tokio::test]
#[ignore = "runs cargo fetch against crates.io — run manually with network access"]
async fn test_install_bevy_stub() {
    let tmp = tmp();
    // Create Cargo.toml
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname=\"test\"\nversion=\"0.1.0\"\n\n[dependencies]\nbevy=\"0.14\"\n",
    )
    .unwrap();

    let summary = install_dependencies(GameEngine::Bevy, tmp.path())
        .await
        .unwrap();
    assert_eq!(summary.engine, GameEngine::Bevy);
}
