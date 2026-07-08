/// Package cache for downloaded tarballs and metadata.

use anyhow::Result;
use mg_types::PackageId;
use std::path::PathBuf;

pub struct PackageCache {
    root: PathBuf,
}

impl PackageCache {
    pub fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Path to cached tarball for a given package version
    pub fn tarball_path(&self, id: &PackageId) -> PathBuf {
        self.root
            .join(id.name_str())
            .join(id.version().to_string())
            .with_extension("tgz")
    }

    /// Path to cached metadata JSON for a package
    pub fn metadata_path(&self, name: &str) -> PathBuf {
        self.root.join(name).join("metadata.json")
    }

    /// Check if a package version is cached
    pub fn contains_tarball(&self, id: &PackageId) -> bool {
        self.tarball_path(id).exists()
    }

    /// Check if package metadata is cached
    pub fn contains_metadata(&self, name: &str) -> bool {
        self.metadata_path(name).exists()
    }

    /// Cache a tarball to disk
    pub fn cache_tarball(&self, id: &PackageId, data: &[u8]) -> Result<()> {
        let path = self.tarball_path(id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Cache metadata JSON
    pub fn cache_metadata(&self, name: &str, data: &[u8]) -> Result<()> {
        let path = self.metadata_path(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Read cached tarball
    pub fn get_tarball(&self, id: &PackageId) -> Result<Option<Vec<u8>>> {
        let path = self.tarball_path(id);
        if path.exists() {
            Ok(Some(std::fs::read(path)?))
        } else {
            Ok(None)
        }
    }

    /// Clear all cached packages
    pub fn clear(&self) -> Result<()> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root)?;
            std::fs::create_dir_all(&self.root)?;
        }
        Ok(())
    }

    /// Total number of cached tarballs
    pub fn tarball_count(&self) -> usize {
        let mut count = 0;
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let pkg_dir = entry.path();
                if pkg_dir.is_dir() {
                    count += std::fs::read_dir(&pkg_dir)
                        .map(|e| e.filter_map(|e| e.ok()).filter(|e| e.path().extension().unwrap_or_default() == "tgz").count())
                        .unwrap_or(0);
                }
            }
        }
        count
    }

    /// Total disk usage in bytes
    pub fn disk_usage(&self) -> u64 {
        let mut total = 0u64;
        let entries = walkdir::WalkDir::new(&self.root).into_iter();
        for entry in entries.flatten() {
            if entry.file_type().is_file() {
                if let Ok(meta) = entry.metadata() {
                    total += meta.len();
                }
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_cache_tarball() {
        let dir = tempdir().unwrap();
        let cache = PackageCache::new(dir.path().to_path_buf()).unwrap();
        let id = PackageId::parse("react@18.2.0").unwrap();
        assert!(!cache.contains_tarball(&id));
        cache.cache_tarball(&id, b"tarball data").unwrap();
        assert!(cache.contains_tarball(&id));
        let data = cache.get_tarball(&id).unwrap().unwrap();
        assert_eq!(data, b"tarball data");
    }

    #[test]
    fn test_cache_metadata() {
        let dir = tempdir().unwrap();
        let cache = PackageCache::new(dir.path().to_path_buf()).unwrap();
        assert!(!cache.contains_metadata("react"));
        cache.cache_metadata("react", b"{}").unwrap();
        assert!(cache.contains_metadata("react"));
    }

    #[test]
    fn test_clear() {
        let dir = tempdir().unwrap();
        let cache = PackageCache::new(dir.path().to_path_buf()).unwrap();
        let id = PackageId::parse("react@18.2.0").unwrap();
        cache.cache_tarball(&id, b"data").unwrap();
        assert!(cache.contains_tarball(&id));
        cache.clear().unwrap();
        assert!(!cache.contains_tarball(&id));
    }
}
