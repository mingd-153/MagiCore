/// HTTP response caching
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// Cache entry
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub data: Vec<u8>,
    pub timestamp: SystemTime,
    pub ttl: Duration,
}

impl CacheEntry {
    pub fn is_valid(&self) -> bool {
        SystemTime::now()
            .duration_since(self.timestamp)
            .map(|elapsed| elapsed < self.ttl)
            .unwrap_or(false)
    }
}

/// HTTP cache with TTL support
#[derive(Debug, Clone)]
pub struct HttpCache {
    cache: Arc<Mutex<HashMap<String, CacheEntry>>>,
}

impl HttpCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        let cache = self.cache.lock().expect("lock poisoned");
        cache.get(key).and_then(|entry| {
            if entry.is_valid() {
                Some(entry.data.clone())
            } else {
                None
            }
        })
    }

    pub fn insert(&self, key: String, data: Vec<u8>, ttl: Duration) {
        let mut cache = self.cache.lock().expect("lock poisoned");
        cache.insert(
            key,
            CacheEntry {
                data,
                timestamp: SystemTime::now(),
                ttl,
            },
        );
    }
}

impl Default for HttpCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "test/cache_test.rs"]
mod tests;
