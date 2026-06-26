use crate::pool::FetchPool;
use crate::registry_client::RegistryClient;
use megagate_types::error::{MegagateError, Result};
use megagate_types::package::{PackageRef, ResolvedDependency};
use megagate_types::store::{IntegrityInfo, StoreBackend};
use governor::{Quota, RateLimiter};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

pub struct Fetcher {
    store: Arc<dyn StoreBackend>,
    pool: FetchPool,
    registry_client: Arc<dyn RegistryClient>,
    rate_limiter: Arc<RateLimiter<governor::state::NotKeyed, governor::state::InMemoryState, governor::clock::DefaultClock>>,
    config: FetcherConfig,
}

#[derive(Debug, Clone)]
pub struct FetcherConfig {
    pub max_concurrency: usize,
    pub timeout: Duration,
    pub retry_count: u32,
    pub retry_delay: Duration,
    pub rate_limit: u32,
}

impl Default for FetcherConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 16,
            timeout: Duration::from_secs(60),
            retry_count: 3,
            retry_delay: Duration::from_secs(1),
            rate_limit: 100,
        }
    }
}

impl Fetcher {
    pub fn new(
        store: Arc<dyn StoreBackend>,
        pool: FetchPool,
        registry_client: Arc<dyn RegistryClient>,
        config: FetcherConfig,
    ) -> Self {
        let rate_limiter = Arc::new(RateLimiter::direct(Quota::per_second(
            NonZeroU32::new(config.rate_limit).unwrap(),
        )));
        Self {
            store,
            pool,
            registry_client,
            rate_limiter,
            config,
        }
    }

    pub async fn fetch(&self, pkg: &ResolvedDependency) -> Result<IntegrityInfo> {
        let pkg_ref = PackageRef::new(pkg.name.clone(), pkg.version.clone());
        if self.store.is_extracted(&pkg_ref).await? {
            eprintln!("DEBUG: Package {}@{} already extracted, skipping download", pkg.name, pkg.version);
            return Ok(IntegrityInfo {
                integrity: pkg.integrity.clone(),
                size: pkg.size,
            });
        }

        self.rate_limiter.until_ready().await;

        let mut last_error = None;
        for attempt in 0..=self.config.retry_count {
            match self.fetch_once(pkg).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.config.retry_count {
                        tokio::time::sleep(self.config.retry_delay * (attempt + 1) as u32).await;
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    async fn fetch_once(&self, pkg: &ResolvedDependency) -> Result<IntegrityInfo> {
        eprintln!("DEBUG: Fetching {} from {}", pkg.name, pkg.resolved);
        let response = self.pool
            .get(&pkg.resolved)
            .await
            .map_err(|e| {
                eprintln!("DEBUG: Pool get error: {}", e);
                MegagateError::NetworkError(e.to_string())
            })?;

        let status = response.status();
        eprintln!("DEBUG: Response status: {}", status);

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            eprintln!("DEBUG: Error response body: {}", text);
            return Err(MegagateError::RegistryError(format!(
                "HTTP {}: {}",
                status,
                text
            )));
        }

        let bytes = response.bytes().await
            .map_err(|e| {
                eprintln!("DEBUG: Response bytes error: {}", e);
                MegagateError::NetworkError(e.to_string())
            })?;

        eprintln!("DEBUG: Downloaded {} bytes", bytes.len());

        let pkg_ref = PackageRef::new(pkg.name.clone(), pkg.version.clone());
        let integrity_info = self.store.write_tarball_bytes(&pkg_ref, &bytes).await?;
        eprintln!("DEBUG: Calling extract_tarball for {}", pkg.name);
        self.store.extract_tarball(&pkg_ref).await?;
        eprintln!("DEBUG: extract_tarball completed for {}", pkg.name);
        Ok(integrity_info)
    }

    pub async fn fetch_multiple(&self, packages: Vec<ResolvedDependency>) -> Result<HashMap<String, IntegrityInfo>> {
        eprintln!("DEBUG: fetch_multiple called with {} packages", packages.len());
        let mut handles = Vec::new();
        for pkg in packages {
            let fetcher = self.clone();
            let key = format!("{}@{}", pkg.name, pkg.version);
            handles.push(tokio::spawn(async move {
                let result = fetcher.fetch(&pkg).await;
                (key, result)
            }));
        }

        let mut results = HashMap::new();
        for handle in handles {
            let (key, result) = handle.await.map_err(|e| MegagateError::IoError(e.to_string()))?;
            eprintln!("DEBUG: fetch_multiple got result for {}", key);
            let result = result?;
            results.insert(key, result);
        }
        eprintln!("DEBUG: fetch_multiple completed with {} results", results.len());
        Ok(results)
    }
}

impl Clone for Fetcher {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            pool: self.pool.clone(),
            registry_client: self.registry_client.clone(),
            rate_limiter: self.rate_limiter.clone(),
            config: self.config.clone(),
        }
    }
}