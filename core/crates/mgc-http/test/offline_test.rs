#![allow(clippy::unwrap_used)]
//! Tests for offline HTTP client functionality

use mgc_http::offline::OfflineClient;
use std::time::{Duration, SystemTime};

#[test]
fn offline_client_cache_hit_fresh() {
    let mut client = OfflineClient::new(Duration::from_secs(600));
    client
        .cache
        .insert("test".into(), (b"data".to_vec(), SystemTime::now()));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let data = rt.block_on(client.get("test")).unwrap();
    assert_eq!(data, b"data");
}

#[test]
fn offline_client_stale_warning() {
    let mut client = OfflineClient::new(Duration::from_secs(1));
    let past = SystemTime::now() - Duration::from_secs(10);
    client.cache.insert("test".into(), (b"data".to_vec(), past));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let data = rt.block_on(client.get("test")).unwrap();
    assert_eq!(data, b"data"); // still returns data but prints warning
}

#[test]
fn offline_client_miss_fails() {
    let client = OfflineClient::new(Duration::from_secs(600));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(client.get("missing"));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("E_NET_OFFLINE"));
}
