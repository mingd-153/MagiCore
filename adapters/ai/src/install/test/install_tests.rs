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

#[tokio::test]
async fn install_model_accepts_sha256_prefixed_remote_checksum() {
    use sha2::Digest;

    let body = b"model bytes with sha256";
    let checksum = format!("sha256:{}", hex::encode(sha2::Sha256::digest(body)));
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/model.bin")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;
    let source = ModelSource::Url {
        url: format!("{}/model.bin", server.url()),
        checksum: Some(checksum),
    };

    let tmp = tmp();
    let summary = install_model("test-model", source, &tmp.path().join("models"))
        .await
        .expect("valid SHA-256 artifact should install");

    mock.assert_async().await;
    assert!(summary.verified);
    assert_eq!(std::fs::read(summary.local_path).unwrap(), body);
}
