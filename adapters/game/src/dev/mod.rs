//! Development server for game engines.
//! Bevy (cargo run), Godot (editor), Unity/Unreal (stub P1).

use crate::engine::GameEngine;
use mgc_types::{MgError, MgResult};
use std::path::Path;

/// Dev server command per engine
#[derive(Debug, Clone)]
pub struct DevCommand {
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: String,
}

/// Start dev server for game project
pub async fn start_dev_server(engine: GameEngine, project_root: &Path) -> MgResult<DevCommand> {
    match engine {
        GameEngine::Bevy => start_bevy_dev(project_root).await,
        GameEngine::Godot => start_godot_dev(project_root).await,
        GameEngine::Unity => start_unity_dev(project_root).await,
        GameEngine::Unreal => start_unreal_dev(project_root).await,
    }
}

/// Start Bevy dev server (cargo run)
/// P1: Standard cargo run (full rebuild on change)
/// P2 (A13 - deferred): Dynamic linking hot reload
/// Requires: (1) Bevy dynamic_linking feature, (2) game compiled as dylib,
///           (3) file watcher, (4) dylib reload API, (5) state preservation
/// Rationale: cargo run sufficient for P1; hot reload is nice-to-have optimization,
///            not security/correctness critical. Adds ~400-500 lines + platform complexity.
async fn start_bevy_dev(project_root: &Path) -> MgResult<DevCommand> {

    Ok(DevCommand {
        command: "cargo".to_string(),
        args: vec!["run".to_string()],
        working_dir: project_root.to_string_lossy().to_string(),
    })
}

/// Start Godot editor
async fn start_godot_dev(project_root: &Path) -> MgResult<DevCommand> {
    // Godot: godot --editor -p .

    Ok(DevCommand {
        command: "godot".to_string(),
        args: vec!["--editor".to_string(), "-p".to_string(), ".".to_string()],
        working_dir: project_root.to_string_lossy().to_string(),
    })
}

/// Start Unity editor (stub P1)
async fn start_unity_dev(_project_root: &Path) -> MgResult<DevCommand> {
    // Unity: mở Unity Hub (platform-specific)
    // P1: stub, P2: actual Unity Hub integration

    Err(MgError::Other(
        "Unity dev server P2 - use Unity Hub manually".into(),
    ))
}

/// Start Unreal editor (stub P1)
async fn start_unreal_dev(_project_root: &Path) -> MgResult<DevCommand> {
    // Unreal: P2

    Err(MgError::Other(
        "Unreal dev server P2 - use Unreal Editor manually".into(),
    ))
}


#[cfg(test)]
#[path = "test/mod_test.rs"]
mod tests;
