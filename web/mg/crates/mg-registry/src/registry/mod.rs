use base64::Engine;
use sha2::{Digest, Sha256, Sha512};

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use parking_lot::{Mutex, RwLock};
use tokio::sync::OnceCell;

use mg_cache::{CacheEntry, MemMapCache};
use mg_core::{PackageId, RegistryConfig};

pub mod file;
pub mod git;
pub mod http;
pub mod jsr;
pub mod npm;

pub use file::{FileRegistry, PackageJsonReader, ParsedPackageJson, WorkspaceRegistry};
pub use git::GitRegistry;
pub use http::HttpRegistry;
pub use jsr::JsrRegistry;
pub use npm::NpmRegistry;

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
    #[error("request failed after {0} retries: {1}")]
    RetryFailed(u32, String),
    #[error("cache miss: {0}")]
    CacheMiss(String),
}

// Maximum retries for transient HTTP errors
const MAX_RETRIES: u32 = 3;

// Base delay in milliseconds for exponential backoff
const BASE_RETRY_DELAY_MS: u64 = 500;

// Maximum delay in milliseconds for exponential backoff
const MAX_RETRY_DELAY_MS: u64 = 30_000;

fn retry_delay(attempt: u32) -> Duration {
    let delay = BASE_RETRY_DELAY_MS * 2u64.pow(attempt.saturating_sub(1));
    Duration::from_millis(delay.min(MAX_RETRY_DELAY_MS))
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

type InflightCell = Arc<OnceCell<Result<serde_json::Value, RegistryError>>>;
type InflightMap = DashMap<String, InflightCell>;

pub struct RegistryClient {
    client: reqwest::Client,
    rate_limiter: Arc<DefaultDirectRateLimiter>,
    inflight: Arc<InflightMap>,
    registries: RwLock<HashMap<String, RegistryConfig>>,
    cache: Option<Mutex<MemMapCache>>,
    etags: DashMap<String, String>,
    offline: AtomicBool,
}

impl RegistryClient {
    pub fn new() -> Self {
        let mut builder = reqwest::Client::builder()
            .pool_max_idle_per_host(64)
            .gzip(true)
            .brotli(true)
            .user_agent(concat!("mg/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(60));

        if let Ok(url) = std::env::var("HTTPS_PROXY").or_else(|_| std::env::var("https_proxy")) {
            if let Ok(proxy) = reqwest::Proxy::https(&url) {
                builder = builder.proxy(proxy);
            }
        }
        if let Ok(url) = std::env::var("HTTP_PROXY").or_else(|_| std::env::var("http_proxy")) {
            if let Ok(proxy) = reqwest::Proxy::http(&url) {
                builder = builder.proxy(proxy);
            }
        }

        let client = builder.build().expect("Failed to build reqwest client");

        let quota = Quota::per_second(NonZeroU32::new(100).unwrap())
            .allow_burst(NonZeroU32::new(200).unwrap());
        let rate_limiter = Arc::new(RateLimiter::direct(quota));

        let cache_path = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mg")
            .join("cache")
            .join("registry.mg_cache");
        let cache = MemMapCache::open(&cache_path).ok().map(Mutex::new);

        Self {
            client,
            rate_limiter,
            inflight: Arc::new(DashMap::new()),
            registries: RwLock::new(HashMap::new()),
            cache,
            etags: DashMap::new(),
            offline: AtomicBool::new(false),
        }
    }

    pub async fn get_json(
        &self,
        url: &str,
        token: Option<String>,
    ) -> Result<serde_json::Value, RegistryError> {
        self.get_json_with_accept(url, token, None).await
    }

    pub async fn get_json_with_accept(
        &self,
        url: &str,
        token: Option<String>,
        accept: Option<&str>,
    ) -> Result<serde_json::Value, RegistryError> {
        if let Some(ref cache) = self.cache {
            let guard = cache.lock();
            if let Some(entry) = guard.get(url) {
                if let Ok(val) = serde_json::from_slice(entry.data) {
                    return Ok(val);
                }
            }
        }

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
        let accept = accept.map(|a| a.to_string());
        let result = cell
            .get_or_init(|| async { self.do_get_json(&url, token, accept.as_deref()).await })
            .await
            .clone();

        if let Ok(ref val) = result {
            if let Some(ref cache) = self.cache {
                if let Ok(data) = serde_json::to_vec(val) {
                    #[allow(clippy::disallowed_methods)]
                    let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
                    let entry = CacheEntry {
                        name: url.as_str(),
                        version: "",
                        integrity: "",
                        data: leaked,
                    };
                    let mut guard = cache.lock();
                    let _ = guard.insert(entry);
                    let _ = guard.flush();
                }
            }
        }

        result
    }

    /// Send an HTTP GET with rate limiting, retry, and exponential backoff.
    /// Retries on: transient 404, 5xx, network errors, and 429 (rate limit).
    /// Fails fast on: 401, 403 (auth errors).
    async fn send_with_retry(
        &self,
        url: &str,
        token: Option<String>,
        etag: Option<&str>,
    ) -> Result<reqwest::Response, RegistryError> {
        self.send_with_retry_accept(url, token, etag, None).await
    }

    async fn send_with_retry_accept(
        &self,
        url: &str,
        token: Option<String>,
        etag: Option<&str>,
        accept: Option<&str>,
    ) -> Result<reqwest::Response, RegistryError> {
        let mut attempt = 0u32;

        loop {
            self.rate_limiter.until_ready().await;

            let mut req = self.client.get(url);
            if let Some(ref t) = token {
                req = req.header("Authorization", format!("Bearer {}", t));
            }
            if let Some(e) = etag {
                req = req.header("If-None-Match", e);
            }
            if let Some(a) = accept {
                req = req.header("Accept", a);
            }

            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    attempt += 1;
                    if attempt > MAX_RETRIES {
                        return Err(RegistryError::RetryFailed(
                            attempt - 1,
                            e.to_string(),
                        ));
                    }
                    let delay = retry_delay(attempt);
                    tokio::time::sleep(delay).await;
                    continue;
                }
            };

            let status = resp.status();

            if status.is_success() || status.as_u16() == 304 {
                return Ok(resp);
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

            // Retryable: 404 (transient CDN miss), 5xx (server error)
            attempt += 1;
            if attempt > MAX_RETRIES {
                return if status.as_u16() == 404 {
                    Err(RegistryError::NotFound(url.to_string()))
                } else {
                    Err(RegistryError::HttpError(status.as_u16()))
                };
            }
            let delay = retry_delay(attempt);
            tokio::time::sleep(delay).await;
        }
    }

    async fn do_get_json(
        &self,
        url: &str,
        token: Option<String>,
        accept: Option<&str>,
    ) -> Result<serde_json::Value, RegistryError> {
        if self.is_offline() {
            if let Some(ref cache) = self.cache {
                let guard = cache.lock();
                if let Some(entry) = guard.get(url) {
                    return serde_json::from_slice(entry.data)
                        .map_err(|e| RegistryError::NetworkError(e.to_string()));
                }
            }
            return Err(RegistryError::NetworkError(format!(
                "offline: no cached data for {}",
                url
            )));
        }

        let etag = self.etags.get(url).map(|v| v.clone());
        let resp = self.send_with_retry_accept(url, token, etag.as_deref(), accept).await?;

        let new_etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_string());

        if resp.status() == 304 {
            if let Some(ref cache) = self.cache {
                let guard = cache.lock();
                if let Some(entry) = guard.get(url) {
                    return serde_json::from_slice(entry.data).map_err(|e| RegistryError::NetworkError(e.to_string()));
                }
            }
            return Err(RegistryError::CacheMiss(url.to_string()));
        }

        let val: serde_json::Value = resp.json().await.map_err(|e| RegistryError::NetworkError(e.to_string()))?;

        if let Some(etag_val) = new_etag {
            self.etags.insert(url.to_string(), etag_val);
        }

        Ok(val)
    }

    pub async fn get_bytes(
        &self,
        url: &str,
        token: Option<String>,
    ) -> Result<Vec<u8>, RegistryError> {
        let resp = self.send_with_retry(url, token, None).await?;
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| RegistryError::NetworkError(e.to_string()))
    }

    pub async fn get_raw(
        &self,
        url: &str,
        token: Option<String>,
    ) -> Result<Vec<u8>, RegistryError> {
        self.get_raw_with_accept(url, token, None).await
    }

    pub async fn get_raw_with_accept(
        &self,
        url: &str,
        token: Option<String>,
        accept: Option<&str>,
    ) -> Result<Vec<u8>, RegistryError> {
        if self.is_offline() {
            if let Some(ref cache) = self.cache {
                let guard = cache.lock();
                if let Some(entry) = guard.get(url) {
                    return Ok(entry.data.to_vec());
                }
            }
            return Err(RegistryError::NetworkError(format!(
                "offline: no cached data for {}",
                url
            )));
        }

        let etag = self.etags.get(url).map(|v| v.clone());
        let resp = self.send_with_retry_accept(url, token, etag.as_deref(), accept).await?;

        let new_etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_string());

        if resp.status() == 304 {
            if let Some(ref cache) = self.cache {
                let guard = cache.lock();
                if let Some(entry) = guard.get(url) {
                    return Ok(entry.data.to_vec());
                }
            }
            return Err(RegistryError::CacheMiss(url.to_string()));
        }

        let body = resp
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| RegistryError::NetworkError(e.to_string()))?;

        if let Some(etag_val) = new_etag {
            self.etags.insert(url.to_string(), etag_val);
        }

        Ok(body)
    }

    pub fn set_offline(&self, offline: bool) {
        self.offline.store(offline, Ordering::Relaxed);
    }

    pub fn is_offline(&self) -> bool {
        self.offline.load(Ordering::Relaxed)
    }

    pub fn add_registry(&self, name: &str, config: RegistryConfig) {
        self.registries.write().insert(name.to_string(), config);
    }

    pub fn get_registry(&self, name: &str) -> Option<RegistryConfig> {
        self.registries.read().get(name).cloned()
    }

    /// Download a package tarball by PackageId (name@version)
    pub async fn download_tarball(&self, package_id: &PackageId) -> Result<Vec<u8>, RegistryError> {
        // Use npm registry to get package metadata and extract tarball URL
        let name = package_id.name().as_str();
        let version = package_id.version().to_string();
        let url = format!("https://registry.npmjs.org/{}/{}", name, version);

        // Get package metadata to find tarball URL and expected integrity
        let json = self.get_json(&url, None).await?;

        let tarball_url = json
            .get("dist")
            .and_then(|d| d.get("tarball"))
            .and_then(|t| t.as_str())
            .ok_or(RegistryError::TarballNotFound)?;

        // Get expected integrity from registry metadata (SRI format: sha256-<base64>)
        let expected_integrity = json
            .get("dist")
            .and_then(|d| d.get("integrity"))
            .and_then(|i| i.as_str())
            .map(|s| s.to_string());

        // Download the tarball bytes
        let bytes = self.get_bytes(tarball_url, None).await?;

        // Verify integrity if provided by registry
        if let Some(ref expected) = expected_integrity {
            let actual = Self::compute_sri(&bytes, expected);
            if &actual != expected {
                return Err(RegistryError::NetworkError(format!(
                    "integrity mismatch: expected {}, got {}",
                    expected, actual
                )));
            }
        }

        Ok(bytes)
    }

    /// Compute SRI hash matching the algorithm prefix from expected integrity
    fn compute_sri(bytes: &[u8], expected: &str) -> String {
        if expected.starts_with("sha512-") {
            let hash = Sha512::digest(bytes);
            let b64 = base64::engine::general_purpose::STANDARD.encode(hash);
            format!("sha512-{}", b64)
        } else {
            let hash = Sha256::digest(bytes);
            let b64 = base64::engine::general_purpose::STANDARD.encode(hash);
            format!("sha256-{}", b64)
        }
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
