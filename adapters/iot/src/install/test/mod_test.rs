#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[tokio::test]
async fn test_install_esp32_rust() {
    let tmp = tmp();
    std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nname=\"test\"\n").unwrap();
    let deps = install_dependencies(IotFramework::Esp32Rust, tmp.path())
        .await
        .unwrap();
    assert!(!deps.is_empty());
}
