#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[tokio::test]
async fn test_install_unreal_fails_closed() {
    // Unreal dependency install is NOT implemented — must fail closed with
    // a typed error, never report an empty "verified" success.
    // Install dependency Unreal CHƯA implement — phải fail-closed với
    // error typed, không bao giờ trả success "verified" rỗng.
    let tmp = tmp();
    let err = install_dependencies(tmp.path()).await.unwrap_err();
    assert!(
        matches!(err, mgc_types::MgError::Unsupported { core, capability, .. }
            if core == "game" && capability.contains("unreal")),
        "expected typed unsupported for unreal install, got: {err}"
    );
}

#[tokio::test]
async fn test_download_unreal_binary_fails_closed() {
    // No fake `.stub` file may be written — the download is not implemented
    // and must fail closed without touching the filesystem.
    // Không được ghi file `.stub` giả — download chưa implement, phải
    // fail-closed và không chạm filesystem.
    let tmp = tmp();
    let err = download_unreal_binary("5.4.0", tmp.path())
        .await
        .unwrap_err();
    assert!(
        matches!(err, mgc_types::MgError::Unsupported { core, .. } if core == "game"),
        "expected typed unsupported, got: {err}"
    );
    let entries: Vec<String> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        entries.is_empty(),
        "no fake engine file may be created, found: {entries:?}"
    );
}
