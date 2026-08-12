//! HTTP security config tests — ensure timeout/TLS config is actually wired.
//! Kiểm chứng cấu hình bảo mật không bị giữ trong struct rồi bỏ qua.

use mg_http::{timeout::TimeoutConfig, HttpClient, TlsConfig};

#[test]
fn http_client_applies_tls_config_errors() {
    let tls = TlsConfig {
        ca_bundle: Some("/definitely/missing/megagate-ca.pem".to_string()),
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
