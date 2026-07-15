/// HTTP response caching
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
