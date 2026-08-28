#![allow(clippy::unwrap_used)]
#![cfg(test)]
use mgc_http::TlsConfig;
use mgc_oci::client::OciClient;

#[test]
fn crate_compiles() {
    let tls = TlsConfig::default();
    assert!(tls.ca_bundle.is_none());
}

#[test]
fn client_new_creates_instance() {
    let _client = OciClient::new("http://localhost:4315", None).unwrap();
}

#[test]
fn client_new_applies_tls_config_errors() {
    let tls = TlsConfig {
        ca_bundle: Some("/definitely/missing/magicore-ca.pem".to_string()),
        ..TlsConfig::default()
    };

    let err = match OciClient::new("https://registry.example.test", Some(tls)) {
        Ok(_) => panic!("expected missing CA bundle to fail"),
        Err(err) => err,
    };
    assert!(
        err.to_string().contains("read CA"),
        "unexpected error: {err}"
    );
}
