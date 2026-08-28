//! Model registry abstraction cho AI adapter.
//! Hỗ trợ HuggingFace Hub, TensorFlow Hub, ONNX Model Zoo.

use mgc_types::{MgError, MgResult};
use std::path::{Path, PathBuf};

pub mod parser;

/// Registry variants
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Registry {
    HuggingFace,
    TensorFlowHub,
    OnnxZoo,
    PyTorchHub,
    Local(PathBuf),
}

impl Registry {
    pub fn from_url(url: &str) -> Option<Self> {
        if url.contains("huggingface.co") {
            Some(Registry::HuggingFace)
        } else if url.contains("tfhub.dev") {
            Some(Registry::TensorFlowHub)
        } else if url.contains("onnx") || url.contains("github.com/onnx") {
            Some(Registry::OnnxZoo)
        } else if url.contains("pytorch.org/hub") {
            Some(Registry::PyTorchHub)
        } else if url.starts_with("file://") || url.starts_with('/') {
            Some(Registry::Local(PathBuf::from(
                url.trim_start_matches("file://"),
            )))
        } else {
            None
        }
    }

    pub fn base_url(&self) -> Option<&str> {
        match self {
            Registry::HuggingFace => Some("https://huggingface.co"),
            Registry::TensorFlowHub => Some("https://tfhub.dev"),
            Registry::OnnxZoo => Some("https://github.com/onnx/models"),
            Registry::PyTorchHub => Some("https://pytorch.org/hub"),
            Registry::Local(_) => None,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Registry::HuggingFace => "huggingface",
            Registry::TensorFlowHub => "tfhub",
            Registry::OnnxZoo => "onnx-zoo",
            Registry::PyTorchHub => "pytorch-hub",
            Registry::Local(_) => "local",
        }
    }
}

/// Model metadata từ registry
#[derive(Debug, Clone)]
pub struct ModelMetadata {
    pub id: String,
    pub registry: Registry,
    pub version: Option<String>,
    pub size_bytes: Option<u64>,
    pub format: Option<ModelFormat>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelFormat {
    SafeTensors,
    PyTorch,
    TensorFlow,
    Onnx,
    HuggingFace,
}

/// Query model từ registry
pub async fn query_model(registry: &Registry, model_id: &str) -> MgResult<ModelMetadata> {
    match registry {
        Registry::HuggingFace => query_huggingface(model_id).await,
        Registry::TensorFlowHub => query_tfhub(model_id).await,
        Registry::OnnxZoo => query_onnx(model_id).await,
        Registry::PyTorchHub => query_pytorch(model_id).await,
        Registry::Local(path) => query_local(path, model_id).await,
    }
}

async fn query_huggingface(model_id: &str) -> MgResult<ModelMetadata> {
    let client = crate::native::HuggingFaceClient::new();
    let json = client.get_json(&format!("/models/{model_id}")).await?;
    Ok(metadata_from_hf_json(&json, model_id))
}

/// Map JSON của /api/models/{id} → ModelMetadata — hàm thuần để test offline.
// (Map /api/models/{id} JSON → ModelMetadata — pure fn for offline tests.)
fn metadata_from_hf_json(json: &serde_json::Value, fallback_id: &str) -> ModelMetadata {
    ModelMetadata {
        id: json
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(fallback_id)
            .to_string(),
        registry: Registry::HuggingFace,
        // "sha" = commit revision trên Hub; thiếu → main branch
        // ("sha" is the Hub commit revision; absent → main branch)
        version: json
            .get("sha")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| Some("main".into())),
        size_bytes: None,
        format: Some(ModelFormat::HuggingFace),
        tags: json
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

async fn query_tfhub(model_id: &str) -> MgResult<ModelMetadata> {
    Ok(ModelMetadata {
        id: model_id.to_string(),
        registry: Registry::TensorFlowHub,
        version: Some("1".into()),
        size_bytes: None,
        format: Some(ModelFormat::TensorFlow),
        tags: vec![],
    })
}

async fn query_onnx(model_id: &str) -> MgResult<ModelMetadata> {
    Ok(ModelMetadata {
        id: model_id.to_string(),
        registry: Registry::OnnxZoo,
        version: None,
        size_bytes: None,
        format: Some(ModelFormat::Onnx),
        tags: vec![],
    })
}

async fn query_pytorch(model_id: &str) -> MgResult<ModelMetadata> {
    Ok(ModelMetadata {
        id: model_id.to_string(),
        registry: Registry::PyTorchHub,
        version: None,
        size_bytes: None,
        format: Some(ModelFormat::PyTorch),
        tags: vec![],
    })
}

async fn query_local(path: &Path, model_id: &str) -> MgResult<ModelMetadata> {
    let model_path = path.join(model_id);
    if !model_path.exists() {
        return Err(MgError::Other(format!(
            "Local model not found: {}",
            model_path.display()
        )));
    }

    let size_bytes = std::fs::metadata(&model_path).ok().map(|m| m.len());

    Ok(ModelMetadata {
        id: model_id.to_string(),
        registry: Registry::Local(path.to_path_buf()),
        version: None,
        size_bytes,
        format: detect_format(&model_path),
        tags: vec![],
    })
}

fn detect_format(path: &Path) -> Option<ModelFormat> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| match ext {
            "safetensors" => Some(ModelFormat::SafeTensors),
            "pt" | "pth" => Some(ModelFormat::PyTorch),
            "pb" | "h5" => Some(ModelFormat::TensorFlow),
            "onnx" => Some(ModelFormat::Onnx),
            _ => None,
        })
}

#[cfg(test)]
#[path = "test/registry_tests.rs"]
mod tests;
