#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for network transparency command

use super::*;

#[test]
fn outbound_connections_stable_and_unique() {
    let all = outbound_connections();
    assert!(all.len() >= 4, "must have at least 4 default hosts");
    let mut hosts: Vec<&str> = all.iter().map(|c| c.host.as_str()).collect();
    hosts.sort();
    hosts.dedup();
    assert_eq!(hosts.len(), all.len(), "hosts must be unique");
    for c in &all {
        assert!(c.port == 443 || c.port == 80, "port must be 443/80");
        assert!(!c.purpose.is_empty(), "must have a purpose");
    }
}

#[test]
fn url_host_parses_registry_urls() {
    assert_eq!(
        url_host("https://registry.npmjs.org").unwrap(),
        ("registry.npmjs.org".into(), 443)
    );
    assert_eq!(
        url_host("https://npm.local:8443/").unwrap(),
        ("npm.local".into(), 8443)
    );
    assert_eq!(
        url_host("http://mirror:8080/x").unwrap(),
        ("mirror".into(), 8080)
    );
    assert!(url_host("not a url").is_none());
}

#[cfg(unix)]
#[test]
fn configured_registries_ignore_external_mgc_toml_symlink() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    std::fs::write(
        external.path().join("mgc.toml"),
        "[registry.private]\nurl = 'https://private.example.test'\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        external.path().join("mgc.toml"),
        root.path().join("mgc.toml"),
    )
    .unwrap();

    assert!(user_registries_from(root.path()).is_empty());
}
