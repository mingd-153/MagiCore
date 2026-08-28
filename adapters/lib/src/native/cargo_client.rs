//! `cargo_client.rs` — Native crates.io HTTP client.
//! Direct API access to crates.io (sparse index protocol).

use super::{PackageMetadata, RegistryClient};
use mgc_types::{MgError, MgResult, PackageId, PackageName, Version};
use serde::Deserialize;

/// Crates.io HTTP client.
/// Client HTTP crates.io.
///
/// Uses sparse index protocol (RFC 2789): https://doc.rust-lang.org/cargo/reference/registry-index.html
pub struct CargoClient {
    registry_url: String,
    http_client: reqwest::Client,
}

impl CargoClient {
    /// Create new crates.io client.
    /// Tạo client crates.io mới.
    pub fn new() -> Self {
        Self {
            registry_url: "https://index.crates.io".to_string(),
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

    /// Construct sparse index URL for crate.
    /// Xây dựng sparse index URL cho crate.
    ///
    /// Format: {registry}/lo/gg/logger (for crate "logger")
    fn index_url(&self, name: &PackageName) -> String {
        let name = name.as_str();
        let path = match name.len() {
            1 => format!("1/{}", name),
            2 => format!("2/{}", name),
            3 => format!("3/{}/{}", &name[..1], name),
            _ => format!("{}/{}/{}", &name[..2], &name[2..4], name),
        };
        format!("{}/{}", self.registry_url, path)
    }

    /// Construct download URL for crate.
    /// Xây dựng download URL cho crate.
    fn download_url(&self, package_id: &PackageId) -> String {
        format!(
            "https://crates.io/api/v1/crates/{}/{}/download",
            package_id.name(),
            package_id.version()
        )
    }
}

#[async_trait::async_trait]
impl RegistryClient for CargoClient {
    async fn fetch_metadata(&self, name: &PackageName) -> MgResult<PackageMetadata> {
        let url = self.index_url(name);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| MgError::Other(format!("failed to fetch crate metadata: {}", e)))?;

        if !response.status().is_success() {
            return Err(MgError::Other(format!(
                "crate not found or registry error: {}",
                response.status()
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|e| MgError::Other(format!("failed to read response: {}", e)))?;

        // Parse newline-delimited JSON (each line is a version entry)
        // Parse newline-delimited JSON (mỗi dòng là version entry)
        let mut versions = Vec::new();
        let mut latest = None;

        for line in body.lines() {
            if line.trim().is_empty() {
                continue;
            }

            let entry: CrateVersion = serde_json::from_str(line)
                .map_err(|e| MgError::Other(format!("failed to parse crate entry: {}", e)))?;

            if !entry.yanked {
                let version = Version::parse(&entry.vers)
                    .map_err(|e| MgError::Other(format!("invalid version: {}", e)))?;

                if latest.as_ref().is_none_or(|current| version > *current) {
                    latest = Some(version.clone());
                }

                versions.push(version);
            }
        }

        let latest = latest.ok_or_else(|| MgError::Other("no valid versions found".to_string()))?;

        Ok(PackageMetadata {
            name: name.clone(),
            versions,
            latest,
            description: None,
            homepage: None,
            repository: None,
        })
    }

    async fn download_package(&self, package_id: &PackageId) -> MgResult<Vec<u8>> {
        let url = self.download_url(package_id);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| MgError::Other(format!("failed to download crate: {}", e)))?;

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
            .map_err(|e| MgError::Other(format!("failed to read crate data: {}", e)))
    }

    async fn list_versions(&self, name: &PackageName) -> MgResult<Vec<Version>> {
        let metadata = self.fetch_metadata(name).await?;
        Ok(metadata.versions)
    }
}

impl Default for CargoClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Crate version entry from sparse index.
/// Entry version crate từ sparse index.
#[derive(Debug, Deserialize)]
struct CrateVersion {
    vers: String,
    yanked: bool,
}
