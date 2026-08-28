#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Tests cho install/download — tách khỏi src theo RULE §5.
// (Tests for install/download — split out of src per RULE §5.)

use super::*;
use tempfile::TempDir;

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
fn serve_once(body: &'static [u8]) -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
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
    (format!("http://127.0.0.1:{port}/model.bin"), handle)
}

#[tokio::test]
async fn test_url_branch_downloads_real_bytes_and_verifies_sha256() {
    let tmp = tmp();
    let target = tmp.path().join("out");
    let (url, server) = serve_once(b"magic-bytes");

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
async fn test_url_branch_wrong_checksum_removes_artifact() {
    let tmp = tmp();
    let target = tmp.path().join("out");
    let (url, server) = serve_once(b"tampered-or-not");

    let source = ModelSource::Url {
        url,
        checksum: Some("sha256:".to_string() + &"0".repeat(64)),
    };
    let err = download_model("m", &source, &target).await.unwrap_err();
    server.join().unwrap();

    assert!(err.to_string().contains("checksum mismatch"));
    // Fail-closed: file tải về bị xoá, không để lại artifact lạ
    // (Fail-closed: downloaded artifact is deleted, nothing suspicious left behind)
    let leftovers: Vec<_> = std::fs::read_dir(&target).unwrap().collect();
    assert!(
        leftovers.is_empty(),
        "no artifact may survive a failed checksum"
    );
}

// Network test — CHẠY THẬT internet, chỉ bật chủ động khi cần (hermetic CI không chạy).
// (Network test hits the real internet — run manually only; hermetic CI skips it.)
#[tokio::test]
#[ignore = "hits huggingface.co and downloads a real remote artifact — run manually"]
async fn test_download_huggingface_live() {
    let tmp = tmp();
    let source = ModelSource::huggingface("bert-base-uncased", "config.json");
    let (path, bytes) = download_model("bert", &source, tmp.path()).await.unwrap();
    assert!(path.exists());
    assert!(bytes > 0, "live download must return real byte count");
}
