//! NPM Registry Client

use mgpm_core::{PackageName, Version};
use crate::RegistryError;

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
