#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::field_reassign_with_default)]
//! Tests for HTTP proxy configuration

use super::*;

#[test]
fn no_proxy_defaults() {
    let cfg = ProxyConfig::from_env();
    assert!(cfg.no_proxy.contains(&"localhost".into()));
    assert!(cfg.no_proxy.contains(&"127.0.0.1".into()));
}

#[test]
fn bypass_localhost() {
    let cfg = ProxyConfig::from_env();
    assert!(cfg.is_bypassed("http://localhost:4315"));
    assert!(cfg.is_bypassed("http://127.0.0.1:4315"));
}

#[test]
fn no_proxy_wildcard() {
    let cfg = ProxyConfig {
        no_proxy: vec!["*".into()],
        ..ProxyConfig::default()
    };
    assert!(cfg.is_bypassed("http://example.com"));
}
