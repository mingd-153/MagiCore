//! JSR Registry Client

use mgpm_core::{PackageName, Version};
use crate::registry::RegistryError;

pub struct JsrRegistry {
    client: reqwest::Client,
    base_url: String,
}

impl JsrRegistry {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
        }
    }

    pub async fn get_package(&self, name: &PackageName) -> Result<serde_json::Value, RegistryError> {
        let url = format!("{}/{}", self.base_url, name.as_str());
        let resp = self.client.get(&url).send().await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(RegistryError::HttpError(resp.status().as_u16()))
        }
    }
}
