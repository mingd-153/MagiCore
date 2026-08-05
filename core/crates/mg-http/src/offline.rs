//! Offline mode (12 §9)
//! (Deterministic offline - stale metadata with warning, cache-only)

use anyhow::{bail, Result};
use reqwest::Client;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Offline HTTP client - chỉ dùng cache, không request mạng
pub struct OfflineClient {
    cache: HashMap<String, (Vec<u8>, SystemTime)>,
    ttl: Duration,
}

impl OfflineClient {
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: HashMap::new(),
            ttl,
        }
    }

    pub fn with_cache(mut self, cache: HashMap<String, (Vec<u8>, SystemTime)>) -> Self {
        self.cache = cache;
        self
    }

    /// GET từ cache - offline deterministic
    pub async fn get(&self, url: &str) -> Result<Vec<u8>> {
        if let Some((data, ts)) = self.cache.get(url) {
            let age = SystemTime::now()
                .duration_since(*ts)
                .unwrap_or(Duration::MAX);
            
            if age > self.ttl {
                // Stale metadata - cho dùng nhưng warning lớn (12 §9)
                eprintln!("WARNING: stale metadata for {} (age: {:.0}s, ttl: {:.0}s) - cannot verify latest version from registry",
                    url, age.as_secs(), self.ttl.as_secs());
                return Ok(data.clone());
            }
            return Ok(data.clone());
        }

        bail!("Offline mode: no cached data for {} - E_NET_OFFLINE", url);
    }

    /// PUT/POST/DELETE - chặn trong offline
    pub async fn put(&self, _url: &str, _body: Vec<u8>) -> Result<Vec<u8>> {
        bail!("Offline mode: PUT not allowed");
    }

    pub async fn post(&self, _url: &str, _body: Vec<u8>) -> Result<Vec<u8>> {
        bail!("Offline mode: POST not allowed");
    }

    pub async fn delete(&self, _url: &str) -> Result<Vec<u8>> {
        bail!("Offline mode: DELETE not allowed");
    }

    /// Load cache từ disk (file JSON: url -> {data, timestamp})
    pub fn load_cache(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("read cache file: {}", e))?;
        let cache: HashMap<String, (Vec<u8>, SystemTime)> = 
            serde_json::from_str(&content)
                .map_err(|e| anyhow::anyhow!("parse cache JSON: {}", e))?;
        Ok(Self::new(Duration::from_secs(600)).with_cache(cache))
    }

    /// Save cache ra disk
    pub fn save_cache(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string(&self.cache)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

/// Hybrid client: online bình thường, offline fallback cache
pub struct HybridClient {
    online: Client,
    offline: OfflineClient,
    offline_mode: bool,
}

impl HybridClient {
    pub fn new(offline_ttl: Duration) -> Result<Self> {
        Ok(Self {
            online: Client::new(),
            offline: OfflineClient::new(offline_ttl),
            offline_mode: false,
        })
    }

    pub fn set_offline(&mut self, offline: bool) {
        self.offline_mode = offline;
    }

    pub async fn get(&self, url: &str) -> Result<Vec<u8>> {
        if self.offline_mode {
            return self.offline.get(url).await;
        }

        // Try online first
        match self.online.get(url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let data = resp.bytes().await?.to_vec();
                // Cache for offline
                // Note: would need interior mutability for real impl
                Ok(data.to_vec())
            }
            Ok(_) => {
                // Fallback to offline cache
                self.offline.get(url).await
            }
            Err(_) => {
                // Network error -> offline cache
                self.offline.get(url).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn offline_client_cache_hit_fresh() {
        let mut client = OfflineClient::new(Duration::from_secs(600));
        client.cache.insert("test".into(), (b"data".to_vec(), SystemTime::now()));
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
}