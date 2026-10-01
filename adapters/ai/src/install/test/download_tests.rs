#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Tests cho install/download — tách khỏi src theo RULE §5.
// (Tests for install/download — split out of src per RULE §5.)

use super::*;
use tempfile::TempDir;

#[test]
fn remote_model_download_quota_is_checked_before_accepting_chunk() {
    assert_eq!(next_model_download_total(4, 6, 10).unwrap(), 10);
    assert!(
        next_model_download_total(4, 7, 10)
            .unwrap_err()
            .to_string()
            .contains("size limit")
    );
    assert!(next_model_download_total(u64::MAX, 1, u64::MAX).is_err());
}

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

/// Helper: block_on cho test đồng bộ.
// (Helper: block_on wrapper for sync tests.)
fn block_on<T>(fut: impl std::future::Future<Output = MgResult<T>>) -> T {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(fut)
        .unwrap()
}

#[test]
fn test_download_local_copies_content() {
    let tmp = tmp();
    let src = tmp.path().join("model.onnx");
    std::fs::write(&src, b"fake onnx").unwrap();

    let target = tmp.path().join("target");
    let source = ModelSource::Local(src);

    let (path, bytes) = block_on(download_model("test", &source, &target));
    assert!(path.exists());
    assert_eq!(bytes, 9);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"fake onnx".to_vec(),
        "local copy must preserve content"
    );
}

#[test]
fn test_model_source_builders() {
    let hf = ModelSource::huggingface("gpt2", "config.json");
    assert!(matches!(hf, ModelSource::HuggingFace { .. }));
    assert_eq!(hf.checksum(), None);

    let url = ModelSource::url("https://example.com/model.bin");
    assert!(matches!(url, ModelSource::Url { .. }));

    // with_checksum gắn được cho cả HF lẫn URL
    // (with_checksum attaches to both HF and Url variants)
    let hf_ck = ModelSource::huggingface("gpt2", "config.json").with_checksum("sha256:abc123");
    assert_eq!(hf_ck.checksum(), Some("sha256:abc123"));

    let url_ck = ModelSource::url("https://example.com/m.bin").with_checksum("blake3:deadbeef");
    assert_eq!(url_ck.checksum(), Some("blake3:deadbeef"));

    let local_ck =
        ModelSource::Local(std::path::PathBuf::from("/tmp/x.bin")).with_checksum("sha256:abc");
    assert_eq!(local_ck.checksum(), None, "Local không mang checksum");
}

#[test]
fn test_filename_from_url() {
    assert_eq!(filename_from_url("https://x.y/a/model.gguf"), "model.gguf");
    assert_eq!(filename_from_url("https://x.y/a/m.bin?token=1"), "m.bin");
    assert_eq!(
        filename_from_url("https://x.y/a/m.safetensors#frag"),
        "m.safetensors"
    );
    assert_eq!(
        filename_from_url("https://x.y/"),
        "model.bin",
        "rỗng → fallback mặc định"
    );
}

#[tokio::test]
async fn download_rejects_huggingface_path_traversal_before_network_or_write() {
    let tmp = tmp();
    let target = tmp.path().join("models");
    let source = ModelSource::huggingface("owner/model", "../../escaped.bin")
        .with_revision("0123456789abcdef0123456789abcdef01234567")
        .with_checksum(format!("sha256:{}", "a".repeat(64)));

    let error = download_model("model", &source, &target)
        .await
        .expect_err("remote artifact path must remain inside the model directory");
    assert!(error.to_string().contains("filename") || error.to_string().contains("path"));
    assert!(!tmp.path().join("escaped.bin").exists());
    assert!(
        !target.exists(),
        "invalid input must not create destination directories"
    );
}

#[test]
fn test_verify_file_checksum_algorithms() {
    let tmp = tmp();
    let file = tmp.path().join("m.bin");
    std::fs::write(&file, b"hello magi").unwrap();

    use sha2::Digest;
    let sha = hex::encode(sha2::Sha256::digest(b"hello magi"));
    let blake = mgc_crypto::Blake3Hasher::hash_bytes(b"hello magi").to_hex();

    verify_file_checksum(&file, &format!("sha256:{sha}")).unwrap();
    verify_file_checksum(&file, &format!("blake3:{blake}")).unwrap();
    verify_file_checksum(&file, &blake).unwrap(); // bare hex = blake3

    let err = verify_file_checksum(&file, "sha256:0000").unwrap_err();
    assert!(err.to_string().contains("checksum mismatch"));

    let err2 = verify_file_checksum(&file, "md5:abc").unwrap_err();
    assert!(err2.to_string().contains("unsupported checksum algorithm"));
}

/// Server HTTP cục bộ tối giản — trả 1 body cố định rồi đóng.
// (Minimal local HTTP server — serves one fixed body then closes.)
fn serve_once(body: &'static [u8]) -> Option<(String, std::thread::JoinHandle<()>)> {
    use std::io::{Read, Write};
    let listener = match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(err) => panic!(
            "AI download tests require localhost; refusing to report skipped tests as passing: {err}"
        ),
    };
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(body);
        }
    });
    Some((format!("http://127.0.0.1:{port}/model.bin"), handle))
}

/// Serve chunked HTTP without Content-Length to exercise streaming quota checks.
/// Phục vụ HTTP chunked không Content-Length để kiểm tra quota theo từng chunk.
fn serve_chunked_once(body: &'static [u8]) -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap_or_else(|error| panic!("AI download tests require localhost: {error}"));
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            let response = format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:X}\r\n",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(body);
            let _ = stream.write_all(b"\r\n0\r\n\r\n");
        }
    });
    (format!("http://127.0.0.1:{port}/model.bin"), handle)
}

