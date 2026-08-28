//! Native registry clients for app platforms.

pub mod cocoapods_client;
pub mod maven_client;
pub mod pub_client;

use mgc_types::{MgResult, PackageId, PackageName, Version};

#[async_trait::async_trait]
pub trait AppRegistryClient {
    async fn fetch_metadata(&self, name: &PackageName) -> MgResult<AppPackageMetadata>;
    async fn download_package(&self, package_id: &PackageId) -> MgResult<Vec<u8>>;
    async fn list_versions(&self, name: &PackageName) -> MgResult<Vec<Version>>;
}

#[derive(Debug, Clone)]
pub struct AppPackageMetadata {
    pub name: PackageName,
    pub versions: Vec<Version>,
    pub latest: Version,
    pub description: Option<String>,
    pub homepage: Option<String>,
}
