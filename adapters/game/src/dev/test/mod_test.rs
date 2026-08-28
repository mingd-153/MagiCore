#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

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
