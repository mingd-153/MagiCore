//! CacheBackend — CAS blob store: get/put/claim/release (T1 refcount đã wire).

use async_trait::async_trait;
use mg_types::error::MgResult;
use std::path::{Path, PathBuf};

#[async_trait]
pub trait CacheBackend: Send + Sync {
    async fn get(&self, key: &str) -> MgResult<Option<PathBuf>>;
    async fn put(&self, key: &str, data: &[u8]) -> MgResult<()>;
    async fn claim(&self, project_root: &Path, blob_hashes: &[String]) -> MgResult<()>;
    async fn release(&self, project_root: &Path) -> MgResult<()>;
}