#![cfg(test)]
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
