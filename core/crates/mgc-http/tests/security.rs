#![allow(clippy::unwrap_used)]
//! HTTP security config tests — ensure timeout/TLS config is actually wired.
//! Kiểm chứng cấu hình bảo mật không bị giữ trong struct rồi bỏ qua.

use mgc_http::{HttpClient, TlsConfig, timeout::TimeoutConfig};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread::JoinHandle,
    time::Duration,
};

fn local_response(response: &'static str) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        stream.write_all(response.as_bytes()).unwrap();
    });
    (format!("http://{address}/"), server)
}

#[tokio::test]
async fn bounded_identity_client_does_not_follow_redirects() {
    let (url, server) = local_response(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/redirect-target\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let timeout = TimeoutConfig {
        connect: Duration::from_secs(1),
        request: Duration::from_secs(1),
        ..TimeoutConfig::default()
    };
    let client = HttpClient::with_security_no_redirects(&timeout, &TlsConfig::default()).unwrap();

    let (status, body) = client.get_bytes_limited(&url, 128).await.unwrap();
    server.join().unwrap();

    assert_eq!(status, 302);
    assert!(body.is_empty());
}

#[tokio::test]
async fn bounded_identity_client_rejects_oversized_responses() {
    let (url, server) =
        local_response("HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n12345");
    let timeout = TimeoutConfig {
        connect: Duration::from_secs(1),
        request: Duration::from_secs(1),
        ..TimeoutConfig::default()
    };
    let client = HttpClient::with_security_no_redirects(&timeout, &TlsConfig::default()).unwrap();

    let error = client.get_bytes_limited(&url, 4).await.unwrap_err();
    server.join().unwrap();

    assert!(error.to_string().contains("response exceeds byte limit"));
}

#[test]
fn http_client_applies_tls_config_errors() {
    let tls = TlsConfig {
        ca_bundle: Some("/definitely/missing/magicore-ca.pem".to_string()),
        ..TlsConfig::default()
    };

    let err = match HttpClient::with_security(&TimeoutConfig::default(), &tls) {
        Ok(_) => panic!("expected missing CA bundle to fail"),
        Err(err) => err,
    };
    assert!(
        err.to_string().contains("read CA"),
        "unexpected error: {err}"
    );
}
