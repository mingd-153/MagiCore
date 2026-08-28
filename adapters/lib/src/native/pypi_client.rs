//! `pypi_client.rs` — Native PyPI HTTP client.
//! Direct API access to PyPI (PEP 503 Simple Repository API + JSON API).

use super::{PackageMetadata, RegistryClient};
use mgc_types::{MgError, MgResult, PackageId, PackageName, Version};
use serde::Deserialize;

/// PyPI HTTP client.
/// Client HTTP PyPI.
///
/// Uses PyPI JSON API: https://warehouse.pypa.io/api-reference/json.html
pub struct PyPiClient {
    registry_url: String,
    http_client: reqwest::Client,
}

impl PyPiClient {
    /// Create new PyPI client.
    /// Tạo client PyPI mới.
    pub fn new() -> Self {
        Self {
            registry_url: "https://pypi.org".to_string(),
            http_client: reqwest::Client::new(),
        }
    }

    /// Create client with custom registry URL.
    /// Tạo client với registry URL tùy chỉnh.
    pub fn with_registry(registry_url: String) -> Self {
        Self {
            registry_url,
            http_client: reqwest::Client::new(),
        }
    }

    /// Construct JSON API URL for package.
    /// Xây dựng JSON API URL cho package.
    fn api_url(&self, name: &PackageName) -> String {
        format!("{}/pypi/{}/json", self.registry_url, name.as_str())
    }
}

#[async_trait::async_trait]
impl RegistryClient for PyPiClient {
    async fn fetch_metadata(&self, name: &PackageName) -> MgResult<PackageMetadata> {
        let url = self.api_url(name);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| MgError::Other(format!("failed to fetch package metadata: {}", e)))?;

        if !response.status().is_success() {
            return Err(MgError::Other(format!(
                "package not found or registry error: {}",
                response.status()
            )));
        }

        let data: PyPiMetadata = response
            .json()
            .await
            .map_err(|e| MgError::Other(format!("failed to parse metadata: {}", e)))?;

        let mut versions = Vec::new();
        for ver_str in data.releases.keys() {
            if let Ok(version) = Version::parse(ver_str) {
                versions.push(version);
            }
        }

        versions.sort();

        let latest = Version::parse(&data.info.version)
            .map_err(|e| MgError::Other(format!("invalid latest version: {}", e)))?;

        Ok(PackageMetadata {
            name: name.clone(),
            versions,
            latest,
            description: Some(data.info.summary),
            homepage: data.info.home_page,
            repository: data.info.project_url,
        })
    }

    async fn download_package(&self, package_id: &PackageId) -> MgResult<Vec<u8>> {
        // First fetch metadata to get download URL
        // Đầu tiên lấy metadata để có download URL
        let metadata_url = self.api_url(package_id.name());

        let response = self
            .http_client
            .get(&metadata_url)
            .send()
            .await
            .map_err(|e| MgError::Other(format!("failed to fetch package metadata: {}", e)))?;

        let data: PyPiMetadata = response
            .json()
            .await
            .map_err(|e| MgError::Other(format!("failed to parse metadata: {}", e)))?;

        // Find the release for this version
        // Tìm release cho version này
        let version_str = package_id.version().to_string();
        let releases = data
            .releases
            .get(&version_str)
            .ok_or_else(|| MgError::Other(format!("version {} not found", version_str)))?;

        // Prefer wheel, fallback to sdist
        // Ưu tiên wheel, fallback sang sdist
        let file = releases
            .iter()
            .find(|r| r.packagetype == "bdist_wheel")
            .or_else(|| releases.iter().find(|r| r.packagetype == "sdist"))
            .ok_or_else(|| MgError::Other("no downloadable files found".to_string()))?;

        // Download the file
        // Tải file
        let response = self
            .http_client
            .get(&file.url)
            .send()
            .await
            .map_err(|e| MgError::Other(format!("failed to download package: {}", e)))?;

        if !response.status().is_success() {
            return Err(MgError::Other(format!(
                "download failed: {}",
                response.status()
            )));
        }

        response
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| MgError::Other(format!("failed to read package data: {}", e)))
    }

    async fn list_versions(&self, name: &PackageName) -> MgResult<Vec<Version>> {
        let metadata = self.fetch_metadata(name).await?;
        Ok(metadata.versions)
    }
}

impl Default for PyPiClient {
    fn default() -> Self {
        Self::new()
    }
}

/// PyPI JSON API metadata response.
/// Response metadata JSON API PyPI.
#[derive(Debug, Deserialize)]
struct PyPiMetadata {
    info: PyPiInfo,
    releases: std::collections::HashMap<String, Vec<PyPiRelease>>,
}

#[derive(Debug, Deserialize)]
struct PyPiInfo {
    version: String,
    summary: String,
    home_page: Option<String>,
    project_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PyPiRelease {
    url: String,
    packagetype: String,
}
