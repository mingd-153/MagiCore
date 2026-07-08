/// Package fetching and downloading
/// 
/// Downloads packages from registries and caches them locally.

use anyhow::Result;
use mg_http::HttpClient;
use mg_types::{PackageId, Version};
use std::path::PathBuf;

pub mod download;
pub mod extract;

/// Fetcher configuration
#[derive(Debug, Clone)]
pub struct FetcherConfig {
    pub registry_url: String,
    pub cache_dir: PathBuf,
}

impl Default for FetcherConfig {
    fn default() -> Self {
        Self {
            registry_url: "https://registry.megagate.io".to_string(),
            cache_dir: dirs::home_dir()
                .unwrap_or_default()
                .join(".megagate")
                .join("cache"),
        }
    }
}

/// Package fetcher
pub struct Fetcher {
    config: FetcherConfig,
    client: HttpClient,
}

impl Fetcher {
    pub fn new(config: FetcherConfig) -> Result<Self> {
        let client = HttpClient::default()?;
        std::fs::create_dir_all(&config.cache_dir)?;
        
        Ok(Self { config, client })
    }

    pub fn default() -> Result<Self> {
        Self::new(FetcherConfig::default())
    }

    /// Fetch package metadata
    pub async fn fetch_metadata(&self, id: &PackageId) -> Result<String> {
        let url = format!("{}/packages/{}/metadata", self.config.registry_url, id.name_str());
        let response = self.client.get(&url).await?;
        Ok(response.text().await?)
    }

    /// Download package tarball
    pub async fn download_package(&self, id: &PackageId, version: &Version) -> Result<PathBuf> {
        let url = format!(
            "{}/packages/{}/{}/tarball",
            self.config.registry_url,
            id.name_str(),
            version
        );

        let cache_key = format!("{}-{}.tgz", id.name_str(), version);
        let cache_path = self.config.cache_dir.join(&cache_key);

        // Check cache
        if cache_path.exists() {
            return Ok(cache_path);
        }

        // Download
        let data = self.client.download(&url).await?;
        
        // Save to cache
        std::fs::write(&cache_path, data)?;

        Ok(cache_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetcher_config() {
        let config = FetcherConfig::default();
        assert_eq!(config.registry_url, "https://registry.megagate.io");
    }
}
