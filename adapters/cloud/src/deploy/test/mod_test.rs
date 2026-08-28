#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

use tempfile::TempDir;

#[tokio::test]
async fn test_deploy_dry_run() {
    let tmp = TempDir::new().unwrap();
    let result = deploy(CloudType::Terraform, tmp.path(), true)
        .await
        .unwrap();
    assert!(result.dry_run);
    assert_eq!(result.duration_ms, 0);
}
