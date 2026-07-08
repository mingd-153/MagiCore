use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use mg_core::{Version, cffi::json::{iterate_versions, iterate_deps}};

use crate::solver::ResolvedDep;

const DEFAULT_TTL: Duration = Duration::from_secs(300);

struct CacheEntry<T> {
    value: T,
    fetched_at: Instant,
}

impl<T: Clone> Clone for CacheEntry<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
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
    versions: DashMap<String, CacheEntry<Arc<[Version]>>>,
    deps: DashMap<String, CacheEntry<Arc<[ResolvedDep]>>>,
    metadata: DashMap<String, CacheEntry<String>>, // raw JSON string
    ttl: Duration,
}

impl RegistryCache {
    pub fn new() -> Self {
        Self {
            versions: DashMap::new(),
            deps: DashMap::new(),
            metadata: DashMap::new(),
            ttl: DEFAULT_TTL,
        }
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            versions: DashMap::new(),
            deps: DashMap::new(),
            metadata: DashMap::new(),
            ttl,
        }
    }

    pub fn get_versions(&self, name: &str) -> Option<Vec<Version>> {
        let entry = self.versions.get(name)?;
        if entry.is_fresh(self.ttl) {
            Some(entry.value.to_vec())
        } else {
            drop(entry);
            self.versions.remove(name);
            None
        }
    }

    pub fn insert_versions(&self, name: String, versions: Vec<Version>) {
        self.versions.insert(name, CacheEntry {
            value: versions.into(),
            fetched_at: Instant::now(),
        });
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
        self.deps.insert(version_key, CacheEntry {
            value: deps.into(),
            fetched_at: Instant::now(),
        });
    }

    /// Get raw JSON metadata from cache (string)
    pub fn get_metadata(&self, name: &str) -> Option<String> {
        let entry = self.metadata.get(name)?;
        if entry.is_fresh(self.ttl) {
            Some(entry.value.clone())
        } else {
            drop(entry);
            self.metadata.remove(name);
            None
        }
    }

    /// Get package versions directly from C FFI (no serde_json deserialize)
    pub fn get_versions_from_json(&self, _name: &str, json: &str) -> Vec<Version> {
        iterate_versions(json)
            .into_iter()
            .filter_map(|v| Version::parse(&v).ok())
            .collect()
    }

    /// Get package dependencies directly from C FFI (no serde_json deserialize)
    pub fn get_deps_from_json(&self, json: &str, version: &str) -> Vec<ResolvedDep> {
        iterate_deps(json, version)
            .into_iter()
            .filter_map(|(pkg, spec)| {
                mg_core::PackageName::new(&pkg).ok().map(|name| ResolvedDep {
                    package: name,
                    spec,
                    optional: false,
                    peer: false,
                })
            })
            .collect()
    }

    pub fn insert_metadata(&self, name: String, metadata: serde_json::Value) {
        self.metadata.insert(name, CacheEntry {
            value: serde_json::to_string(&metadata).unwrap_or_default(),
            fetched_at: Instant::now(),
        });
    }

    pub fn clear(&self) {
        self.versions.clear();
        self.deps.clear();
        self.metadata.clear();
    }

    pub fn len(&self) -> usize {
        self.versions.len() + self.deps.len() + self.metadata.len()
    }

    pub fn is_empty(&self) -> bool {
        self.versions.is_empty() && self.deps.is_empty() && self.metadata.is_empty()
    }

    pub fn evict_stale(&self) {
        let ttl = self.ttl;
        self.versions.retain(|_, entry| entry.is_fresh(ttl));
        self.deps.retain(|_, entry| entry.is_fresh(ttl));
        self.metadata.retain(|_, entry| entry.is_fresh(ttl));
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
    use mg_core::PackageName;

    fn sample_versions() -> Vec<Version> {
        vec![
            Version::parse("1.0.0").unwrap(),
            Version::parse("2.0.0").unwrap(),
        ]
    }

    #[test]
    fn test_insert_and_get_versions() {
        let cache = RegistryCache::new();
        cache.insert_versions("react".to_string(), sample_versions());
        let versions = cache.get_versions("react").unwrap();
        assert_eq!(versions.len(), 2);
    }

    #[test]
    fn test_get_versions_missing() {
        let cache = RegistryCache::new();
        assert!(cache.get_versions("nonexistent").is_none());
    }

    #[test]
    fn test_ttl_expiry() {
        let cache = RegistryCache::with_ttl(Duration::from_nanos(1));
        cache.insert_versions("react".to_string(), sample_versions());
        std::thread::sleep(Duration::from_nanos(100));
        assert!(cache.get_versions("react").is_none());
    }

    #[test]
    fn test_deps() {
        let cache = RegistryCache::new();
        let deps = vec![
            ResolvedDep {
                package: PackageName::new("loose-envify").unwrap(),
                spec: "^1.1.0".to_string(),
                optional: false,
                peer: false,
            },
        ];
        cache.insert_deps("react@18.2.0".to_string(), deps.clone());
        let cached = cache.get_deps("react@18.2.0").unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].package.as_str(), "loose-envify");
    }

    #[test]
    fn test_evict_stale() {
        let cache = RegistryCache::with_ttl(Duration::from_nanos(1));
        cache.insert_versions("react".to_string(), sample_versions());
        std::thread::sleep(Duration::from_nanos(100));
        cache.evict_stale();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_clear() {
        let cache = RegistryCache::new();
        cache.insert_versions("react".to_string(), sample_versions());
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_c_json_extract_versions() {
        let cache = RegistryCache::new();
        let json = r#"{
            "name": "react",
            "versions": {
                "18.2.0": { "name": "react", "version": "18.2.0", "dependencies": {} },
                "19.0.0": { "name": "react", "version": "19.0.0", "dependencies": {} }
            }
        }"#;
        let versions = cache.get_versions_from_json("react", json);
        assert_eq!(versions.len(), 2);
        assert!(versions.iter().any(|v| v.to_string() == "18.2.0"));
        assert!(versions.iter().any(|v| v.to_string() == "19.0.0"));
    }

    #[test]
    fn test_c_json_extract_deps() {
        let cache = RegistryCache::new();
        let json = r#"{
            "name": "react",
            "versions": {
                "18.2.0": { "name": "react", "version": "18.2.0", "dependencies": { "loose-envify": "^1.1.0" } }
            }
        }"#;
        let deps = cache.get_deps_from_json(json, "18.2.0");
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].package.as_str(), "loose-envify");
        assert_eq!(deps[0].spec, "^1.1.0");
    }
}
