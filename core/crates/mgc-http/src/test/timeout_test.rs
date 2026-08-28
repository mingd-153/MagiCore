#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for HTTP timeout configuration

use super::*;

#[test]
fn timeout_config_defaults() {
    let cfg = TimeoutConfig::default();
    assert_eq!(cfg.connect.as_secs(), 10);
    assert_eq!(cfg.request.as_secs(), 30);
    assert_eq!(cfg.upload_chunk.as_secs(), 60);
}
