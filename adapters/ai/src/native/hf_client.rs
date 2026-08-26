//! HuggingFace Hub API client — HTTP thật qua reqwest.
// (HuggingFace Hub API client — real HTTP via reqwest.)

use super::{ApiClient, ModelInfo};
use mgc_types::{MgError, MgResult};
use serde_json::Value;

/// HuggingFace Hub client
#[derive(Debug, Clone)]
pub struct HuggingFaceClient {
    pub api_url: String,
    pub token: Option<String>,
}

impl HuggingFaceClient {
    pub fn new() -> Self {
        HuggingFaceClient {
            api_url: "https://huggingface.co/api".to_string(),
            token: None,
        }
    }

    pub fn with_token(mut self, token: String) -> Self {
        self.token = Some(token);
        self
    }

    pub fn with_api_url(mut self, url: String) -> Self {
        self.api_url = url;
        self
    }

    /// GET JSON từ endpoint — gắn Bearer token nếu có, kiểm tra status.
    /// pub(crate): dùng chung cho registry query (một đường HTTP duy nhất cho HF).
    // (GET JSON from endpoint — Bearer token when present, status checked.
    // crate-visible so registry queries share the single HF HTTP path.)
    pub(crate) async fn get_json(&self, path: &str) -> MgResult<Value> {
        let url = format!("{}{}", self.api_url, path);
        let mut req = reqwest::Client::new().get(&url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let response = req
            .send()
            .await
            .map_err(|e| MgError::Network(format!("HuggingFace request failed: {e}")))?;
        if !response.status().is_success() {
            return Err(MgError::Network(format!(
                "HuggingFace HTTP {}: {url}",
                response.status()
            )));
        }
        response
            .json()
            .await
            .map_err(|e| MgError::Other(format!("HuggingFace JSON parse failed: {e}")))
    }
}

impl Default for HuggingFaceClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse ModelInfo từ JSON của HF API — hàm thuần để test offline.
// (Parse ModelInfo from HF API JSON — pure fn for offline tests.)
pub(crate) fn parse_model_info(json: &Value, fallback_id: &str) -> ModelInfo {
    ModelInfo {
        id: json
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(fallback_id)
            .to_string(),
        author: json
            .get("author")
            .and_then(Value::as_str)
            .map(str::to_string),
        downloads: json.get("downloads").and_then(Value::as_u64),
        tags: json
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        pipeline_tag: json
            .get("pipeline_tag")
            .and_then(Value::as_str)
            .map(str::to_string),
        size_bytes: None, // HF metadata không trả size tổng — điền khi có sibling info
    }
}

/// Parse danh sách model id từ mảng JSON trả về bởi /api/models.
// (Parse model-id list from the /api/models JSON array.)
pub(crate) fn parse_model_ids(json: &Value) -> Vec<String> {
    json.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[async_trait::async_trait]
impl ApiClient for HuggingFaceClient {
    async fn get_model_info(&self, model_id: &str) -> MgResult<ModelInfo> {
        if model_id.trim().is_empty() {
            return Err(MgError::Other("model id is empty".into()));
        }
        let json = self.get_json(&format!("/models/{model_id}")).await?;
        Ok(parse_model_info(&json, model_id))
    }

    async fn list_models(&self, limit: Option<usize>) -> MgResult<Vec<String>> {
        let limit = limit.unwrap_or(10);
        let json = self.get_json(&format!("/models?limit={limit}")).await?;
        Ok(parse_model_ids(&json))
    }

    async fn model_exists(&self, model_id: &str) -> MgResult<bool> {
        if model_id.trim().is_empty() {
            return Ok(false);
        }
        let url = format!("{}/models/{model_id}", self.api_url);
        let mut req = reqwest::Client::new().head(&url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let response = req
            .send()
            .await
            .map_err(|e| MgError::Network(format!("HuggingFace HEAD failed: {e}")))?;
        Ok(response.status().is_success())
    }
}

/// Download model file từ HuggingFace Hub → Vec<u8> (dùng chung http_download).
// (Download a model file from the Hub into memory via the shared http_download.)
pub async fn download_file(
    model_id: &str,
    filename: &str,
    revision: Option<&str>,
    token: Option<&str>,
) -> MgResult<Vec<u8>> {
    let url = format!(
        "https://huggingface.co/{}/{}/{}",
        model_id,
        revision.unwrap_or("main"),
        filename
    );
    let mut req = reqwest::Client::new().get(&url);
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }
    let response = req
        .send()
        .await
        .map_err(|e| MgError::Network(format!("HuggingFace download failed: {e}")))?;
    if !response.status().is_success() {
        return Err(MgError::Network(format!(
            "HuggingFace HTTP {}: {url}",
            response.status()
        )));
    }
    // ponytail: full-buffer — file model lớn cần stream → dùng install::download_model (P3 resume)
    // (ponytail: full-buffer — large models should use install::download_model; P3 adds streaming/resume)
    let bytes = response
        .bytes()
        .await
        .map_err(|e| MgError::Network(format!("download body failed: {e}")))?;
    Ok(bytes.to_vec())
}

/// Get model card (README.md) từ HuggingFace
pub async fn get_model_card(model_id: &str) -> MgResult<String> {
    let url = format!("https://huggingface.co/{}/raw/main/README.md", model_id);
    let response = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| MgError::Network(format!("model card request failed: {e}")))?;
    if !response.status().is_success() {
        return Err(MgError::Network(format!(
            "model card HTTP {}: {url}",
            response.status()
        )));
    }
    response
        .text()
        .await
        .map_err(|e| MgError::Network(format!("model card body failed: {e}")))
}

#[cfg(test)]
#[path = "test/hf_client_tests.rs"]
mod tests;
