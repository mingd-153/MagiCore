//! `native/mod.rs` — Native registry clients for lib adapter.
//! Direct HTTP clients for crates.io and PyPI (future: avoid cargo/pip exec).

pub mod cargo_client;
pub mod pypi_client;

use mgc_types::{MgResult, PackageId, PackageName, Version};

/// Registry client trait.
/// Trait client registry.
#[async_trait::async_trait]
pub trait RegistryClient {
    /// Fetch package metadata from registry.
    /// Lấy metadata package từ registry.
    async fn fetch_metadata(&self, name: &PackageName) -> MgResult<PackageMetadata>;

    /// Download package tarball/archive.
    /// Tải tarball/archive package.
    async fn download_package(&self, package_id: &PackageId) -> MgResult<Vec<u8>>;

    /// List available versions for package.
    /// Liệt kê versions có sẵn cho package.
    async fn list_versions(&self, name: &PackageName) -> MgResult<Vec<Version>>;
}

/// Package metadata from registry.
/// Metadata package từ registry.
#[derive(Debug, Clone)]
pub struct PackageMetadata {
    pub name: PackageName,
    pub versions: Vec<Version>,
    pub latest: Version,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
}
