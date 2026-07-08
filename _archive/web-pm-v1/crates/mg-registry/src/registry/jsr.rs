//! JSR Registry Client

use std::time::Duration;

use crate::registry::RegistryError;
use mg_core::PackageName;

const MAX_RETRIES: u32 = 3;
const BASE_RETRY_DELAY_MS: u64 = 500;
const MAX_RETRY_DELAY_MS: u64 = 30_000;

fn retry_delay(attempt: u32) -> Duration {
    let delay = BASE_RETRY_DELAY_MS * 2u64.pow(attempt.saturating_sub(1));
    Duration::from_millis(delay.min(MAX_RETRY_DELAY_MS))
}

pub struct JsrRegistry {
    client: reqwest::Client,
    base_url: String,
}

impl JsrRegistry {
    pub fn new(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!("failed to build reqwest client: {e}, using default");
                reqwest::Client::new()
            });
        Self {
            client,
            base_url: base_url.to_string(),
        }
    }

    pub async fn get_package(
        &self,
        name: &PackageName,
    ) -> Result<serde_json::Value, RegistryError> {
        let url = format!("{}/{}", self.base_url, name.as_str());
        let mut attempt = 0u32;
        loop {
            let resp = self.client.get(&url).send().await?;
            if resp.status().is_success() {
                return Ok(resp.json().await?);
            }

            let status = resp.status().as_u16();
            attempt += 1;
            if attempt > MAX_RETRIES {
                return Err(RegistryError::HttpError(status));
            }
            tokio::time::sleep(retry_delay(attempt)).await;
        }
    }
}
