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
    async fn test_scaffold_unreal() {
        let tmp = tmp();
        let mut ctx = TemplateContext::new("my-unreal-game", crate::engine::GameEngine::Unreal);
        ctx.unreal_version = Some("5.4".to_string());

        scaffold(ctx, tmp.path()).await.unwrap();

        assert!(tmp.path().join("my-unreal-game.uproject").exists());
        assert!(tmp.path().join("mgc.toml").exists());
        assert!(tmp
            .path()
            .join("Source/my-unreal-game/my-unreal-game.Build.cs")
            .exists());
    }
}
