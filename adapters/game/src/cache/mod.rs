//! Engine binaries cache management.
//! Cache Godot/Unity binaries in ~/.cache/mgc/game/

use crate::engine::GameEngine;
use mgc_types::{MgError, MgResult};
use std::path::PathBuf;

/// Get cache directory for game engines
pub fn cache_dir(engine: GameEngine) -> MgResult<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| MgError::Other("Cannot determine home directory".into()))?;

    let cache_path = home
        .join(".cache")
        .join("mgc")
        .join("game")
        .join(engine.as_str());

    Ok(cache_path)
}

/// Get cached binary path for specific engine version
pub fn cached_binary_path(engine: GameEngine, version: &str) -> MgResult<PathBuf> {
    let cache = cache_dir(engine)?;

    let binary_name = match engine {
        GameEngine::Godot => format!("godot-{}", version),
        GameEngine::Unity => format!("unity-{}", version),
        GameEngine::Unreal => format!("unreal-{}", version),
        GameEngine::Bevy => return Err(MgError::Other("Bevy uses Cargo - no binary cache".into())),
    };

    Ok(cache.join(binary_name))
}

/// Check if engine binary is cached
pub fn is_cached(engine: GameEngine, version: &str) -> MgResult<bool> {
    if matches!(engine, GameEngine::Bevy) {
        // Bevy = Cargo dep, không cache binary
        return Ok(false);
    }

    let path = cached_binary_path(engine, version)?;
    Ok(path.exists())
}

/// List cached versions for engine
pub fn list_cached_versions(engine: GameEngine) -> MgResult<Vec<String>> {
    let cache = cache_dir(engine)?;

    if !cache.exists() {
        return Ok(vec![]);
    }

    let mut versions = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&cache) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                // Extract version from "godot-4.3.0" → "4.3.0"
                if let Some(version) = name.split('-').nth(1) {
                    versions.push(version.to_string());
                }
            }
        }
    }

    Ok(versions)
}

/// Remove cached binary
pub fn remove_cached_binary(engine: GameEngine, version: &str) -> MgResult<()> {
    let path = cached_binary_path(engine, version)?;

    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    Ok(())
}


#[cfg(test)]
#[path = "../test/mod_test.rs"]
mod tests;
