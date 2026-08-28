#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for chunked upload functionality

use super::*;

#[test]
fn uploader_creation() {
    let client = HttpClient::new().unwrap();
    let uploader = ChunkedUploader::new(client, "http://localhost:4315");
    assert_eq!(uploader.chunk_size, 10 * 1024 * 1024);
}
