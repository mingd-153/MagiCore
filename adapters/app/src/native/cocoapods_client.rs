//! CocoaPods HTTP client (iOS/macOS packages).
use super::{AppPackageMetadata, AppRegistryClient};
use mgc_types::{MgError, MgResult, PackageId, PackageName, Version};

#[allow(dead_code)] // P2: nối HTTP thật khi implement fetch — wired in P2
pub struct CocoaPodsClient {
    http_client: reqwest::Client,
}

impl CocoaPodsClient {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
        }
    }
}

impl Default for CocoaPodsClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AppRegistryClient for CocoaPodsClient {
    async fn fetch_metadata(&self, _name: &PackageName) -> MgResult<AppPackageMetadata> {
        Err(MgError::Other(
            "CocoaPods metadata fetch not implemented yet (P2)".into(),
        ))
    }
    async fn download_package(&self, _package_id: &PackageId) -> MgResult<Vec<u8>> {
        Err(MgError::Other(
            "CocoaPods package download not implemented yet (P2)".into(),
        ))
    }
    async fn list_versions(&self, _name: &PackageName) -> MgResult<Vec<Version>> {
        Err(MgError::Other(
            "CocoaPods version listing not implemented yet (P2)".into(),
        ))
    }
}
