use crate::solver::ResolvedDep;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const DEFAULT_TTL: Duration = Duration::from_secs(300);

struct CacheEntry<T> {
    value: Arc<[T]>,
    fetched_at: Instant,
}

impl<T: Clone> Clone for CacheEntry<T> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
            fetched_at: self.fetched_at,
        }
    }
}

impl<T> CacheEntry<T> {
    fn is_fresh(&self, ttl: Duration) -> bool {
        self.fetched_at.elapsed() < ttl
    }
}

#[derive(Clone)]
pub struct RegistryCache {
    versions: DashMap<String, CacheEntry<mg_types::Version>>,
    deps: DashMap<String, CacheEntry<ResolvedDep>>,
    ttl: Duration,
}

impl RegistryCache {
    pub fn new() -> Self {
        Self {
            versions: DashMap::new(),
            deps: DashMap::new(),
            ttl: DEFAULT_TTL,
        }
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            versions: DashMap::new(),
            deps: DashMap::new(),
            ttl,
        }
    }

    pub fn get_versions(&self, name: &str) -> Option<Vec<mg_types::Version>> {
        let entry = self.versions.get(name)?;
        if entry.is_fresh(self.ttl) {
            Some(entry.value.to_vec())
        } else {
            drop(entry);
            self.versions.remove(name);
            None
        }
    }

    pub fn insert_versions(&self, name: String, versions: Vec<mg_types::Version>) {
        self.versions.insert(
            name,
            CacheEntry {
                value: versions.into(),
                fetched_at: Instant::now(),
            },
        );
    }

    pub fn get_deps(&self, version_key: &str) -> Option<Vec<ResolvedDep>> {
        let entry = self.deps.get(version_key)?;
        if entry.is_fresh(self.ttl) {
            Some(entry.value.to_vec())
        } else {
            drop(entry);
            self.deps.remove(version_key);
            None
        }
    }

    pub fn insert_deps(&self, version_key: String, deps: Vec<ResolvedDep>) {
        self.deps.insert(
            version_key,
            CacheEntry {
                value: deps.into(),
                fetched_at: Instant::now(),
            },
        );
    }

    pub fn clear(&self) {
        self.versions.clear();
        self.deps.clear();
    }

    pub fn len(&self) -> usize {
        self.versions.len() + self.deps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.versions.is_empty() && self.deps.is_empty()
    }

    pub fn evict_stale(&self) {
        self.versions.retain(|_, entry| entry.is_fresh(self.ttl));
        self.deps.retain(|_, entry| entry.is_fresh(self.ttl));
    }
}

impl Default for RegistryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_versions() -> Vec<mg_types::Version> {
        vec![
            mg_types::Version::parse("1.0.0").unwrap(),
            mg_types::Version::parse("2.0.0").unwrap(),
        ]
    }

    #[test]
    fn test_insert_and_get_versions() {
        let cache = RegistryCache::new();
        cache.insert_versions("react".to_string(), sample_versions());
        let v = cache.get_versions("react").unwrap();
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn test_get_versions_missing() {
        let cache = RegistryCache::new();
        assert!(cache.get_versions("nonexistent").is_none());
    }

    #[test]
    fn test_clear() {
        let cache = RegistryCache::new();
        cache.insert_versions("react".to_string(), sample_versions());
        cache.clear();
        assert!(cache.is_empty());
    }
}
