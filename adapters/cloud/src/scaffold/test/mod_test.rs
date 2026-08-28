#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

use tempfile::TempDir;

#[tokio::test]
async fn test_scaffold_cdk() {
    let tmp = TempDir::new().unwrap();
    scaffold_project(CloudType::Cdk, "test", tmp.path())
        .await
        .unwrap();
    assert!(tmp.path().join("package.json").exists());
}
