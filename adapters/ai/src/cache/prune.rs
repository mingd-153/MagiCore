//! Cache pruning logic.

use super::metadata::load_metadata;
use super::{cache_dir, model_cache_path};
use crate::registry::Registry;
use mgc_types::MgResult;
use std::path::PathBuf;

/// Prune strategy
#[derive(Debug, Clone, Copy)]
pub enum PruneStrategy {
    /// Remove models older than N days
    OlderThan(u64),
    /// Remove models not accessed in N days
    UnusedFor(u64),
    /// Keep only N most recent
    KeepRecent(usize),
    /// Remove models larger than N bytes
    LargerThan(u64),
}

/// Prune result
#[derive(Debug, Clone)]
pub struct PruneResult {
    pub removed_count: usize,
    pub bytes_freed: u64,
    pub removed_models: Vec<String>,
}

/// Prune cache theo strategy
pub fn prune_cache(registry: &Registry, strategy: PruneStrategy) -> MgResult<PruneResult> {
    let cache = cache_dir(registry)?;

    if !cache.exists() {
        return Ok(PruneResult {
            removed_count: 0,
            bytes_freed: 0,
            removed_models: vec![],
        });
    }

    let mut removed_count = 0;
    let mut bytes_freed = 0u64;
    let mut removed_models = Vec::new();

    let entries = std::fs::read_dir(&cache)?;

    for entry in entries.flatten() {
        let path = entry.path();

        if !path.is_dir() && !path.is_file() {
            continue;
        }

        // Load metadata
        let meta = load_metadata(&path).ok();

        let should_remove = match strategy {
            PruneStrategy::OlderThan(days) => {
                meta.as_ref().map(|m| m.age_days() > days).unwrap_or(false)
            }
            PruneStrategy::UnusedFor(days) => meta
                .as_ref()
                .map(|m| m.days_since_access() > days)
                .unwrap_or(false),
            PruneStrategy::LargerThan(bytes) => {
                meta.as_ref().map(|m| m.size_bytes > bytes).unwrap_or(false)
            }
            PruneStrategy::KeepRecent(_) => false, // Handled separately
        };

        if should_remove {
            if let Some(m) = &meta {
                bytes_freed += m.size_bytes;
                removed_models.push(m.model_id.clone());
            }

            if path.is_dir() {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }

            removed_count += 1;
        }
    }

    Ok(PruneResult {
        removed_count,
        bytes_freed,
        removed_models,
    })
}

/// Remove specific model from cache
pub fn remove_model(registry: &Registry, model_id: &str) -> MgResult<u64> {
    let path = model_cache_path(registry, model_id)?;

    if !path.exists() {
        return Ok(0);
    }

    let bytes = if path.is_dir() {
        let size = dir_size(&path);
        std::fs::remove_dir_all(&path)?;
        size
    } else {
        let size = std::fs::metadata(&path)?.len();
        std::fs::remove_file(&path)?;
        size
    };

    Ok(bytes)
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

#[cfg(test)]
#[path = "test/prune_tests.rs"]
mod tests;
