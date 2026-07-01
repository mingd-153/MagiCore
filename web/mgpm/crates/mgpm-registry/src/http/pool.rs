use std::collections::HashMap;
use std::time::{Duration, Instant};
use parking_lot::Mutex;

struct PooledConnection {
    host: String,
    _created_at: Instant,
    last_used: Instant,
    id: u64,
}

pub struct ConnectionPool {
    inner: Mutex<InnerPool>,
    config: PoolConfig,
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

    pub fn with_config(config: PoolConfig) -> Self {
        Self {
            inner: Mutex::new(InnerPool {
                connections: Vec::new(),
                by_host: HashMap::new(),
                next_id: 0,
            }),
            config,
        }
    }

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
            _created_at: Instant::now(),
            last_used: Instant::now(),
            id,
        });

        let idx = inner.connections.len() - 1;
        inner.by_host.entry(host.to_string())
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
        let to_remove: Vec<u64> = inner.connections
            .iter()
            .filter(|c| now.duration_since(c.last_used) >= self.config.idle_timeout)
            .map(|c| c.id)
            .collect();

        for id in &to_remove {
            if let Some(pos) = inner.connections.iter().position(|c| c.id == *id) {
                let conn = inner.connections.remove(pos);
                if let Some(host_list) = inner.by_host.get_mut(&conn.host) {
                    host_list.retain(|&x| x != conn.id as usize);
                    if host_list.is_empty() {
                        inner.by_host.remove(&conn.host);
                    }
                }
            }
        }
    }

    pub fn stats(&self) -> PoolStats {
        let inner = self.inner.lock();
        PoolStats {
            active: inner.connections.len(),
            by_host: inner.by_host.iter().map(|(k, v)| (k.clone(), v.len())).collect(),
        }
    }
}

impl Default for ConnectionPool {
    fn default() -> Self {
        Self::new()
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
