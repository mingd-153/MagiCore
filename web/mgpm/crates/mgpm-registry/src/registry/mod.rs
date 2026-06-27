use std::collections::HashMap;
use std::sync::Arc;

use mgpm_core::{PackageId, PackageName, Version};

pub struct NpmRegistry {
    client: reqwest::Client,
    base_url: String,
}

impl NpmRegistry {
    pub fn new(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(64)
            .build()
            .unwrap_or_default();
        Self { client, base_url: base_url.to_string() }
    }

    pub async fn get_package(&self, name: &PackageName) -> Result<serde_json::Value, RegistryError> {
        let url = format!("{}/{}", self.base_url, name.as_str());
        let resp = self.client.get(&url).send().await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(RegistryError::HttpError(resp.status().as_u16()))
        }
    }

    pub async fn get_tarball(&self, name: &PackageName, version: &Version) -> Result<String, RegistryError> {
        let url = format!("{}/{}/{}", self.base_url, name.as_str(), version);
        let json: serde_json::Value = self.client.get(&url).send().await?.json().await?;
        json.get("dist")
            .and_then(|d| d.get("tarball"))
            .and_then(|t| t.as_str())
            .map(String::from)
            .ok_or(RegistryError::TarballNotFound)
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum RegistryError {
    #[error("HTTP error: {0}")]
    HttpError(u16),
    #[error("network error: {0}")]
    NetworkError(String),
    #[error("tarball not found")]
    TarballNotFound,
}

impl From<reqwest::Error> for RegistryError {
    fn from(e: reqwest::Error) -> Self {
        Self::NetworkError(e.to_string())
    }
}

pub struct RegistryManager {
    npm_registries: HashMap<String, Arc<NpmRegistry>>,
}

impl RegistryManager {
    pub fn new() -> Self {
        Self { npm_registries: HashMap::new() }
    }

    pub fn add_npm(&mut self, name: &str, base_url: &str) {
        self.npm_registries.insert(name.to_string(), Arc::new(NpmRegistry::new(base_url)));
    }

    pub fn get_npm(&self, name: &str) -> Option<&Arc<NpmRegistry>> {
        self.npm_registries.get(name)
    }
}

impl Default for RegistryManager {
    fn default() -> Self { Self::new() }
}