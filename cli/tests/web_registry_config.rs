#[path = "../src/commands/web_registry_config.rs"]
#[allow(dead_code)]
mod web_registry_config;

#[test]
fn builds_search_endpoint_from_default_base() {
    let url = web_registry_config::search_endpoint(
        web_registry_config::DEFAULT_WEB_REGISTRY_URL,
        "react vite",
        20,
        40,
    )
    .unwrap();
    assert_eq!(
        url,
        "https://registry.npmjs.org/-/v1/search?text=react+vite&size=20&from=40"
    );
}

#[test]
fn preserves_registry_path_prefix() {
    let url =
        web_registry_config::advisory_bulk_endpoint("https://registry.example.com/npm").unwrap();
    assert_eq!(
        url,
        "https://registry.example.com/npm/-/npm/v1/security/advisories/bulk"
    );
}

#[test]
fn rejects_plain_http_remote_registry() {
    let err = web_registry_config::join_registry_path("http://registry.example.com", "-/v1/search")
        .unwrap_err();
    assert!(err.to_string().contains("must use HTTPS"));
}

#[test]
fn allows_plain_http_loopback_registry() {
    let url =
        web_registry_config::join_registry_path("http://127.0.0.1:4315", "-/v1/search").unwrap();
    assert_eq!(url.to_string(), "http://127.0.0.1:4315/-/v1/search");
}
