//! Cache metadata management.

use mgc_types::{MgError, MgResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Cache entry metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub model_id: String,
    pub size_bytes: u64,
    pub cached_at: SystemTime,
    pub last_accessed: SystemTime,
    pub checksum: Option<String>,
}

impl CacheEntry {
    pub fn from_path(model_id: &str, path: &PathBuf) -> MgResult<Self> {
        let metadata = std::fs::metadata(path)?;
        let cached_at = metadata.created().unwrap_or_else(|_| SystemTime::now());
        let last_accessed = metadata.accessed().unwrap_or_else(|_| SystemTime::now());

        Ok(CacheEntry {
            model_id: model_id.to_string(),
            size_bytes: metadata.len(),
            cached_at,
            last_accessed,
            checksum: None,
        })
    }

    pub fn age_days(&self) -> u64 {
        SystemTime::now()
            .duration_since(self.cached_at)
            .map(|d| d.as_secs() / 86400)
            .unwrap_or(0)
    }

    pub fn days_since_access(&self) -> u64 {
        SystemTime::now()
            .duration_since(self.last_accessed)
            .map(|d| d.as_secs() / 86400)
            .unwrap_or(0)
    }
}

/// Save cache metadata to JSON
pub fn save_metadata(path: &Path, entry: &CacheEntry) -> MgResult<()> {
    let json = serde_json::to_string_pretty(entry)
        .map_err(|e| MgError::Other(format!("Serialize metadata: {}", e)))?;

    let meta_file = path.with_extension("meta.json");
    std::fs::write(&meta_file, json)?;

    Ok(())
}

/// Load cache metadata from JSON
pub fn load_metadata(path: &PathBuf) -> MgResult<CacheEntry> {
    let meta_file = path.with_extension("meta.json");

    if !meta_file.exists() {
        // Fallback: create from path
        return CacheEntry::from_path("unknown", path);
    }

    let json = std::fs::read_to_string(&meta_file)?;
    let entry: CacheEntry = serde_json::from_str(&json)
        .map_err(|e| MgError::Other(format!("Deserialize metadata: {}", e)))?;

    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn test_cache_entry_from_path() {
        let tmp = tmp();
        let model = tmp.path().join("model.bin");
        std::fs::write(&model, vec![0u8; 1024]).unwrap();

        let entry = CacheEntry::from_path("test/model", &model).unwrap();
        assert_eq!(entry.model_id, "test/model");
        assert_eq!(entry.size_bytes, 1024);
    }

    #[test]
    fn test_cache_entry_age() {
        let entry = CacheEntry {
            model_id: "test".into(),
            size_bytes: 0,
            cached_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            checksum: None,
        };

        assert_eq!(entry.age_days(), 0);
        assert_eq!(entry.days_since_access(), 0);
    }

    #[test]
    fn test_save_load_metadata() {
        let tmp = tmp();
        let model = tmp.path().join("model.bin");
        std::fs::write(&model, b"test").unwrap();

        let entry = CacheEntry::from_path("bert/base", &model).unwrap();
        save_metadata(&model, &entry).unwrap();

        let loaded = load_metadata(&model).unwrap();
        assert_eq!(loaded.model_id, "bert/base");
        assert_eq!(loaded.size_bytes, 4);
    }
}
