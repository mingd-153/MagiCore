/// HTTP client for MegaGate
/// 
/// Provides retry logic, rate limiting, caching, and progress tracking.

use anyhow::Result;
use reqwest::{Client, Response};
use std::time::Duration;

pub mod retry;
pub mod ratelimit;
pub mod cache;

/// HTTP client configuration
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    pub timeout: Duration,
    pub max_retries: u32,
    pub user_agent: String,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_retries: 3,
            user_agent: format!("MegaGate/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

/// HTTP client with retry and rate limiting
pub struct HttpClient {
    client: Client,
    config: HttpClientConfig,
}

impl HttpClient {
    pub fn new(config: HttpClientConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .build()?;

        Ok(Self { client, config })
    }

    pub fn default() -> Result<Self> {
        Self::new(HttpClientConfig::default())
    }

    /// GET request with retry
    pub async fn get(&self, url: &str) -> Result<Response> {
        let mut attempts = 0;
        loop {
            match self.client.get(url).send().await {
                Ok(resp) if resp.status().is_success() => return Ok(resp),
                Ok(resp) if attempts >= self.config.max_retries => {
                    anyhow::bail!("HTTP GET failed after {} retries: {}", attempts, resp.status());
                }
                Err(e) if attempts >= self.config.max_retries => {
                    anyhow::bail!("HTTP GET failed after {} retries: {}", attempts, e);
                }
                _ => {
                    attempts += 1;
                    tokio::time::sleep(Duration::from_millis(100 * 2_u64.pow(attempts))).await;
                }
            }
        }
    }

    /// Download file as bytes
    pub async fn download(&self, url: &str) -> Result<Vec<u8>> {
        let response = self.get(url).await?;
        Ok(response.bytes().await?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let client = HttpClient::default();
        assert!(client.is_ok());
    }
}