#[tokio::test]
async fn test_url_branch_downloads_real_bytes_and_verifies_sha256() {
    let tmp = tmp();
    let target = tmp.path().join("out");
    let Some((url, server)) = serve_once(b"magic-bytes") else {
        return;
    };

    use sha2::Digest;
    let good = hex::encode(sha2::Sha256::digest(b"magic-bytes"));
    let source = ModelSource::Url {
        url,
        checksum: Some(format!("sha256:{good}")),
    };

    let (path, bytes) = download_model("m", &source, &target).await.unwrap();
    server.join().unwrap();
    assert_eq!(bytes, b"magic-bytes".len() as u64);
    assert_eq!(std::fs::read(&path).unwrap(), b"magic-bytes".to_vec());

    // Checksum đúng → file còn nguyên
    // (Correct checksum → artifact stays on disk)
    assert!(path.exists());
}

#[tokio::test]
async fn remote_download_quota_rejects_before_writing_over_limit_chunk() {
    let tmp = tmp();
    let staging = tmp.path().join("bounded.part");
    let (url, server) = serve_chunked_once(b"12345");

    let error = http_download_to_staging_with_limit(&url, &staging, 4)
        .await
        .expect_err("response larger than the configured limit must be rejected");
    server.join().unwrap();
    assert!(error.to_string().contains("size limit"), "{error}");
    assert_eq!(
        std::fs::metadata(&staging).unwrap().len(),
        0,
        "the over-limit response chunk must be rejected before it is written"
    );
}

#[tokio::test]
async fn remote_download_without_checksum_is_rejected_before_publish() {
    let tmp = tmp();
    let target = tmp.path().join("unverified-models");
    let source = ModelSource::Url {
        url: "https://example.invalid/model.bin".into(),
        checksum: None,
    };

    let error = download_model("m", &source, &target)
        .await
        .expect_err("remote artifacts without a trusted digest must fail closed");

    assert!(error.to_string().contains("checksum"), "{error}");
    assert!(
        !target.exists(),
        "a rejected remote artifact must not create or publish a target"
    );
}

#[test]
fn remote_integrity_requires_valid_digest_and_immutable_huggingface_revision() {
    let missing_revision = ModelSource::huggingface("owner/model", "model.safetensors")
        .with_checksum(format!("sha256:{}", "a".repeat(64)));
    assert!(
        validate_remote_integrity(&missing_revision)
            .unwrap_err()
            .to_string()
            .contains("revision")
    );

    let mutable_revision = ModelSource::HuggingFace {
        repo_id: "owner/model".into(),
        revision: Some("main".into()),
        filename: "model.safetensors".into(),
        checksum: Some(format!("sha256:{}", "a".repeat(64))),
    };
    assert!(
        validate_remote_integrity(&mutable_revision)
            .unwrap_err()
            .to_string()
            .contains("40-hex")
    );

    let invalid_checksum = ModelSource::Url {
        url: "https://example.invalid/model.bin".into(),
        checksum: Some("sha256:abc".into()),
    };
    assert!(
        validate_remote_integrity(&invalid_checksum)
            .unwrap_err()
            .to_string()
            .contains("64 hexadecimal")
    );

    let local = ModelSource::Local(PathBuf::from("/tmp/model.bin"));
    assert!(validate_remote_integrity(&local).is_ok());
}

#[tokio::test]
async fn test_url_branch_wrong_checksum_preserves_existing_artifact() {
    let tmp = tmp();
    let target = tmp.path().join("out");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("model.bin"), b"trusted-old-artifact").unwrap();
    let Some((url, server)) = serve_once(b"tampered-or-not") else {
        return;
    };

    let source = ModelSource::Url {
        url,
        checksum: Some("sha256:".to_string() + &"0".repeat(64)),
    };
    let err = download_model("m", &source, &target).await.unwrap_err();
    server.join().unwrap();

    assert!(err.to_string().contains("checksum mismatch"));
    assert_eq!(
        std::fs::read(target.join("model.bin")).unwrap(),
        b"trusted-old-artifact",
        "failed replacement must preserve the previous verified artifact"
    );
    assert!(
        std::fs::read_dir(&target).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("mgc-download")),
        "failed checksum must not leak staging files"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn download_rejects_symlinked_model_parent_without_touching_target() {
    use std::os::unix::fs::symlink;

    let tmp = tmp();
    let target = tmp.path().join("models");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    symlink(&outside, target.join("weights")).unwrap();
    let source = ModelSource::huggingface("owner/model", "weights/model.bin")
        .with_revision("0123456789abcdef0123456789abcdef01234567")
        .with_checksum(format!("sha256:{}", "a".repeat(64)));

    let error = download_model("model", &source, &target)
        .await
        .expect_err("symlinked artifact parent must be rejected");
    assert!(error.to_string().contains("real directory"));
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
}

#[tokio::test]
async fn huggingface_defaults_are_rejected_without_network_pins() {
    let tmp = tmp();
    let source = ModelSource::huggingface("bert-base-uncased", "config.json");
    let error = download_model("bert", &source, tmp.path())
        .await
        .expect_err("unpinned Hugging Face source must fail before network access");
    assert!(error.to_string().contains("checksum"));
    assert!(
        !tmp.path().exists() || std::fs::read_dir(tmp.path()).unwrap().next().is_none(),
        "unpinned remote source must not create a published artifact"
    );
}
