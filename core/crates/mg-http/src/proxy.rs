//! Proxy configuration (12 §8)
//! (Đọc HTTP_PROXY/HTTPS_PROXY/NO_PROXY + per-registry override)

use anyhow::Result;
use reqwest::Proxy;
use std::collections::HashMap;
use std::env;
use url::Url;

/// Proxy configuration từ env + per-registry override
#[derive(Debug, Clone, Default)]
pub struct ProxyConfig {
    pub http: Option<String>,
    pub https: Option<String>,
    pub no_proxy: Vec<String>,
    pub per_registry: HashMap<String, String>, // registry_url -> proxy_url
}

impl ProxyConfig {
    /// Load từ environment variables
    pub fn from_env() -> Self {
        let http = env::var("HTTP_PROXY").or_else(|_| env::var("http_proxy")).ok();
        let https = env::var("HTTPS_PROXY").or_else(|_| env::var("https_proxy")).ok();
        let no_proxy = env::var("NO_PROXY")
            .or_else(|_| env::var("no_proxy"))
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_else(|_| vec!["localhost".into(), "127.0.0.1".into()]);
        
        Self {
            http,
            https,
            no_proxy,
            per_registry: HashMap::new(),
        }
    }

    /// Thêm override cho registry cụ thể
    pub fn with_registry_override(mut self, registry_url: impl Into<String>, proxy_url: impl Into<String>) -> Self {
        self.per_registry.insert(registry_url.into(), proxy_url.into());
        self
    }

    /// Kiểm tra xem URL có bị bypass proxy không
    pub fn is_bypassed(&self, url: &str) -> bool {
        if let Ok(parsed) = Url::parse(url) {
            let host = parsed.host_str().unwrap_or("");
            for np in &self.no_proxy {
                if np == "*" || host == np || host.ends_with(&format!(".{}", np)) {
                    return true;
                }
            }
        }
        false
    }

    /// Build reqwest::Proxy cho URL cụ thể
    pub fn build_proxy(&self, url: &str) -> Result<Option<Proxy>> {
        if self.is_bypassed(url) {
            return Ok(None);
        }

        let parsed = Url::parse(url)?;
        let scheme = parsed.scheme();
        let proxy_url = if scheme == "https" {
            self.https.as_ref().or(self.http.as_ref())
        } else {
            self.http.as_ref()
        };

        // Per-registry override優先
        for (reg, proxy) in &self.per_registry {
            if url.starts_with(reg) {
                return Ok(Some(Proxy::all(proxy).map_err(|e| anyhow::anyhow!("invalid proxy url: {}", e))?));
            }
        }

        if let Some(p) = proxy_url {
            Ok(Some(Proxy::all(p).map_err(|e| anyhow::anyhow!("invalid proxy url: {}", e))?))
        } else {
            Ok(None)
        }
    }

    /// Apply vào reqwest::ClientBuilder
    pub fn apply(&self, builder: reqwest::ClientBuilder, url: &str) -> Result<reqwest::ClientBuilder> {
        if let Some(proxy) = self.build_proxy(url)? {
            Ok(builder.proxy(proxy))
        } else {
            Ok(builder)
        }
    }
}

#[cfg(test)]
mod tests {
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
        let mut cfg = ProxyConfig::default();
        cfg.no_proxy = vec!["*".into()];
        assert!(cfg.is_bypassed("http://example.com"));
    }
}