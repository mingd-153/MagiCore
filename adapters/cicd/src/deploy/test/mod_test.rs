#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

#[tokio::test]
async fn test_deploy_dry_run() {
    let result = deploy(DeployTarget::Aws, true).await.unwrap();
    assert!(result.contains("dry_run: true"));
}
