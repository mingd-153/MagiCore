use megagate_types::error::{MegagateError, Result};
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;

pub struct FetchPool {
    client: Arc<Client>,
}

impl FetchPool {
    pub fn new(max_concurrency: usize, timeout: Duration) -> Self {
        let client = Client::builder()
            .pool_max_idle_per_host(max_concurrency)
            .timeout(timeout)
            .build()
            .expect("Failed to create HTTP client");
        Self {
            client: Arc::new(client),
        }
    }

    pub async fn get(&self, url: &str) -> Result<reqwest::Response> {
        self.client
            .get(url)
            .send()
            .await
            .map_err(|e| MegagateError::NetworkError(e.to_string()))
    }

    pub async fn head(&self, url: &str) -> Result<reqwest::Response> {
        self.client
            .head(url)
            .send()
            .await
            .map_err(|e| MegagateError::NetworkError(e.to_string()))
    }
}

impl Clone for FetchPool {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
        }
    }
}