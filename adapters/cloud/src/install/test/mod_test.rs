#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_install_cdk() {
        let tmp = TempDir::new().unwrap();
        let deps = install_dependencies(CloudType::Cdk, tmp.path())
            .await
            .unwrap();
        assert!(!deps.is_empty());
    }
}
