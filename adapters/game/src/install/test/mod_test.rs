#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter install summary tests.
//! Kiểm tra summary install adapter game không phụ thuộc network.

use super::*;

use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[tokio::test]
async fn test_install_bevy_stub() {
    let tmp = tmp();
    // Create a Cargo project without remote deps — giữ test hermetic, không gọi crates.io.
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname=\"test\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();

    let summary = install_dependencies(GameEngine::Bevy, tmp.path())
        .await
        .unwrap();
    assert_eq!(summary.engine, GameEngine::Bevy);
}
