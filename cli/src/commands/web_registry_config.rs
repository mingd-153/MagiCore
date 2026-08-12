// Web registry configuration — centralizes npm-compatible registry URLs.
// Cấu hình registry web — không rải URL registry trong từng command.
use anyhow::{Context, Result};
use url::Url;

pub const DEFAULT_WEB_REGISTRY_URL: &str = "https://registry.npmjs.org";
pub const WEB_REGISTRY_URL_ENV: &str = "MEGAGATE_WEB_REGISTRY_URL";

pub fn web_registry_url() -> String {
    std::env::var(WEB_REGISTRY_URL_ENV).unwrap_or_else(|_| DEFAULT_WEB_REGISTRY_URL.to_string())
}

pub fn search_endpoint(base: &str, query: &str, size: u32, from: u32) -> Result<String> {
    let mut url = join_registry_path(base, "-/v1/search")?;
    url.query_pairs_mut()
        .append_pair("text", query)
        .append_pair("size", &size.to_string())
        .append_pair("from", &from.to_string());
    Ok(url.to_string())
}

pub fn advisory_bulk_endpoint(base: &str) -> Result<String> {
    Ok(join_registry_path(base, "-/npm/v1/security/advisories/bulk")?.to_string())
}

pub fn join_registry_path(base: &str, path: &str) -> Result<Url> {
    let mut url = Url::parse(base).with_context(|| format!("invalid registry URL: {base}"))?;
    if url.scheme() != "https" && !is_loopback_host(url.host_str()) {
        anyhow::bail!("registry URL must use HTTPS unless it targets localhost");
    }
    let base_path = url.path().trim_end_matches('/');
    let path = path.trim_start_matches('/');
    let joined = if base_path.is_empty() || base_path == "/" {
        format!("/{path}")
    } else {
        format!("{base_path}/{path}")
    };
    url.set_path(&joined);
    Ok(url)
}

fn is_loopback_host(host: Option<&str>) -> bool {
    matches!(host, Some("localhost" | "127.0.0.1" | "::1"))
}
