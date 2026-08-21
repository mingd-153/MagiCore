//! npm registry search client - SHARED FOR ALL CORES
//! Client tìm kiếm npm registry - DÙNG CHUNG CHO MỌI CORE

use async_trait::async_trait;
use crate::{Registry, ResultMetadata, SearchClient, SearchResult};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

/// npm search API response format
/// Format response API npm search
#[derive(Debug, Deserialize)]
struct NpmSearchResponse {
    objects: Vec<NpmSearchObject>,
}

#[derive(Debug, Deserialize)]
struct NpmSearchObject {
    package: NpmPackage,
    score: NpmScore,
}

#[derive(Debug, Deserialize)]
struct NpmPackage {
    name: String,
    version: String,
    description: Option<String>,
    date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NpmScore {
    #[serde(rename = "final")]
    final_score: f64,
    detail: NpmScoreDetail,
}

#[derive(Debug, Deserialize)]
struct NpmScoreDetail {
    quality: f64,
    popularity: f64,
    maintenance: f64,
}

/// npm registry search client - usable by web, game, ai, cloud, iot, app, lib, cicd cores
/// Client tìm kiếm npm registry - dùng được cho web, game, ai, cloud, iot, app, lib, cicd cores
pub struct NpmSearchClient {
    client: Client,
    registry_url: String,
}

impl NpmSearchClient {
    /// Create new npm search client
    /// Tạo npm search client mới
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap_or_default(),
            registry_url: "https://registry.npmjs.org".to_string(),
        }
    }
}

impl Default for NpmSearchClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchClient for NpmSearchClient {
    fn registry(&self) -> Registry {
        Registry::Npm
    }

    async fn search(&self, query: &str) -> anyhow::Result<Vec<SearchResult>> {
        let url = format!(
            "{}/-/v1/search?text={}&size=20",
            self.registry_url,
            urlencoding::encode(query)
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "MegaGate/0.4.1")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("npm search failed: HTTP {}", response.status());
        }

        let data: NpmSearchResponse = response.json().await?;

        Ok(data
            .objects
            .into_iter()
            .map(|obj| SearchResult {
                name: obj.package.name.clone(),
                registry: Registry::Npm,
                full_path: obj.package.name,
                version: obj.package.version,
                description: obj.package.description.unwrap_or_default(),
                metadata: ResultMetadata {
                    downloads: None,
                    stars: None,
                    updated: obj.package.date.unwrap_or_default(),
                    quality: Some(obj.score.detail.quality as f32),
                },
                score: 0.0, // Will be computed by ranking module
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_npm_client_creation() {
        let client = NpmSearchClient::new();
        assert_eq!(client.registry(), Registry::Npm);
    }
}
