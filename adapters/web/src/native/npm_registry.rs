/// npm registry client — fetches package metadata and tarballs
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub name: String,
    pub description: Option<String>,
    pub versions: std::collections::HashMap<String, VersionInfo>,
    #[serde(rename = "dist-tags")]
    pub dist_tags: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub dependencies: Option<std::collections::HashMap<String, String>>,
    pub dist: Option<DistInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistInfo {
    pub tarball: String,
    #[serde(rename = "integrity")]
    pub integrity: Option<String>,
}

pub struct NpmRegistry {
    registry_url: String,
    client: reqwest::Client,
}

impl NpmRegistry {
    pub fn new(registry_url: &str) -> Self {
        Self {
            registry_url: registry_url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn fetch_metadata(&self, package: &str) -> Result<PackageMetadata> {
        let url = format!("{}/{}", self.registry_url, package);
        let resp = self.client.get(&url).send().await?;
        let metadata: PackageMetadata = resp.json().await?;
        Ok(metadata)
    }

    pub async fn download_tarball(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self.client.get(url).send().await?;
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }
}
