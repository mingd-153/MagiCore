#![cfg(test)]
use mg_http::TlsConfig;
use mg_oci::client::OciClient;

#[test]
fn crate_compiles() {
    assert!(true);
}

#[test]
fn client_new_creates_instance() {
    let _client = OciClient::new("http://localhost:4315", None).unwrap();
    // Just verify the client was created successfully
    assert!(true);
}

#[test]
fn client_new_applies_tls_config_errors() {
    let tls = TlsConfig {
        ca_bundle: Some("/definitely/missing/megagate-ca.pem".to_string()),
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
