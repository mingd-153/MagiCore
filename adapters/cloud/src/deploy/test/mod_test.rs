#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

use tempfile::TempDir;

#[tokio::test]
async fn deploy_is_not_claimed_until_a_native_provider_engine_exists() {
    let tmp = TempDir::new().unwrap();
    let error = deploy(CloudType::Terraform, tmp.path(), true)
        .await
        .expect_err("dry-run must not report a fake native plan");
    assert!(matches!(error, MgError::Unsupported { .. }));
}
