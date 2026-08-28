// Metadata cache for core-web — in-memory LRU plus shared-cache stale fallback.
// Cache metadata cho core-web — tách TTL/ETag/fallback khỏi shared cache root.
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use lru::LruCache;
use mgc_resolver::DependencyError;
use mgc_types::PackageName;
use serde::{Deserialize, Serialize};

use crate::cache::{current_unix_secs, SharedWebCache};
use crate::native;

const MAX_METADATA_CACHE_ENTRIES: usize = 2048;
const METADATA_CACHE_TTL_SECS: u64 = 6 * 60 * 60;

pub struct MetadataCache {
    cache: Mutex<LruCache<String, (Arc<native::npm_registry::PackageMetadata>, Instant)>>,
    ttl: Duration,
}

impl MetadataCache {
    pub fn new() -> Self {
        let max_entries = std::env::var("MAGICORE_WEB_METADATA_CACHE_MAX_ENTRIES")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(MAX_METADATA_CACHE_ENTRIES);
        let ttl_secs = std::env::var("MAGICORE_WEB_METADATA_CACHE_TTL_SECS")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(METADATA_CACHE_TTL_SECS);
        Self {
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(max_entries).unwrap_or(NonZeroUsize::MIN),
            )),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    fn guard(
        &self,
    ) -> MutexGuard<'_, LruCache<String, (Arc<native::npm_registry::PackageMetadata>, Instant)>>
    {
        self.cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn get(&self, key: &str) -> Option<Arc<native::npm_registry::PackageMetadata>> {
        let mut cache = self.guard();
        if let Some((meta, instant)) = cache.get(key) {
            if instant.elapsed() < self.ttl {
                return Some(Arc::clone(meta));
            } else {
                cache.pop(key);
            }
        }
        None
    }

    pub fn insert(&self, key: String, meta: Arc<native::npm_registry::PackageMetadata>) {
        let mut cache = self.guard();
        cache.put(key, (meta, Instant::now()));
    }
}

impl Default for MetadataCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMetadataEnvelope {
    pub fetched_at: u64,
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub stale_retry_after: Option<u64>,
    pub metadata: native::npm_registry::PackageMetadata,
}

#[derive(Debug, Clone)]
pub struct CachedMetadataRecord {
    pub fetched_at: u64,
    pub etag: Option<String>,
    pub stale_retry_after: Option<u64>,
    pub metadata: native::npm_registry::PackageMetadata,
}

pub fn metadata_record_is_fresh(record: &CachedMetadataRecord) -> bool {
    if record.fetched_at == 0 {
        return false;
    }
    current_unix_secs().saturating_sub(record.fetched_at) <= metadata_ttl_secs()
}

pub fn metadata_record_retry_deferred(record: &CachedMetadataRecord) -> bool {
    record
        .stale_retry_after
        .is_some_and(|retry_after| retry_after > current_unix_secs())
}

pub fn metadata_record_is_usable_stale(record: &CachedMetadataRecord) -> bool {
    if record.fetched_at == 0 {
        return true;
    }
    current_unix_secs().saturating_sub(record.fetched_at) <= metadata_max_stale_fallback_secs()
}

pub fn metadata_ttl_secs() -> u64 {
    std::env::var("MAGICORE_WEB_METADATA_TTL_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(6 * 60 * 60)
}

pub fn metadata_max_stale_fallback_secs() -> u64 {
    std::env::var("MAGICORE_WEB_METADATA_MAX_STALE_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ttl| *ttl > 0)
        .unwrap_or(24 * 60 * 60)
}

pub fn metadata_stale_retry_ttl_secs() -> u64 {
    std::env::var("MAGICORE_WEB_METADATA_STALE_RETRY_TTL_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ttl| *ttl > 0)
        .unwrap_or(30)
}

pub fn metadata_concurrency_limit() -> usize {
    std::env::var("MAGICORE_WEB_METADATA_CONCURRENCY")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|limit| *limit > 0)
        .unwrap_or(24)
}

pub fn next_stale_retry_after() -> u64 {
    current_unix_secs().saturating_add(metadata_stale_retry_ttl_secs())
}

pub async fn load_metadata_with_fallback(
    package: &PackageName,
    registry: &native::npm_registry::NpmRegistry,
    shared_cache: Option<&SharedWebCache>,
) -> Result<Arc<native::npm_registry::PackageMetadata>, DependencyError> {
    load_metadata_by_name_with_fallback(package.as_str(), registry, shared_cache).await
}

pub async fn load_metadata_by_name_with_fallback(
    package: &str,
    registry: &native::npm_registry::NpmRegistry,
    shared_cache: Option<&SharedWebCache>,
) -> Result<Arc<native::npm_registry::PackageMetadata>, DependencyError> {
    let registry_url = registry.registry_url().to_string();
    let cached = if let Some(shared_cache) = shared_cache {
        shared_cache.read_metadata(package, &registry_url)?
    } else {
        None
    };

    if let Some(cached) = cached.as_ref() {
        if metadata_record_is_fresh(cached) {
            return Ok(Arc::new(cached.metadata.clone()));
        }

        if metadata_record_retry_deferred(cached) && metadata_record_is_usable_stale(cached) {
            return Ok(Arc::new(cached.metadata.clone()));
        }

        if let Some(etag) = &cached.etag {
            match registry
                .fetch_metadata_conditional(package, Some(etag))
                .await
            {
                Ok(None) => {
                    if let Some(shared_cache) = shared_cache {
                        let _ = shared_cache.write_metadata(
                            package,
                            &cached.metadata,
                            Some(etag.clone()),
                            &registry_url,
                        );
                    }
                    return Ok(Arc::new(cached.metadata.clone()));
                }
                Ok(Some((metadata, new_etag))) => {
                    if let Some(shared_cache) = shared_cache {
                        let _ = shared_cache.write_metadata(
                            package,
                            &metadata,
                            Some(new_etag),
                            &registry_url,
                        );
                    }
                    return Ok(Arc::new(metadata));
                }
                Err(_) => {
                    if !metadata_record_is_usable_stale(cached) {
                        return Err(DependencyError(format!(
                            "npm metadata refresh failed for '{}' and cached metadata is too old to reuse",
                            package
                        )));
                    }
                    if let Some(shared_cache) = shared_cache {
                        let _ = shared_cache.write_metadata_record(
                            package,
                            &cached.metadata,
                            cached.etag.clone(),
                            cached.fetched_at,
                            Some(next_stale_retry_after()),
                            &registry_url,
                        );
                    }
                    return Ok(Arc::new(cached.metadata.clone()));
                }
            }
        }
    }

    match registry.fetch_metadata_with_etag(package).await {
        Ok((metadata, etag)) => {
            if let Some(shared_cache) = shared_cache {
                let _ = shared_cache.write_metadata_record(
                    package,
                    &metadata,
                    etag,
                    current_unix_secs(),
                    None,
                    &registry_url,
                );
            }
            Ok(Arc::new(metadata))
        }
        Err(e) => {
            if let Some(cached) = cached {
                if metadata_record_is_usable_stale(&cached) {
                    if let Some(shared_cache) = shared_cache {
                        let _ = shared_cache.write_metadata_record(
                            package,
                            &cached.metadata,
                            cached.etag.clone(),
                            cached.fetched_at,
                            Some(next_stale_retry_after()),
                            &registry_url,
                        );
                    }
                    return Ok(Arc::new(cached.metadata));
                }
            }
            Err(DependencyError(format!(
                "failed to fetch metadata for '{}': {}",
                package, e
            )))
        }
    }
}
