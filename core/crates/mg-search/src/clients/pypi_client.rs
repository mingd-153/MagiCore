//! PyPI search client - SHARED FOR ALL CORES
//! Client tìm kiếm PyPI - DÙNG CHUNG CHO MỌI CORE

use async_trait::async_trait;
use crate::{Registry, ResultMetadata, SearchClient, SearchResult};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

/// PyPI package info format
/// Format thông tin package PyPI
#[derive(Debug, Deserialize)]
struct PyPIPackageResponse {
    info: PyPIInfo,
}

#[derive(Debug, Deserialize)]
struct PyPIInfo {
    name: String,
    version: String,
    summary: Option<String>,
}

/// PyPI search client - usable by web, game, ai, cloud, iot, app, lib, cicd cores
/// Client tìm kiếm PyPI - dùng được cho web, game, ai, cloud, iot, app, lib, cicd cores
pub struct PyPISearchClient {
    client: Client,
    api_url: String,
}

impl PyPISearchClient {
    /// Create new PyPI search client
    /// Tạo PyPI search client mới
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap_or_default(),
            api_url: "https://pypi.org/pypi".to_string(),
        }
    }

    /// Try exact match first, then common variations
    /// Thử exact match trước, sau đó thử các biến thể phổ biến
    async fn try_variants(&self, query: &str) -> anyhow::Result<Vec<SearchResult>> {
        let variants = vec![
            query.to_string(),
            format!("python-{}", query),
            format!("{}-python", query),
            format!("py{}", query),
        ];

        let mut results = Vec::new();
        for variant in variants {
            if let Ok(result) = self.fetch_package(&variant).await {
                results.push(result);
            }
        }

        Ok(results)
    }

    async fn fetch_package(&self, name: &str) -> anyhow::Result<SearchResult> {
        let url = format!("{}/{}/json", self.api_url, name);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "MegaGate/0.4.1")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("PyPI package not found: {}", name);
        }

        let data: PyPIPackageResponse = response.json().await?;

        Ok(SearchResult {
            name: data.info.name.clone(),
            registry: Registry::PyPI,
            full_path: data.info.name,
            version: data.info.version,
            description: data.info.summary.unwrap_or_default(),
            metadata: ResultMetadata {
                downloads: None,
                stars: None,
                updated: String::new(),
                quality: None,
            },
            score: 0.0,
        })
    }
}

impl Default for PyPISearchClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchClient for PyPISearchClient {
    fn registry(&self) -> Registry {
        Registry::PyPI
    }

    async fn search(&self, query: &str) -> anyhow::Result<Vec<SearchResult>> {
        self.try_variants(query).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pypi_client_creation() {
        let client = PyPISearchClient::new();
        assert_eq!(client.registry(), Registry::PyPI);
    }
}
