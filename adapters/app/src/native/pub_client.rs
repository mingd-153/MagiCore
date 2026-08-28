//! pub.dev HTTP client (Flutter packages).
use super::{AppPackageMetadata, AppRegistryClient};
use mgc_types::{MgError, MgResult, PackageId, PackageName, Version};

#[allow(dead_code)] // P2: nối HTTP thật khi implement fetch — wired in P2
pub struct PubClient {
    registry_url: String,
    http_client: reqwest::Client,
}

impl PubClient {
    pub fn new() -> Self {
        Self {
            registry_url: "https://pub.dev".to_string(),
            http_client: reqwest::Client::new(),
        }
    }
}

impl Default for PubClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AppRegistryClient for PubClient {
    async fn fetch_metadata(&self, _name: &PackageName) -> MgResult<AppPackageMetadata> {
        Err(MgError::Other(
            "pub.dev metadata fetch not implemented yet (P2)".into(),
        ))
    }
    async fn download_package(&self, _package_id: &PackageId) -> MgResult<Vec<u8>> {
        Err(MgError::Other(
            "pub.dev package download not implemented yet (P2)".into(),
        ))
    }
    async fn list_versions(&self, _name: &PackageName) -> MgResult<Vec<Version>> {
        Err(MgError::Other(
            "pub.dev version listing not implemented yet (P2)".into(),
        ))
    }
}
