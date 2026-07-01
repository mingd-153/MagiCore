use std::sync::Arc;

use url::Url;

use crate::registry::{RegistryClient, RegistryError};
use mgpm_core::{PackageMetadata, PackageName, Version};

pub struct NpmRegistry {
    client: Arc<RegistryClient>,
    base_url: String,
    token: Option<String>,
}

impl NpmRegistry {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: Arc::new(RegistryClient::new()),
            base_url: base_url.to_string(),
            token: None,
        }
    }

    pub fn new_with_client(
        base_url: &str,
        client: Arc<RegistryClient>,
        token: Option<String>,
    ) -> Self {
        Self {
            client,
            base_url: base_url.to_string(),
            token,
        }
    }

    pub fn with_token(mut self, token: String) -> Self {
        self.token = Some(token);
        self
    }

    pub fn set_token(&mut self, token: Option<String>) {
        self.token = token;
    }

    pub async fn get_package(
        &self,
        name: &PackageName,
    ) -> Result<serde_json::Value, RegistryError> {
        let url = format!("{}/{}", self.base_url, name.as_str());
        self.client.get_json(&url, self.token.clone()).await
    }

    pub async fn get_tarball(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> Result<String, RegistryError> {
        let url = format!("{}/{}/{}", self.base_url, name.as_str(), version);
        let json: serde_json::Value = self.client.get_json(&url, self.token.clone()).await?;
        json.get("dist")
            .and_then(|d| d.get("tarball"))
            .and_then(|t| t.as_str())
            .map(String::from)
            .ok_or(RegistryError::TarballNotFound)
    }

    pub async fn get_package_versions(
        &self,
        name: &PackageName,
    ) -> Result<Vec<Version>, RegistryError> {
        let json = self.get_package(name).await?;
        let versions_map = json["versions"]
            .as_object()
            .ok_or_else(|| RegistryError::NotFound(name.to_string()))?;
        let mut versions: Vec<Version> = versions_map
            .keys()
            .filter_map(|v| Version::parse(v).ok())
            .collect();
        versions.sort();
        versions.dedup();
        Ok(versions)
    }

    pub async fn search(&self, query: &str) -> Result<Vec<PackageMetadata>, RegistryError> {
        let base =
            Url::parse(&self.base_url).map_err(|e| RegistryError::NetworkError(e.to_string()))?;
        let url = base
            .join("/-/v1/search")
            .map_err(|e| RegistryError::NetworkError(e.to_string()))?;
        let url = Url::parse_with_params(url.as_str(), &[("text", query)])
            .map_err(|e| RegistryError::NetworkError(e.to_string()))?;

        let json = self
            .client
            .get_json(url.as_str(), self.token.clone())
            .await?;

        let objects = json["objects"]
            .as_array()
            .ok_or_else(|| RegistryError::NotFound("search results".to_string()))?;

        let mut results = Vec::with_capacity(objects.len());
        for obj in objects {
            if let Some(pkg) = obj.get("package") {
                if let Some(name_str) = pkg.get("name").and_then(|v| v.as_str()) {
                    if let Ok(name) = PackageName::new(name_str) {
                        let version = pkg
                            .get("version")
                            .and_then(|v| v.as_str())
                            .and_then(|v| Version::parse(v).ok())
                            .unwrap_or_default();

                        let keywords: Vec<String> = pkg
                            .get("keywords")
                            .and_then(|k| k.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();

                        results.push(PackageMetadata {
                            name,
                            version,
                            description: pkg
                                .get("description")
                                .and_then(|d| d.as_str())
                                .map(String::from),
                            author: None,
                            license: pkg
                                .get("license")
                                .and_then(|l| l.as_str())
                                .map(String::from),
                            repository: None,
                            homepage: None,
                            keywords,
                            dependencies: Vec::new(),
                            dev_dependencies: Vec::new(),
                            peer_dependencies: Vec::new(),
                            optional_dependencies: Vec::new(),
                            versions: Vec::new(),
                            created: None,
                            modified: None,
                        });
                    }
                }
            }
        }

        Ok(results)
    }
}
