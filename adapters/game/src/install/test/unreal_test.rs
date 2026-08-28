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
    async fn test_install_unreal() {
        let tmp = tmp();
        let (packages, bytes, verified) = install_dependencies(tmp.path()).await.unwrap();

        assert_eq!(packages.len(), 0);
        assert_eq!(bytes, 0);
        assert!(verified);
    }

    #[tokio::test]
    async fn test_download_unreal_stub() {
        let tmp = tmp();
        let binary = download_unreal_binary("5.4.0", tmp.path()).await.unwrap();

        assert!(binary.exists());
        assert!(binary.to_string_lossy().contains("UnrealEngine-5.4.0"));
    }
}
