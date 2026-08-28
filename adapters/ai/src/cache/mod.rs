//! Model cache management cho AI adapter.
//! Hỗ trợ HuggingFace cache, Ollama cache, local cache.

use mgc_types::{MgError, MgResult};
use std::path::PathBuf;

pub mod metadata;
pub mod prune;

use crate::registry::Registry;

/// Cache location cho từng registry
pub fn cache_dir(registry: &Registry) -> MgResult<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| MgError::Other("Cannot determine home directory".into()))?;

    let cache_path = match registry {
        Registry::HuggingFace => home.join(".cache").join("huggingface").join("hub"),
        Registry::PyTorchHub => home.join(".cache").join("torch").join("hub"),
        Registry::TensorFlowHub => home.join(".cache").join("tfhub"),
        Registry::OnnxZoo => home.join(".cache").join("onnx"),
        Registry::Local(path) => path.clone(),
    };

    Ok(cache_path)
}

/// Get cache path cho specific model
pub fn model_cache_path(registry: &Registry, model_id: &str) -> MgResult<PathBuf> {
    let base = cache_dir(registry)?;

    // Sanitize model_id for filesystem
    let safe_id = model_id.replace('/', "--");

    Ok(base.join(safe_id))
}

/// Check if model exists in cache
pub fn is_cached(registry: &Registry, model_id: &str) -> MgResult<bool> {
    let path = model_cache_path(registry, model_id)?;
    Ok(path.exists())
}

/// List all cached models for registry
pub fn list_cached_models(registry: &Registry) -> MgResult<Vec<String>> {
    let cache = cache_dir(registry)?;

    if !cache.exists() {
        return Ok(vec![]);
    }

    let mut models = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&cache) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                // Restore "/" from "--"
                let model_id = name.replace("--", "/");
                models.push(model_id);
            }
        }
    }

    Ok(models)
}

/// Get total cache size in bytes
pub fn cache_size(registry: &Registry) -> MgResult<u64> {
    let cache = cache_dir(registry)?;

    if !cache.exists() {
        return Ok(0);
    }

    fn dir_size(path: &PathBuf) -> u64 {
        let mut size = 0u64;
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        size += metadata.len();
                    } else if metadata.is_dir() {
                        size += dir_size(&entry.path());
                    }
                }
            }
        }
        size
    }

    Ok(dir_size(&cache))
}

#[cfg(test)]
#[path = "test/cache_tests.rs"]
mod tests;
