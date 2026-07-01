use std::path::{Path, PathBuf};
use crate::error::CacheError;
use crate::memmap::MemMapCache;
use crate::CacheEntry;

pub struct ETagStore {
    cache: MemMapCache,
    path: PathBuf,
}

impl ETagStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CacheError> {
        let path = path.as_ref().to_path_buf();
        let cache = MemMapCache::open(&path)?;
        Ok(Self { cache, path })
    }

    pub fn get_etag(&self, url: &str) -> Option<String> {
        self.cache.get(url).map(|e| String::from_utf8_lossy(e.data).to_string())
    }

    pub fn store(&mut self, url: &str, etag: &str) -> Result<(), CacheError> {
        let entry = CacheEntry { name: url, data: etag.as_bytes() };
        self.cache.insert(entry)?;
        self.cache.flush()?;
        Ok(())
    }

    pub fn clear(&mut self) -> Result<(), CacheError> {
        let path = self.path.clone();
        self.cache = MemMapCache::open(path)?;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.cache.entry_count() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod tests {
    use super::*;

    #[test]
    fn test_etag_store_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("etags.mgpm_cache");
        let mut store = ETagStore::open(&path).unwrap();

        store.store("https://registry.npmjs.org/lodash", r#""abc123""#).unwrap();

        let etag = store.get_etag("https://registry.npmjs.org/lodash");
        assert_eq!(etag, Some(r#""abc123""#.to_string()));
    }

    #[test]
    fn test_etag_store_miss() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("etags.mgpm_cache");
        let store = ETagStore::open(&path).unwrap();

        assert!(store.get_etag("https://example.com/nonexistent").is_none());
    }

    #[test]
    fn test_etag_store_multiple_urls() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("etags.mgpm_cache");
        let mut store = ETagStore::open(&path).unwrap();

        store.store("https://registry.npmjs.org/lodash", r#""abc""#).unwrap();
        store.store("https://registry.npmjs.org/react", r#""def""#).unwrap();
        store.store("https://registry.npmjs.org/vue", r#""ghi""#).unwrap();

        assert_eq!(store.len(), 3);
        assert_eq!(store.get_etag("https://registry.npmjs.org/lodash"), Some(r#""abc""#.to_string()));
        assert_eq!(store.get_etag("https://registry.npmjs.org/react"), Some(r#""def""#.to_string()));
        assert_eq!(store.get_etag("https://registry.npmjs.org/vue"), Some(r#""ghi""#.to_string()));
    }
}
