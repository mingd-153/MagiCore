//! HTTP Registry Client (direct tarball URLs)

use std::time::Duration;

use crate::registry::RegistryError;

const MAX_RETRIES: u32 = 3;
const BASE_RETRY_DELAY_MS: u64 = 500;
const MAX_RETRY_DELAY_MS: u64 = 30_000;

fn retry_delay(attempt: u32) -> Duration {
    let delay = BASE_RETRY_DELAY_MS * 2u64.pow(attempt.saturating_sub(1));
    Duration::from_millis(delay.min(MAX_RETRY_DELAY_MS))
}

pub struct HttpRegistry {
    client: reqwest::Client,
}

impl HttpRegistry {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!("failed to build reqwest client: {e}, using default");
                reqwest::Client::new()
            });
        Self { client }
    }

    pub async fn get_tarball(&self, url: &str) -> Result<Vec<u8>, RegistryError> {
        let mut attempt = 0u32;
        loop {
            let resp = self.client.get(url).send().await?;
            if resp.status().is_success() {
                return Ok(resp.bytes().await?.to_vec());
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

impl Default for HttpRegistry {
    fn default() -> Self {
        Self::new()
    }
}
