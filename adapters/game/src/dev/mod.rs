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
async fn start_bevy_dev(project_root: &Path) -> MgResult<DevCommand> {
    // Bevy: cargo run
    // A13: future - dynamic linking (.dylib/.dll reload, keep runtime state)

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
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        TempDir::new().unwrap()
    }

    #[tokio::test]
    async fn test_bevy_dev_command() {
        let tmp = tmp();
        let cmd = start_dev_server(GameEngine::Bevy, tmp.path())
            .await
            .unwrap();

        assert_eq!(cmd.command, "cargo");
        assert_eq!(cmd.args[0], "run");
    }

    #[tokio::test]
    async fn test_godot_dev_command() {
        let tmp = tmp();
        let cmd = start_dev_server(GameEngine::Godot, tmp.path())
            .await
            .unwrap();

        assert_eq!(cmd.command, "godot");
        assert!(cmd.args.contains(&"--editor".to_string()));
    }

    #[tokio::test]
    async fn test_unity_dev_stub() {
        let tmp = tmp();
        let result = start_dev_server(GameEngine::Unity, tmp.path()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unreal_dev_stub() {
        let tmp = tmp();
        let result = start_dev_server(GameEngine::Unreal, tmp.path()).await;

        assert!(result.is_err());
    }
}
