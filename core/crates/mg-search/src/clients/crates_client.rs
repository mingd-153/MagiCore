//! crates.io search client - SHARED FOR ALL CORES
//! Client tìm kiếm crates.io - DÙNG CHUNG CHO MỌI CORE

use async_trait::async_trait;
use crate::{Registry, ResultMetadata, SearchClient, SearchResult};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

/// crates.io API response format
/// Format response API crates.io
#[derive(Debug, Deserialize)]
struct CratesSearchResponse {
    crates: Vec<Crate>,
}

#[derive(Debug, Deserialize)]
struct Crate {
    name: String,
    max_version: String,
    description: Option<String>,
    downloads: u64,
    updated_at: String,
}

/// crates.io search client - usable by web, game, ai, cloud, iot, app, lib, cicd cores
/// Client tìm kiếm crates.io - dùng được cho web, game, ai, cloud, iot, app, lib, cicd cores
pub struct CratesSearchClient {
    client: Client,
    api_url: String,
}

impl CratesSearchClient {
    /// Create new crates.io search client
    /// Tạo crates.io search client mới
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap_or_default(),
            api_url: "https://crates.io/api/v1".to_string(),
        }
    }
}

impl Default for CratesSearchClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchClient for CratesSearchClient {
    fn registry(&self) -> Registry {
        Registry::Crates
    }

    async fn search(&self, query: &str) -> anyhow::Result<Vec<SearchResult>> {
        let url = format!(
            "{}/crates?q={}&per_page=20",
            self.api_url,
            urlencoding::encode(query)
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "MegaGate/0.4.1")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("crates.io search failed: HTTP {}", response.status());
        }

        let data: CratesSearchResponse = response.json().await?;

        Ok(data
            .crates
            .into_iter()
            .map(|c| SearchResult {
                name: c.name.clone(),
                registry: Registry::Crates,
                full_path: c.name,
                version: c.max_version,
                description: c.description.unwrap_or_default(),
                metadata: ResultMetadata {
                    downloads: Some(c.downloads),
                    stars: None,
                    updated: c.updated_at,
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
    fn test_crates_client_creation() {
        let client = CratesSearchClient::new();
        assert_eq!(client.registry(), Registry::Crates);
    }
}
