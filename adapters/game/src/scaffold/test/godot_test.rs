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
    async fn test_scaffold_godot() {
        let tmp = tmp();
        let ctx = TemplateContext::new("my-godot-game", crate::engine::GameEngine::Godot);

        scaffold(ctx, tmp.path()).await.unwrap();

        assert!(tmp.path().join("project.godot").exists());
        assert!(tmp.path().join("Main.tscn").exists());
        assert!(tmp.path().join("mgc.toml").exists());

        let project = std::fs::read_to_string(tmp.path().join("project.godot")).unwrap();
        assert!(project.contains("my-godot-game"));
    }
}
