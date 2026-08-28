//! Game engine installation for game adapter.
//! Bevy (cargo orchestrate), Godot (binary), Unity (UPM), Unreal (stub).

use crate::engine::GameEngine;
use mgc_types::MgResult;
use std::path::Path;

pub mod bevy;
pub mod godot;
pub mod unity;
pub mod unreal;

/// Install summary per engine
#[derive(Debug, Clone)]
pub struct InstallSummary {
    pub engine: GameEngine,
    pub installed_packages: Vec<String>,
    pub bytes_downloaded: u64,
    pub duration_ms: u64,
    pub verified: bool,
}

/// Install dependencies for game project
pub async fn install_dependencies(
    engine: GameEngine,
    project_root: &Path,
) -> MgResult<InstallSummary> {
    let start = std::time::Instant::now();

    let (packages, bytes, verified) = match engine {
        GameEngine::Bevy => bevy::install_dependencies(project_root).await?,
        GameEngine::Godot => godot::install_dependencies(project_root).await?,
        GameEngine::Unity => unity::install_dependencies(project_root).await?,
        GameEngine::Unreal => unreal::install_dependencies(project_root).await?,
    };

    let duration_ms = start.elapsed().as_millis() as u64;

    Ok(InstallSummary {
        engine,
        installed_packages: packages,
        bytes_downloaded: bytes,
        duration_ms,
        verified,
    })
}


#[cfg(test)]
#[path = "test/mod_test.rs"]
mod tests;
