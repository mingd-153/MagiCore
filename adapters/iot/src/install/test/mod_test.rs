#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[tokio::test]
async fn test_install_esp32_rust_fails_closed_until_verified() {
    // ESP32 install is not E2E-verified — must fail closed with typed
    // guidance instead of returning a fabricated installed-deps list.
    // Install ESP32 chưa E2E-verified — phải fail-closed với hướng dẫn
    // typed thay vì trả danh sách cài đặt bịa.
    let tmp = tmp();
    std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nname=\"test\"\n").unwrap();
    let err = install_dependencies(IotFramework::Esp32Rust, tmp.path())
        .await
        .unwrap_err();
    assert!(
        matches!(err, mgc_types::MgError::Unsupported { core, capability, .. }
            if core == "iot" && capability.contains("esp32-rust")),
        "expected typed unsupported, got: {err}"
    );
}
