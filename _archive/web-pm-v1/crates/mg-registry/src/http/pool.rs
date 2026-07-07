use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

struct PooledConnection {
    host: String,
    #[allow(dead_code)]
    created_at: Instant,
    last_used: Instant,
    id: u64,
}

pub struct ConnectionPool {
    inner: Mutex<InnerPool>,
    pub config: PoolConfig,
}

struct InnerPool {
    connections: Vec<PooledConnection>,
    by_host: HashMap<String, Vec<usize>>,
    next_id: u64,
}

pub struct PoolConfig {
    pub max_per_host: usize,
    pub max_total: usize,
    pub idle_timeout: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_per_host: 6,
            max_total: 48,
            idle_timeout: Duration::from_secs(30),
        }
    }
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(InnerPool {
                connections: Vec::new(),
                by_host: HashMap::new(),
                next_id: 0,
            }),
            config: PoolConfig::default(),
        }
    }
}

impl Default for ConnectionPool {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionPool {

    pub fn can_connect(&self, host: &str) -> bool {
        let inner = self.inner.lock();
        let host_count = inner.by_host.get(host).map_or(0, |v| v.len());
        host_count < self.config.max_per_host
            && inner.connections.len() < self.config.max_total
    }

    pub fn acquire(&self, host: &str) -> ConnectionHandle {
        let mut inner = self.inner.lock();
        let id = inner.next_id;
        inner.next_id += 1;

        inner.connections.push(PooledConnection {
            host: host.to_string(),
            created_at: Instant::now(),
            last_used: Instant::now(),
            id,
        });

        let idx = inner.connections.len() - 1;
        inner
            .by_host
            .entry(host.to_string())
            .or_default()
            .push(idx);

        ConnectionHandle { id }
    }

    pub fn release(&self, handle: ConnectionHandle) {
        let mut inner = self.inner.lock();
        if let Some(conn) = inner.connections.iter_mut().find(|c| c.id == handle.id) {
            conn.last_used = Instant::now();
        }
    }

    pub fn cleanup(&self) {
        let mut inner = self.inner.lock();
        let now = Instant::now();
        let mut to_remove = Vec::new();
        for (i, conn) in inner.connections.iter().enumerate() {
            if now.duration_since(conn.last_used) >= self.config.idle_timeout {
                to_remove.push(i);
            }
        }
        for &i in to_remove.iter().rev() {
            let host = inner.connections[i].host.clone();
            inner.by_host.entry(host).and_modify(|v| v.retain(|&x| x != i));
            inner.connections.swap_remove(i);
        }
    }

    pub fn stats(&self) -> PoolStats {
        let inner = self.inner.lock();
        PoolStats {
            active: inner.connections.len(),
            by_host: inner
                .by_host
                .iter()
                .map(|(k, v)| (k.clone(), v.len()))
                .collect(),
        }
    }

    pub fn active_connections(&self) -> usize {
        self.inner.lock().connections.len()
    }

    pub fn total_downloads(&self) -> usize {
        // approximate — tracks highest connection id
        self.inner.lock().next_id as usize
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionHandle {
    id: u64,
}

#[derive(Debug, Clone)]
pub struct PoolStats {
    pub active: usize,
    pub by_host: HashMap<String, usize>,
}

unsafe impl Send for ConnectionHandle {}
unsafe impl Sync for ConnectionHandle {}

pub struct DownloadStats {
    pub total: AtomicUsize,
    pub concurrent_peak: AtomicUsize,
    pub total_bytes: AtomicUsize,
}

impl Default for DownloadStats {
    fn default() -> Self {
        Self {
            total: AtomicUsize::new(0),
            concurrent_peak: AtomicUsize::new(0),
            total_bytes: AtomicUsize::new(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_connection_pool_basic() {
        let pool = ConnectionPool::new();
        assert!(pool.can_connect("example.com"));
        let handle = pool.acquire("example.com");
        pool.release(handle);
        assert_eq!(pool.stats().active, 1);
    }

    #[test]
    fn test_pool_config_limits() {
        let pool = ConnectionPool::new();
        let host = "test.com";
        for _ in 0..6 {
            assert!(pool.can_connect(host));
            pool.acquire(host);
        }
        assert!(!pool.can_connect(host));
    }

    #[test]
    fn test_pool_cleanup() {
        let mut pool = ConnectionPool::new();
        let handle = pool.acquire("cleanup-test");
        pool.release(handle);
        assert!(pool.stats().active > 0);
        pool.config.idle_timeout = Duration::from_nanos(0);
        std::thread::sleep(Duration::from_millis(1));
        pool.cleanup();
        assert_eq!(pool.stats().active, 0);
    }

    #[test]
    fn test_download_stats() {
        let stats = DownloadStats::default();
        stats.total.fetch_add(5, Ordering::Relaxed);
        assert_eq!(stats.total.load(Ordering::Relaxed), 5);
    }
}
