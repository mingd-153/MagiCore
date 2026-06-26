use crate::error::Result;
use crate::package::{PackageManifest, PackageRef};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IntegrityInfo {
    pub integrity: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageMetadata {
    pub integrity: String,
    pub size: u64,
    pub extracted_at: chrono::DateTime<chrono::Utc>,
    pub publish_time: Option<chrono::DateTime<chrono::Utc>>,
    pub approved_builds: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PruneResult {
    pub removed: usize,
    pub freed_bytes: u64,
}

#[async_trait]
pub trait StoreBackend: Send + Sync {
    async fn init(&self) -> Result<()>;
    async fn exists(&self, pkg: &PackageRef) -> Result<bool>;
    async fn is_extracted(&self, pkg: &PackageRef) -> Result<bool>;
    async fn get_path(&self, pkg: &PackageRef) -> Result<PathBuf>;
    async fn write_tarball_bytes(&self, pkg: &PackageRef, data: &[u8]) -> Result<IntegrityInfo>;
    async fn extract_tarball(&self, pkg: &PackageRef) -> Result<()>;
    async fn read_tarball_bytes(&self, pkg: &PackageRef) -> Result<Vec<u8>>;
    async fn write_manifest(&self, pkg: &PackageRef, manifest: &PackageManifest) -> Result<()>;
    async fn read_manifest(&self, pkg: &PackageRef) -> Result<Option<PackageManifest>>;
    async fn write_metadata(&self, pkg: &PackageRef, meta: &PackageMetadata) -> Result<()>;
    async fn read_metadata(&self, pkg: &PackageRef) -> Result<Option<PackageMetadata>>;
    async fn create_hardlink(&self, pkg: &PackageRef, target: &PathBuf) -> Result<()>;
    async fn create_symlink(&self, pkg: &PackageRef, target: &PathBuf) -> Result<()>;
    async fn remove(&self, pkg: &PackageRef) -> Result<()>;
    async fn prune(&self, referenced: HashMap<String, PackageRef>) -> Result<PruneResult>;
    async fn verify_integrity(&self, pkg: &PackageRef) -> Result<bool>;
}
