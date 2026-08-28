#![cfg(test)]
#![allow(clippy::unwrap_used)]

use super::*;
use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[tokio::test]
async fn test_install_local_model() {
    let tmp = tmp();
    let src = tmp.path().join("model.bin");
    std::fs::write(&src, b"fake model").unwrap();

    let target = tmp.path().join("target");
    let source = ModelSource::Local(src.clone());

    let summary = install_model("test-model", source, &target).await.unwrap();
    assert_eq!(summary.model_id, "test-model");
    assert!(summary.local_path.exists());
}
