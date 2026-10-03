#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[tokio::test]
async fn test_scaffold_esp32() {
    let tmp = tmp();
    scaffold_project(IotFramework::Esp32Rust, "test", "esp32c3", tmp.path())
        .await
        .unwrap();
    assert!(tmp.path().join("Cargo.toml").exists());
    assert!(tmp.path().join("mgc.toml").exists());
}

#[tokio::test]
async fn test_scaffold_platformio() {
    let tmp = tmp();
    scaffold_project(IotFramework::Platformio, "test", "esp32dev", tmp.path())
        .await
        .unwrap();
    assert!(tmp.path().join("platformio.ini").exists());
}

#[tokio::test]
async fn scaffold_rejects_unknown_board_before_creating_project_files() {
    let temp = tmp();
    let project = temp.path().join("firmware");
    let error = scaffold_project(
        IotFramework::Esp32Rust,
        "firmware",
        "unknown-board",
        &project,
    )
    .await
    .expect_err("unknown boards must be rejected");

    assert!(matches!(error, mgc_types::MgError::Unsupported { .. }));
    assert!(
        !project.exists(),
        "rejection must happen before filesystem writes"
    );
}

#[tokio::test]
async fn scaffold_rejects_board_from_another_framework() {
    let temp = tmp();
    let project = temp.path().join("firmware");
    let error = scaffold_project(
        IotFramework::Esp32Rust,
        "firmware",
        "nrf52dk_nrf52832",
        &project,
    )
    .await
    .expect_err("framework mismatch must be rejected");

    assert!(matches!(error, mgc_types::MgError::Unsupported { .. }));
    assert!(
        !project.exists(),
        "rejection must happen before filesystem writes"
    );
}
