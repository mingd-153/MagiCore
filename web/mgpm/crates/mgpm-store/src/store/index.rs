use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub integrity: String,
    pub shard: String,
    pub filename: String,
    pub is_executable: bool,
    pub manifest_json: Option<String>,
    pub size_bytes: u64,
    pub compressed_size_bytes: u64,
    pub created_at: u64,
}

pub trait StoreIndex: Send + Sync {
    fn add_package(&self, info: &PackageInfo) -> Result<(), StoreError>;
    fn get_package(&self, name: &str, version: &str) -> Result<Option<PackageInfo>, StoreError>;
    fn get_by_integrity(&self, hash: &str) -> Result<Option<PackageInfo>, StoreError>;
    fn package_exists(&self, hash: &str) -> Result<bool, StoreError>;
    fn delete_package(&self, hash: &str) -> Result<(), StoreError>;
    fn register_project(&self, path: &Path) -> Result<(), StoreError>;
    fn unregister_project(&self, path: &Path) -> Result<(), StoreError>;
    fn get_unreferenced_packages(&self) -> Result<Vec<PackageInfo>, StoreError>;
    fn update_integrity_cache(&self, file_path: &Path, hash: &str) -> Result<(), StoreError>;
    fn get_cached_integrity(&self, file_path: &Path) -> Result<Option<String>, StoreError>;
    fn begin_transaction(&self) -> Result<(), StoreError>;
    fn commit(&self) -> Result<(), StoreError>;
    fn rollback(&self) -> Result<(), StoreError>;
    fn package_count(&self) -> Result<u64, StoreError>;
    fn project_count(&self) -> Result<u64, StoreError>;
    fn total_size(&self) -> Result<u64, StoreError>;
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("I/O error: {path}: {msg}")]
    Io { path: PathBuf, msg: String },
    #[error("cross-device link: {path}")]
    CrossDevice { path: PathBuf },
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("integrity check failed for {0}")]
    IntegrityCheck(String),
    #[error("database error: {0}")]
    Database(String),
    #[error("cache error: {0}")]
    Cache(String),
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io { path: PathBuf::new(), msg: e.to_string() }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Database(e.to_string())
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

impl From<bincode::Error> for StoreError {
    fn from(e: bincode::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}
