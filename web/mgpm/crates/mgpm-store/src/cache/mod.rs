//! Package cache with content-addressable tarball storage

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::store::{ContentStore, FileEntry, ImportMethod};
use crate::tarball::TarballExtractor;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPackage {
    pub package_id: String,
    pub name: String,
    pub version: String,
    pub tarball_hash: String,
    pub file_hashes: Vec<String>,
    pub integrity: String,
    pub registry: String,
}

pub struct PackageCache {
    store: ContentStore,
    extractor: TarballExtractor,
    index: RwLock<HashMap<String, CachedPackage>>,
    cache_root: PathBuf,
}

impl PackageCache {
    pub fn new(store_root: PathBuf, cache_root: PathBuf) -> std::io::Result<Self> {
        let store = ContentStore::new(store_root)?;
        let extractor = TarballExtractor::new();
        
        Ok(Self {
            store,
            extractor,
            index: RwLock::new(HashMap::new()),
            cache_root,
        })
    }

    pub fn store(&self) -> &ContentStore {
        &self.store
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_root
    }

    pub fn get_package(&self, package_id: &str) -> Option<CachedPackage> {
        let index = self.index.read().unwrap();
        index.get(package_id).cloned()
    }

    pub fn has_package(&self, package_id: &str) -> bool {
        let index = self.index.read().unwrap();
        index.contains_key(package_id)
    }

    pub fn cache_tarball(
        &self,
        package_id: &str,
        name: &str,
        version: &str,
        tarball_path: &Path,
        registry: &str,
    ) -> std::io::Result<CachedPackage> {
        let tarball_hash = self.store.hash_file(tarball_path)?;
        let (imported_hash, _method) = self.store.import_file(tarball_path)?;
        
        let file_hashes = self.extractor.list_files(tarball_path).unwrap_or_default();
        
        let integrity = format!("sha256-{}", tarball_hash);

        let cached = CachedPackage {
            package_id: package_id.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            tarball_hash: imported_hash,
            file_hashes,
            integrity,
            registry: registry.to_string(),
        };

        {
            let mut index = self.index.write().unwrap();
            index.insert(package_id.to_string(), cached.clone());
        }

        Ok(cached)
    }

    pub fn extract_package(
        &self,
        package_id: &str,
        dest: &Path,
    ) -> std::io::Result<Vec<crate::tarball::ExtractedEntry>> {
        let cached = self.get_package(package_id)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("package not in cache: {}", package_id),
                )
            })?;

        let tarball_path = self.store.get_file(&cached.tarball_hash)?;
        
        let entries = self.extractor.extract(&tarball_path, dest)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        
        for entry in &entries {
            self.store.import_file(dest.join(&entry.path))?;
        }

        Ok(entries)
    }

    pub fn remove_package(&self, package_id: &str) -> std::io::Result<()> {
        if let Some(cached) = self.get_package(package_id) {
            self.store.dec_ref(&cached.tarball_hash)?;
            
            if self.store.get_ref_count(&cached.tarball_hash) == 0 {
                self.store.delete_file(&cached.tarball_hash)?;
            }
        }

        let mut index = self.index.write().unwrap();
        index.remove(package_id);
        
        Ok(())
    }

    pub fn list_packages(&self) -> Vec<CachedPackage> {
        let index = self.index.read().unwrap();
        index.values().cloned().collect()
    }

    pub fn prune(&self) -> std::io::Result<usize> {
        let mut removed = 0;
        
        for hash in self.store.list_files() {
            if hash.ref_count == 0 {
                self.store.delete_file(&hash.hash)?;
                removed += 1;
            }
        }
        
        Ok(removed)
    }

    pub fn cache_size(&self) -> u64 {
        self.store.total_size()
    }

    pub fn package_count(&self) -> usize {
        let index = self.index.read().unwrap();
        index.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_cache() {
        let temp = tempfile::tempdir().unwrap();
        let store_root = temp.path().join("store");
        let cache_root = temp.path().join("cache");
        
        let cache = PackageCache::new(store_root, cache_root).unwrap();
        assert_eq!(cache.package_count(), 0);
    }
}