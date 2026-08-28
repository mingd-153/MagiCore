//! `cache/metadata.rs` — Cache metadata tracking for lib adapter.
//! Tracks package metadata in cache (similar to web cache_metadata.rs).

use mgc_types::{MgError, MgResult, PackageId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Cache metadata for a package.
/// Metadata cache cho package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub package_id: PackageId,
    pub cached_at: u64, // Unix timestamp
    pub size_bytes: u64,
    pub file_path: PathBuf,
}

/// Cache metadata store.
/// Kho metadata cache.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CacheMetadata {
    entries: HashMap<String, CacheEntry>, // key: package_id string
}

impl CacheMetadata {
    /// Load cache metadata from file.
    /// Tải metadata cache từ file.
    pub fn load(path: &Path) -> MgResult<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let data = std::fs::read_to_string(path)
            .map_err(|e| MgError::Other(format!("failed to read metadata: {}", e)))?;

        serde_json::from_str(&data)
            .map_err(|e| MgError::Other(format!("failed to parse metadata: {}", e)))
    }

    /// Save cache metadata to file.
    /// Lưu metadata cache vào file.
    pub fn save(&self, path: &Path) -> MgResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| MgError::Other(format!("failed to create dir: {}", e)))?;
        }

        let data = serde_json::to_string_pretty(self)
            .map_err(|e| MgError::Other(format!("failed to serialize metadata: {}", e)))?;

        std::fs::write(path, data)
            .map_err(|e| MgError::Other(format!("failed to write metadata: {}", e)))
    }

    /// Add entry to cache metadata.
    /// Thêm entry vào metadata cache.
    pub fn add(&mut self, entry: CacheEntry) {
        self.entries.insert(entry.package_id.to_string(), entry);
    }

    /// Remove entry from cache metadata.
    /// Xóa entry khỏi metadata cache.
    pub fn remove(&mut self, package_id: &PackageId) -> Option<CacheEntry> {
        self.entries.remove(&package_id.to_string())
    }

    /// Get entry from cache metadata.
    /// Lấy entry từ metadata cache.
    pub fn get(&self, package_id: &PackageId) -> Option<&CacheEntry> {
        self.entries.get(&package_id.to_string())
    }

    /// List all entries.
    /// Liệt kê tất cả entries.
    pub fn entries(&self) -> impl Iterator<Item = &CacheEntry> {
        self.entries.values()
    }

    /// Total cache size in bytes.
    /// Tổng kích thước cache theo bytes.
    pub fn total_size(&self) -> u64 {
        self.entries.values().map(|e| e.size_bytes).sum()
    }

    /// Count of cached packages.
    /// Số lượng packages đã cache.
    pub fn count(&self) -> usize {
        self.entries.len()
    }
}

/// Get default metadata file path for language.
/// Lấy đường dẫn file metadata mặc định cho ngôn ngữ.
pub fn metadata_path(language: &str) -> MgResult<PathBuf> {
    let cache_dir = super::cache_dir(language)?;
    Ok(cache_dir.join("metadata.json"))
}
