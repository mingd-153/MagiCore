#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for TLS configuration

use super::*;

#[test]
fn tls_config_defaults() {
    let cfg = TlsConfig::default();
    assert!(!cfg.allow_untrusted);
    assert_eq!(cfg.min_version, TlsVersion::V1_2);
}

#[test]
fn validate_https_required() {
    assert!(validate_registry_url("https://registry.example.com", false).is_ok());
    assert!(validate_registry_url("http://localhost:4315", true).is_ok());
    assert!(validate_registry_url("http://127.0.0.1:4315", true).is_ok());
    assert!(validate_registry_url("http://remote.com", false).is_err());
}
