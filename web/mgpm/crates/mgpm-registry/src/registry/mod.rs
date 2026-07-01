use base64::Engine;
use sha2::{Digest, Sha256};

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use parking_lot::{Mutex, RwLock};
use tokio::sync::OnceCell;

use mgpm_cache::{CacheEntry, ETagStore, MemMapCache};
use mgpm_core::{PackageId, RegistryConfig};

use crate::http::{DownloadError, DownloadManager, DownloadRequest, DownloadedPackage};

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

type InflightCell = Arc<OnceCell<Result<serde_json::Value, RegistryError>>>;
type InflightMap = DashMap<String, InflightCell>;

pub struct RegistryClient {
    client: reqwest::Client,
    rate_limiter: Arc<DefaultDirectRateLimiter>,
    inflight: Arc<InflightMap>,
    registries: RwLock<HashMap<String, RegistryConfig>>,
    cache: Option<Mutex<MemMapCache>>,
    etag_store: Option<Mutex<ETagStore>>,
    downloader: DownloadManager,
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

        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mgpm")
            .join("cache");

        let cache_path = cache_dir.join("registry.mgpm_cache");
        let cache = MemMapCache::open(&cache_path).ok().map(Mutex::new);

        let etag_path = cache_dir.join("etags.mgpm_cache");
        let etag_store = match ETagStore::open(&etag_path) {
            Ok(store) => Some(Mutex::new(store)),
            Err(e) => {
                tracing::warn!("failed to open ETag cache ({}), ETag disabled", e);
                None
            }
        };

        Self {
            client,
            rate_limiter,
            inflight: Arc::new(DashMap::new()),
            registries: RwLock::new(HashMap::new()),
            cache,
            etag_store,
            downloader: DownloadManager::default(),
        }
    }

    pub async fn get_json(
        &self,
        url: &str,
        token: Option<String>,
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
        let has_etag = self.etag_store.is_some();
        let result = cell.get_or_init(|| async {
            if has_etag {
                self.do_get_json_with_etag(&url, token).await
            } else {
                self.do_get_json(&url, token).await
            }
        }).await.clone();

        if !has_etag {
            if let Ok(ref val) = result {
                if let Some(ref cache) = self.cache {
                    if let Ok(data) = serde_json::to_vec(val) {
                        let entry = CacheEntry {
                            name: url.as_str(),
                            data: &data,
                        };
                        let mut guard = cache.lock();
                        let _ = guard.insert(entry);
                        let _ = guard.flush();
                    }
                }
            }
        }

        result
    }

    async fn do_get_json_with_etag(
        &self,
        url: &str,
        token: Option<String>,
    ) -> Result<serde_json::Value, RegistryError> {
        let mut send_etag = true;

        loop {
            let etag = if send_etag {
                self.etag_store.as_ref().and_then(|s| s.lock().get_etag(url))
            } else {
                None
            };

            self.rate_limiter.until_ready().await;

            let mut req = self.client.get(url);
            if let Some(ref t) = token {
                req = req.header("Authorization", format!("Bearer {}", t));
            }
            if let Some(ref e) = etag {
                req = req.header("If-None-Match", e);
            }

            let resp = req.send().await.map_err(|e| RegistryError::NetworkError(e.to_string()))?;
            let status = resp.status();

            match status.as_u16() {
                304 => {
                    tracing::debug!("304 Not Modified: {}", url);
                    let body = self.cache.as_ref()
                        .and_then(|c| {
                            let guard = c.lock();
                            let entry = guard.get(url)?;
                            Some(entry.data.to_vec())
                        });
                    if let Some(body) = body {
                        return serde_json::from_slice(&body)
                            .map_err(|e| RegistryError::NetworkError(e.to_string()));
                    }
                    tracing::warn!("304 but no cached body for {}, retrying without ETag", url);
                    send_etag = false;
                    continue;
                }
                200..=202 => {
                    let etag_val = resp.headers()
                        .get("etag")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());

                    let body = resp.bytes().await
                        .map_err(|e| RegistryError::NetworkError(e.to_string()))?;

                    if let Some(ref store) = self.etag_store {
                        if let Some(ref etag) = etag_val {
                            let mut guard = store.lock();
                            let _ = guard.store(url, etag);
                        }
                    }

                    if let Some(ref cache) = self.cache {
                        let entry = CacheEntry { name: url, data: &body };
                        let mut guard = cache.lock();
                        let _ = guard.insert(entry);
                        let _ = guard.flush();
                    }

                    return serde_json::from_slice(&body)
                        .map_err(|e| RegistryError::NetworkError(e.to_string()));
                }
                429 => {
                    let retry_after = resp
                        .headers()
                        .get("Retry-After")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(5);
                    tokio::time::sleep(Duration::from_secs(retry_after)).await;
                    continue;
                }
                401 | 403 => {
                    return Err(RegistryError::NetworkError(format!(
                        "authentication required (status {})",
                        status.as_u16()
                    )));
                }
                404 => return Err(RegistryError::NotFound(url.to_string())),
                _ => return Err(RegistryError::HttpError(status.as_u16())),
            }
        }
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

    /// Download a package tarball by PackageId (name@version)
    pub async fn download_tarball(
        &self,
        package_id: &PackageId,
    ) -> Result<Vec<u8>, RegistryError> {
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
            let actual = Self::compute_sha256_sri(&bytes);
            if &actual != expected {
                return Err(RegistryError::NetworkError(format!(
                    "integrity mismatch: expected {}, got {}",
                    expected, actual
                )));
            }
        }

        Ok(bytes)
    }

    /// Download multiple tarballs concurrently using DownloadManager
    pub async fn download_batch(
        &self,
        packages: &[DownloadRequest],
    ) -> Vec<Result<DownloadedPackage, RegistryError>> {
        let results = self.downloader.download_batch(packages).await;
        results
            .into_iter()
            .map(|r| r.map_err(|e| match e {
                DownloadError::HttpError(code) => RegistryError::HttpError(code),
                DownloadError::NetworkError(msg) => RegistryError::NetworkError(msg),
                DownloadError::Timeout(url) => {
                    RegistryError::NetworkError(format!("timeout: {}", url))
                }
                DownloadError::IntegrityMismatch { expected, actual } => {
                    RegistryError::NetworkError(format!(
                        "integrity mismatch: expected {}, got {}",
                        expected, actual
                    ))
                }
            }))
            .collect()
    }

    /// Access the DownloadManager for custom batch operations
    pub fn download_manager(&self) -> &DownloadManager {
        &self.downloader
    }

    /// Compute SHA-256 SRI format hash (sha256-<base64>)
    fn compute_sha256_sri(bytes: &[u8]) -> String {
        let hash = Sha256::digest(bytes);
        let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(hash);
        format!("sha256-{}", b64)
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
