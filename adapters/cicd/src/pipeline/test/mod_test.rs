#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

use tempfile::TempDir;

#[tokio::test]
async fn test_generate_pipeline() {
    let tmp = TempDir::new().unwrap();
    generate_pipeline("test", tmp.path()).await.unwrap();
    assert!(tmp.path().join(".github/workflows/ci.yml").exists());
}
