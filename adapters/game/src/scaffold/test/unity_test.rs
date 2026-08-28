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
    async fn test_scaffold_unity() {
        let tmp = tmp();
        let mut ctx = TemplateContext::new("my-unity-game", crate::engine::GameEngine::Unity);
        ctx.unity_version = Some("6000.0".to_string());

        scaffold(ctx, tmp.path()).await.unwrap();

        assert!(tmp.path().join("Packages/manifest.json").exists());
        assert!(tmp.path().join("Assets/Bootstrap.cs").exists());
        assert!(tmp.path().join("mgc.toml").exists());

        let bootstrap = std::fs::read_to_string(tmp.path().join("Assets/Bootstrap.cs")).unwrap();
        assert!(bootstrap.contains("my-unity-game"));
    }
}
