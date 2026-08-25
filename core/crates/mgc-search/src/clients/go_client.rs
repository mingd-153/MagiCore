//! pkg.go.dev search client - SHARED FOR ALL CORES
//! Client tìm kiếm pkg.go.dev - DÙNG CHUNG CHO MỌI CORE

use crate::{Registry, ResultMetadata, SearchClient, SearchResult};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

/// pkg.go.dev API response format
/// Format response API pkg.go.dev
#[derive(Debug, Deserialize)]
struct GoSearchResponse {
    results: Vec<GoResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct GoResult {
    package_path: String,
    version: String,
    synopsis: String,
}

/// Go package search client - usable by web, game, ai, cloud, iot, app, lib, cicd cores
/// Client tìm kiếm Go package - dùng được cho web, game, ai, cloud, iot, app, lib, cicd cores
pub struct GoSearchClient {
    client: Client,
    api_url: String,
}

impl GoSearchClient {
    /// Create new Go search client
    /// Tạo Go search client mới
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap_or_default(),
            api_url: "https://api.pkg.go.dev".to_string(),
        }
    }
}

impl Default for GoSearchClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchClient for GoSearchClient {
    fn registry(&self) -> Registry {
        Registry::Go
    }

    async fn search(&self, query: &str) -> anyhow::Result<Vec<SearchResult>> {
        let url = format!(
            "{}/search?q={}&limit=20",
            self.api_url,
            urlencoding::encode(query)
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "MagiCore/0.4.1")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("pkg.go.dev search failed: HTTP {}", response.status());
        }

        let data: GoSearchResponse = response.json().await?;

        Ok(data
            .results
            .into_iter()
            .map(|r| SearchResult {
                name: r
                    .package_path
                    .split('/')
                    .last()
                    .unwrap_or(&r.package_path)
                    .to_string(),
                registry: Registry::Go,
                full_path: r.package_path,
                version: r.version,
                description: r.synopsis,
                metadata: ResultMetadata {
                    downloads: None,
                    stars: None,
                    updated: String::new(),
                    quality: None,
                },
                score: 0.0,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_go_client_creation() {
        let client = GoSearchClient::new();
        assert_eq!(client.registry(), Registry::Go);
    }
}
