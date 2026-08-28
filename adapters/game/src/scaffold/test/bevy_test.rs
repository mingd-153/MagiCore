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
    async fn test_scaffold_bevy() {
        let tmp = tmp();
        let ctx = TemplateContext::new("my-bevy-game", crate::engine::GameEngine::Bevy);

        scaffold(ctx, tmp.path()).await.unwrap();

        assert!(tmp.path().join("Cargo.toml").exists());
        assert!(tmp.path().join("src/main.rs").exists());
        assert!(tmp.path().join("mgc.toml").exists());

        let cargo = std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
        assert!(cargo.contains("my-bevy-game"));
        assert!(cargo.contains("bevy"));
    }
}
