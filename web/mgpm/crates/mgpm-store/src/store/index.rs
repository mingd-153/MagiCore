use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    pub passed: bool,
    pub integrity_ok: bool,
    pub permissions_ok: bool,
    pub stale_warning: bool,
    pub stale_hours: f64,
    pub last_audit: String,
    pub warnings: Vec<String>,
    pub db_size_mb: u64,
    pub wal_size_kb: u64,
    pub cache_entries: usize,
    pub detected_ram_gb: u64,
}

impl AuditReport {
    pub fn is_healthy(&self) -> bool {
        self.passed && self.integrity_ok && self.permissions_ok
    }

    pub fn is_stale(&self) -> bool {
        self.stale_warning
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub integrity: String,
    pub shard: String,
    pub filename: String,
    pub is_executable: bool,
    pub manifest_json: Option<String>,
    pub metadata: Option<String>,
    pub size_bytes: u64,
    pub compressed_size_bytes: u64,
    pub created_at: u64,
}

impl PackageInfo {
    /// Set metadata from a serializable value (JSON)
    pub fn set_metadata<T: Serialize>(&mut self, value: &T) -> Result<(), serde_json::Error> {
        self.metadata = Some(serde_json::to_string(value)?);
        Ok(())
    }

    /// Get metadata as a deserialized value
    pub fn get_metadata<T: for<'de> Deserialize<'de>>(&self) -> Option<T> {
        self.metadata.as_ref().and_then(|s| serde_json::from_str(s).ok())
    }
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
    fn get_all_packages(&self) -> Result<Vec<PackageInfo>, StoreError>;
    fn check_integrity(&self) -> Result<bool, StoreError> {
        Ok(true)
    }
    fn is_readonly(&self) -> bool {
        false
    }
    fn total_size(&self) -> Result<u64, StoreError>;

    // Audit & Generation methods (default implementations return NotImplemented)
    fn health_check(&self) -> Result<Vec<String>, StoreError> {
        Err(StoreError::Database("health_check not implemented".into()))
    }
    fn vacuum(&self) -> Result<(), StoreError> {
        Err(StoreError::Database("vacuum not implemented".into()))
    }
    fn deep_integrity_check(&self) -> Result<Vec<String>, StoreError> {
        Err(StoreError::Database("deep_integrity_check not implemented".into()))
    }
    fn audit(&self) -> Result<AuditReport, StoreError> {
        Err(StoreError::Database("audit not implemented".into()))
    }
    fn check_permissions(&self) -> Result<Vec<String>, StoreError> {
        Err(StoreError::Database("check_permissions not implemented".into()))
    }
    fn snapshot_permissions(&self) -> Result<(), StoreError> {
        Err(StoreError::Database("snapshot_permissions not implemented".into()))
    }
    fn advance_generation(&self) -> Result<u64, StoreError> {
        Err(StoreError::Database("advance_generation not implemented".into()))
    }
    fn current_generation(&self) -> u64 {
        0
    }
    fn clean_old_generations(&self, _keep: u64) -> Result<u64, StoreError> {
        Err(StoreError::Database("clean_old_generations not implemented".into()))
    }
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
