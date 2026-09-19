#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests — Godot lanes fail closed (no package manager, no
//! automated editor download); a silent Ok would fake capability.

use super::*;

use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[tokio::test]
async fn test_install_godot_fails_closed() {
    let tmp = tmp();
    let err = install_dependencies(tmp.path()).await.unwrap_err();
    assert!(err.to_string().contains("no dependency install step"));
}

#[tokio::test]
async fn test_download_godot_binary_not_automated() {
    let tmp = tmp();
    let err = download_godot_binary("4.3.0", tmp.path())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not automated"));
}
