// Registry configuration for core-web — validates effective registry endpoints.
// Cấu hình registry của core-web — gom policy endpoint khỏi adapter chính.
use crate::audit::allow_insecure_loopback_url;

pub const DEFAULT_NPM_REGISTRY: &str = "https://registry.npmjs.org";

pub fn effective_registry_url(default: &str) -> String {
    let url = std::env::var("MAGICORE_WEB_REGISTRY_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string());
    if !url.starts_with("https://") && !allow_insecure_loopback_url(&url) {
        panic!(
            "registry URL must use HTTPS: '{url}' (loopback http://127.0.0.1/localhost is allowed)"
        );
    }
    validate_registry_allowed(&url);
    url
}

pub fn validate_registry_allowed(url: &str) {
    let Some(allowed) = std::env::var("MAGICORE_WEB_ALLOWED_REGISTRIES").ok() else {
        return;
    };
    let allowed_list: Vec<&str> = allowed
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if allowed_list.is_empty() {
        return;
    }
    let normalized = url.trim_end_matches('/');
    let matched = allowed_list
        .iter()
        .any(|a| normalized == a.trim_end_matches('/'));
    if matched {
        return;
    }
    panic!(
        "registry '{}' is not in MAGICORE_WEB_ALLOWED_REGISTRIES ({})",
        url, allowed
    );
}
