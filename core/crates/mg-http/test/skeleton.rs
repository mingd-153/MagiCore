#![allow(clippy::unwrap_used)]
#![cfg(test)]
use mg_http::HttpClient;

// Basic skeleton test - crate compiles and can be imported.
// Test khói để đảm bảo crate public API import được bằng tên hợp lệ.
#[test]
fn test_crate_compiles() {
    let _client = HttpClient::new().unwrap();
}
