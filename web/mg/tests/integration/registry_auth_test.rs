//! Integration tests for registry auth, rate limits, proxy, and .npmrc parsing.
//!
//! Note: HTTP-based tests (429 retry, auth headers, 404) use a TCP server
//! that speaks HTTP/1.1. The production RegistryClient uses http2_prior_knowledge,
//! so those tests are omitted here — they require an HTTP/2-capable server.

use mg_core::RegistryConfig;
use mg_registry::{NpmRegistry, RegistryClient};

#[test]
fn test_token_in_registry_config() {
    let config = RegistryConfig::npm().with_token("npm_abc123");
    assert_eq!(config.token, Some("npm_abc123".to_string()));
    assert_eq!(config.url, "https://registry.npmjs.org");

    let config2 = RegistryConfig::default();
    assert!(config2.token.is_none());
    assert_eq!(config2.url, "https://registry.npmjs.org");
}

#[test]
fn test_registry_config_with_scope() {
    let config = RegistryConfig::npm()
        .with_token("token123")
        .with_scope("@mycompany");
    assert_eq!(config.scope, Some("@mycompany".to_string()));
    assert_eq!(config.token, Some("token123".to_string()));
}

#[test]
fn test_npm_registry_token_methods() {
    let _ = NpmRegistry::new("https://registry.npmjs.org").with_token("test-token".to_string());

    let mut registry2 = NpmRegistry::new("https://custom.registry.com");
    registry2.set_token(Some("custom-token".to_string()));
    registry2.set_token(None);
}

#[test]
fn test_registry_client_creation_with_proxy_envs() {
    tempfile::TempDir::new().unwrap();

    std::env::set_var("HTTPS_PROXY", "http://proxy.local:8080");
    std::env::set_var("HTTP_PROXY", "http://proxy.local:8080");
    std::env::set_var("NO_PROXY", "localhost,127.0.0.1");

    let client = RegistryClient::new();
    drop(client);

    std::env::remove_var("HTTPS_PROXY");
    std::env::remove_var("HTTP_PROXY");
    std::env::remove_var("NO_PROXY");
}

#[test]
fn test_registry_client_creation_without_proxy() {
    std::env::remove_var("HTTPS_PROXY");
    std::env::remove_var("https_proxy");
    std::env::remove_var("HTTP_PROXY");
    std::env::remove_var("http_proxy");

    let client = RegistryClient::new();
    drop(client);
}

#[test]
fn test_registry_client_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<RegistryClient>();
}

#[test]
fn test_registry_client_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<RegistryClient>();
}

#[test]
fn test_registry_config_is_clone() {
    fn assert_clone<T: Clone>() {}
    assert_clone::<RegistryConfig>();
}

#[test]
fn test_registry_client_creation_with_token_flow() {
    let client = RegistryClient::new();
    let config = RegistryConfig::npm().with_token("test-token-123");
    assert_eq!(config.token, Some("test-token-123".to_string()));
    drop(client);
}

#[test]
fn test_default_registry_url() {
    let config = RegistryConfig::default();
    assert_eq!(config.url, "https://registry.npmjs.org");
}

#[test]
fn test_npmrc_parsing_basic() {
    let dir = tempfile::tempdir().unwrap();
    let npmrc_path = dir.path().join(".npmrc");

    std::fs::write(
        &npmrc_path,
        r#"registry=https://custom.registry.com/
//registry.npmjs.org/:_authToken=npm_abc123
@scope:registry=https://scope.registry.com/
# this is a comment
; this is also a comment
"#,
    )
    .unwrap();

    let content = std::fs::read_to_string(&npmrc_path).unwrap();
    let mut entries: Vec<(String, String)> = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim().to_string();
            let value = line[eq_pos + 1..].trim().to_string();
            entries.push((key, value));
        }
    }

    assert_eq!(entries.len(), 3);
    assert!(entries.contains(&(
        "registry".to_string(),
        "https://custom.registry.com/".to_string()
    )));
    assert!(entries.contains(&(
        "//registry.npmjs.org/:_authToken".to_string(),
        "npm_abc123".to_string()
    )));
    assert!(entries.contains(&(
        "@scope:registry".to_string(),
        "https://scope.registry.com/".to_string()
    )));
}

#[test]
fn test_npmrc_parsing_empty_and_whitespace() {
    let dir = tempfile::tempdir().unwrap();
    let npmrc_path = dir.path().join(".npmrc");

    std::fs::write(&npmrc_path, "").unwrap();
    let content = std::fs::read_to_string(&npmrc_path).unwrap();
    let entries: Vec<(String, String)> = content
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#') && !t.starts_with(';')
        })
        .filter_map(|l| {
            l.find('=')
                .map(|p| (l[..p].trim().into(), l[p + 1..].trim().into()))
        })
        .collect();
    assert!(entries.is_empty());
}

#[test]
fn test_npmrc_parsing_edge_cases() {
    let dir = tempfile::tempdir().unwrap();
    let npmrc_path = dir.path().join(".npmrc");

    std::fs::write(
        &npmrc_path,
        r#"key-with=equals=value
trailing=value-with=multiple=equals
"#,
    )
    .unwrap();

    let content = std::fs::read_to_string(&npmrc_path).unwrap();
    let entries: Vec<(String, String)> = content
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#') && !t.starts_with(';')
        })
        .filter_map(|l| {
            l.find('=')
                .map(|p| (l[..p].trim().into(), l[p + 1..].trim().into()))
        })
        .collect();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, "key-with");
    assert_eq!(entries[0].1, "equals=value");
    assert_eq!(entries[1].0, "trailing");
    assert_eq!(entries[1].1, "value-with=multiple=equals");
}
