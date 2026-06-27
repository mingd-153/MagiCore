use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use parking_lot::RwLock;
use tokio::sync::OnceCell;

use mgpm_core::RegistryConfig;

pub mod npm;
pub mod jsr;
pub mod git;
pub mod http;
pub mod file;

pub use npm::NpmRegistry;
pub use jsr::JsrRegistry;
pub use git::GitRegistry;
pub use http::HttpRegistry;
pub use file::{FileRegistry, WorkspaceRegistry, PackageJsonReader, ParsedPackageJson};

#[derive(Debug, Clone, thiserror::Error)]
pub enum RegistryError {
    #[error("HTTP error: {0}")]
    HttpError(u16),
    #[error("network error: {0}")]
    NetworkError(String),
    #[error("tarball not found")]
    TarballNotFound,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("rate limited - retry after {0}s")]
    RateLimited(u64),
}

impl From<reqwest::Error> for RegistryError {
    fn from(e: reqwest::Error) -> Self {
        Self::NetworkError(e.to_string())
    }
}

impl From<std::io::Error> for RegistryError {
    fn from(e: std::io::Error) -> Self {
        Self::NetworkError(e.to_string())
    }
}

pub struct RegistryClient {
    client: reqwest::Client,
    rate_limiter: Arc<DefaultDirectRateLimiter>,
    inflight: Arc<DashMap<String, Arc<OnceCell<Result<serde_json::Value, RegistryError>>>>>,
    registries: RwLock<HashMap<String, RegistryConfig>>,
}

impl RegistryClient {
    pub fn new() -> Self {
        let mut builder = reqwest::Client::builder()
            .pool_max_idle_per_host(64)
            .http2_prior_knowledge()
            .gzip(true)
            .brotli(true)
            .user_agent(concat!("mgpm/", env!("CARGO_PKG_VERSION")));

        if let Ok(url) = std::env::var("HTTPS_PROXY")
            .or_else(|_| std::env::var("https_proxy"))
        {
            if let Ok(proxy) = reqwest::Proxy::https(&url) {
                builder = builder.proxy(proxy);
            }
        }
        if let Ok(url) = std::env::var("HTTP_PROXY")
            .or_else(|_| std::env::var("http_proxy"))
        {
            if let Ok(proxy) = reqwest::Proxy::http(&url) {
                builder = builder.proxy(proxy);
            }
        }

        let client = builder.build().expect("Failed to build reqwest client");

        let quota = Quota::per_second(NonZeroU32::new(100).unwrap())
            .allow_burst(NonZeroU32::new(200).unwrap());
        let rate_limiter = Arc::new(RateLimiter::direct(quota));

        Self {
            client,
            rate_limiter,
            inflight: Arc::new(DashMap::new()),
            registries: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get_json(
        &self,
        url: &str,
        token: Option<String>,
    ) -> Result<serde_json::Value, RegistryError> {
        let cell = match self.inflight.entry(url.to_string()) {
            Entry::Occupied(entry) => entry.get().clone(),
            Entry::Vacant(entry) => {
                let cell = Arc::new(OnceCell::new());
                entry.insert(cell.clone());
                cell
            }
        };

        let url = url.to_string();
        let token = token.clone();
        cell.get_or_init(|| async {
            self.do_get_json(&url, token).await
        }).await.clone()
    }

    async fn do_get_json(
        &self,
        url: &str,
        token: Option<String>,
    ) -> Result<serde_json::Value, RegistryError> {
        loop {
            self.rate_limiter.until_ready().await;

            let mut req = self.client.get(url);
            if let Some(ref t) = token {
                req = req.header("Authorization", format!("Bearer {}", t));
            }

            let resp = req.send().await.map_err(|e| RegistryError::NetworkError(e.to_string()))?;

            let status = resp.status();
            if status.is_success() {
                return resp.json().await.map_err(|e| RegistryError::NetworkError(e.to_string()));
            }

            if status.as_u16() == 429 {
                let retry_after = resp
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(5);
                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                continue;
            }

            if status.as_u16() == 401 || status.as_u16() == 403 {
                return Err(RegistryError::NetworkError(format!(
                    "authentication required (status {})",
                    status.as_u16()
                )));
            }

            if status.as_u16() == 404 {
                return Err(RegistryError::NotFound(url.to_string()));
            }

            return Err(RegistryError::HttpError(status.as_u16()));
        }
    }

    pub async fn get_bytes(
        &self,
        url: &str,
        token: Option<String>,
    ) -> Result<Vec<u8>, RegistryError> {
        loop {
            self.rate_limiter.until_ready().await;

            let mut req = self.client.get(url);
            if let Some(ref t) = token {
                req = req.header("Authorization", format!("Bearer {}", t));
            }

            let resp = req.send().await.map_err(|e| RegistryError::NetworkError(e.to_string()))?;

            let status = resp.status();
            if status.is_success() {
                return resp.bytes().await.map_err(|e| RegistryError::NetworkError(e.to_string())).map(|b| b.to_vec());
            }

            if status.as_u16() == 429 {
                let retry_after = resp
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(5);
                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                continue;
            }

            if status.as_u16() == 404 {
                return Err(RegistryError::NotFound(url.to_string()));
            }

            return Err(RegistryError::HttpError(status.as_u16()));
        }
    }

    pub fn add_registry(&self, name: &str, config: RegistryConfig) {
        self.registries.write().insert(name.to_string(), config);
    }

    pub fn get_registry(&self, name: &str) -> Option<RegistryConfig> {
        self.registries.read().get(name).cloned()
    }
}

impl Default for RegistryClient {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RegistryManager {
    registry_client: Arc<RegistryClient>,
    npm_registries: HashMap<String, Arc<NpmRegistry>>,
}

impl RegistryManager {
    pub fn new() -> Self {
        Self {
            registry_client: Arc::new(RegistryClient::new()),
            npm_registries: HashMap::new(),
        }
    }

    pub fn registry_client(&self) -> &RegistryClient {
        &self.registry_client
    }

    pub fn add_npm(&mut self, name: &str, base_url: &str, config: Option<RegistryConfig>) {
        let token = config.as_ref().and_then(|c| c.token.clone());
        let npm = Arc::new(NpmRegistry::new_with_client(
            base_url,
            self.registry_client.clone(),
            token,
        ));
        if let Some(cfg) = config {
            self.registry_client.add_registry(name, cfg);
        }
        self.npm_registries.insert(name.to_string(), npm);
    }

    pub fn get_npm(&self, name: &str) -> Option<&Arc<NpmRegistry>> {
        self.npm_registries.get(name)
    }
}

impl Default for RegistryManager {
    fn default() -> Self {
        Self::new()
    }
}
