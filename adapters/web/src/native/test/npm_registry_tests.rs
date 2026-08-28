#![cfg(test)]
#![allow(clippy::unwrap_used)]

// NPM registry tests for core-web — kept beside the native module test tree.
// Test registry NPM của core-web — tách khỏi thân file production để dễ mở rộng.
use super::*;
use std::io::ErrorKind;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn bind_test_listener() -> Option<TcpListener> {
    match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => Some(listener),
        Err(err) if err.kind() == ErrorKind::PermissionDenied => {
            eprintln!("skipping socket-backed test in sandbox: {err}");
            None
        }
        Err(err) => panic!("failed to bind socket-backed test listener: {err}"),
    }
}

#[tokio::test]
async fn test_fetch_metadata_retries_after_transient_failure() {
    let hits = Arc::new(AtomicUsize::new(0));
    let Some(listener) = bind_test_listener().await else {
        return;
    };
    let addr = listener.local_addr().unwrap();
    let hits_for_server = hits.clone();

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let hit = hits_for_server.fetch_add(1, Ordering::SeqCst);
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
            if hit == 0 || hit == 1 {
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 3\r\n\r\nbad",
                    )
                    .await;
            } else {
                let body = r#"{"name":"react","description":null,"versions":{"18.2.0":{"version":"18.2.0","dependencies":null,"optionalDependencies":null,"os":null,"cpu":null,"dist":{"tarball":"http://example.test/react.tgz","integrity":null}}},"dist-tags":{"latest":"18.2.0"}}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        }
    });

    let registry = NpmRegistry::new(&format!("http://{}", addr));
    let metadata = registry.fetch_metadata("react").await.unwrap();
    assert_eq!(metadata.name, "react");
    assert!(hits.load(Ordering::SeqCst) >= 3);
}

#[test]
fn test_check_publish_age_blocks_new_package() {
    let mut meta = PackageMetadata {
        name: "evil-pkg".to_string(),
        description: None,
        versions: Default::default(),
        dist_tags: Default::default(),
        time: Default::default(),
    };
    let published_at = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    meta.time.insert("1.0.0".to_string(), published_at);

    let result = check_publish_age(&meta, "1.0.0", 86400);
    assert!(result.is_err(), "should block packages published < 24h ago");
    assert!(result.unwrap_err().contains("quarantine"));

    let result = check_publish_age(&meta, "1.0.0", 1800);
    assert!(result.is_ok());
}

#[test]
fn test_check_publish_age_allows_old_package() {
    let mut meta = PackageMetadata {
        name: "safe-pkg".to_string(),
        description: None,
        versions: Default::default(),
        dist_tags: Default::default(),
        time: Default::default(),
    };
    let published_at = (chrono::Utc::now() - chrono::Duration::hours(48)).to_rfc3339();
    meta.time.insert("2.0.0".to_string(), published_at);

    let result = check_publish_age(&meta, "2.0.0", 86400);
    assert!(result.is_ok());
}

#[test]
fn test_base64_encode_hello() {
    assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
}
