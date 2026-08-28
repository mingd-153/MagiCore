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
    async fn test_scaffold_esp32() {
        let tmp = tmp();
        scaffold_project(IotFramework::Esp32Rust, "test", "esp32c3", tmp.path())
            .await
            .unwrap();
        assert!(tmp.path().join("Cargo.toml").exists());
        assert!(tmp.path().join("mgc.toml").exists());
    }

    #[tokio::test]
    async fn test_scaffold_platformio() {
        let tmp = tmp();
        scaffold_project(IotFramework::Platformio, "test", "esp32dev", tmp.path())
            .await
            .unwrap();
        assert!(tmp.path().join("platformio.ini").exists());
    }
}
