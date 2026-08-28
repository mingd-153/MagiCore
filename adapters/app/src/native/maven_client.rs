//! Maven Central HTTP client (Kotlin/Android packages).
use super::{AppPackageMetadata, AppRegistryClient};
use mgc_types::{MgError, MgResult, PackageId, PackageName, Version};

#[allow(dead_code)] // P2: nối HTTP thật khi implement fetch — wired in P2
pub struct MavenClient {
    http_client: reqwest::Client,
}

impl MavenClient {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
        }
    }
}

impl Default for MavenClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AppRegistryClient for MavenClient {
    async fn fetch_metadata(&self, _name: &PackageName) -> MgResult<AppPackageMetadata> {
        Err(MgError::Other(
            "Maven Central metadata fetch not implemented yet (P2)".into(),
        ))
    }
    async fn download_package(&self, _package_id: &PackageId) -> MgResult<Vec<u8>> {
        Err(MgError::Other(
            "Maven package download not implemented yet (P2)".into(),
        ))
    }
    async fn list_versions(&self, _name: &PackageName) -> MgResult<Vec<Version>> {
        Err(MgError::Other(
            "Maven version listing not implemented yet (P2)".into(),
        ))
    }
}
