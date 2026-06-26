use async_trait::async_trait;
use megagate_types::error::{MegagateError, Result};
use megagate_types::registry::{RegistryPackageMetadata, RegistryPackageVersion, RegistryVersionsResponse};
use semver::Version;
use std::sync::Arc;

#[async_trait]
pub trait RegistryClient: Send + Sync {
    async fn get_package_versions(&self, name: &str) -> Result<Vec<RegistryPackageVersion>>;
    async fn get_package_metadata(&self, name: &str, version: &str) -> Result<RegistryPackageMetadata>;
    async fn get_all_versions(&self, name: &str) -> Result<RegistryVersionsResponse>;
}

pub struct NpmRegistryClient {
    base_url: String,
    client: Arc<reqwest::Client>,
}

impl NpmRegistryClient {
    pub fn new(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create registry client");
        Self {
            base_url,
            client: Arc::new(client),
        }
    }
}

#[async_trait]
impl RegistryClient for NpmRegistryClient {
    async fn get_package_versions(&self, name: &str) -> Result<Vec<RegistryPackageVersion>> {
        let url = format!("{}/{}", self.base_url, name);
        let response = self.client.get(&url).send().await
            .map_err(|e| MegagateError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(MegagateError::RegistryError(format!(
                "Failed to get versions for {}: {}", name, response.status()
            )));
        }

        // Get response text first for debugging
        let text = response.text().await
            .map_err(|e| MegagateError::NetworkError(e.to_string()))?;
        
        eprintln!("DEBUG: Response text length: {}", text.len());
        eprintln!("DEBUG: Response text preview: {}", &text[..std::cmp::min(200, text.len())]);
        
        // Deserialize from text
        let data: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| MegagateError::NetworkError(format!("JSON decode error: {}", e)))?;

        let mut versions = Vec::new();
        if let Some(versions_map) = data.get("versions").and_then(|v| v.as_object()) {
            for (_, pkg_value) in versions_map {
                if let Ok(pkg) = serde_json::from_value::<RegistryPackageVersion>(pkg_value.clone()) {
                    versions.push(pkg);
                }
            }
        }
        versions.sort_by(|a, b| {
            let va = Version::parse(&a.version);
            let vb = Version::parse(&b.version);
            match (va, vb) {
                (Ok(va), Ok(vb)) => vb.cmp(&va),
                (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                (Err(_), Err(_)) => std::cmp::Ordering::Equal,
            }
        });
        Ok(versions)
    }

    async fn get_package_metadata(&self, name: &str, version: &str) -> Result<RegistryPackageMetadata> {
        let url = format!("{}/{}/{}", self.base_url, name, version);
        let response = self.client.get(&url).send().await
            .map_err(|e| MegagateError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(MegagateError::RegistryError(format!(
                "Failed to get metadata for {}@{}: {}", name, version, response.status()
            )));
        }

        response.json().await
            .map_err(|e| MegagateError::NetworkError(e.to_string()))
    }

    async fn get_all_versions(&self, name: &str) -> Result<RegistryVersionsResponse> {
        let url = format!("{}/{}", self.base_url, name);
        let response = self.client.get(&url).send().await
            .map_err(|e| MegagateError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(MegagateError::RegistryError(format!(
                "Failed to get all versions for {}: {}", name, response.status()
            )));
        }

        response.json().await
            .map_err(|e| MegagateError::NetworkError(e.to_string()))
    }
}