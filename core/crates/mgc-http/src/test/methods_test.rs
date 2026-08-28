#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for HTTP client methods

use super::*;

#[test]
fn http_client_default_works() {
    let client = HttpClient::new().unwrap();
    assert!(client.retry.delay(0).as_secs() >= 1);
}
