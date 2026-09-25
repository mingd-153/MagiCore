#![cfg(test)]
#![allow(clippy::unwrap_used)]

use super::*;

#[tokio::test]
async fn package_install_fails_closed_for_every_game_engine() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    for engine in [
        GameEngine::Bevy,
        GameEngine::Godot,
        GameEngine::Unity,
        GameEngine::Unreal,
    ] {
        let error = install_dependencies(engine, tmp.path()).await.unwrap_err();
        assert!(matches!(error, MgError::Unsupported { core: "game", .. }));
        assert!(
            error
                .to_string()
                .contains("no provider package manager was invoked")
        );
    }
}
