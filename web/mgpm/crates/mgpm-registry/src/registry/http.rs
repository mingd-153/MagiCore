//! HTTP Registry Client (direct tarball URLs)

use mgpm_core::{PackageName, Version};
use crate::registry::RegistryError;

pub struct HttpRegistry {
    client: reqwest::Client,
}

impl HttpRegistry {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }

    pub async fn get_tarball(&self, url: &str) -> Result<Vec<u8>, RegistryError> {
        let resp = self.client.get(url).send().await?;
        if resp.status().is_success() {
            Ok(resp.bytes().await?.to_vec())
        } else {
            Err(RegistryError::HttpError(resp.status().as_u16()))
        }
    }
}
