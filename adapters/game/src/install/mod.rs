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
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        TempDir::new().unwrap()
    }

    #[tokio::test]
    async fn test_install_bevy_stub() {
        let tmp = tmp();
        // Create Cargo.toml
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname=\"test\"\nversion=\"0.1.0\"\n\n[dependencies]\nbevy=\"0.14\"\n",
        )
        .unwrap();

        let summary = install_dependencies(GameEngine::Bevy, tmp.path())
            .await
            .unwrap();
        assert_eq!(summary.engine, GameEngine::Bevy);
    }
}
