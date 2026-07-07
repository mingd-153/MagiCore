use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::http::pool::{ConnectionPool, DownloadStats};

pub mod pool;

pub struct DownloadManager {
    client: reqwest::Client,
    semaphore: Arc<Semaphore>,
    connection_pool: Arc<ConnectionPool>,
    stats: Arc<DownloadStats>,
    concurrent_peak: Arc<AtomicUsize>,
    concurrent_current: Arc<AtomicUsize>,
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadManager {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(6)
            .pool_idle_timeout(Duration::from_secs(30))
            .tcp_keepalive(Duration::from_secs(60))
            .http2_keep_alive_interval(Duration::from_secs(10))
            .http2_keep_alive_timeout(Duration::from_secs(5))
            .tcp_nodelay(true)
            .user_agent(concat!("mg/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!("failed to build reqwest client: {e}, using default");
                reqwest::Client::new()
            });

        Self {
            client,
            semaphore: Arc::new(Semaphore::new(48)),
            connection_pool: Arc::new(ConnectionPool::new()),
            stats: Arc::new(DownloadStats::default()),
            concurrent_peak: Arc::new(AtomicUsize::new(0)),
            concurrent_current: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn stats(&self) -> DownloadStatsRef {
        DownloadStatsRef {
            total: self.stats.total.load(Ordering::Relaxed),
            concurrent_peak: self.concurrent_peak.load(Ordering::Relaxed),
            total_bytes: self.stats.total_bytes.load(Ordering::Relaxed),
            active_connections: self.connection_pool.active_connections(),
            pool_total: self.connection_pool.total_downloads(),
        }
    }

    pub async fn download_batch(
        &self,
        urls: &[DownloadRequest],
    ) -> Vec<Result<DownloadedPackage, DownloadError>> {
        let permit_count = urls.len().min(48);
        let _permit = self.semaphore.acquire_many(permit_count as u32).await.unwrap();

        let mut handles = Vec::with_capacity(urls.len());
        for req in urls {
            let client = self.client.clone();
            let stats = self.stats.clone();
            let peak = self.concurrent_peak.clone();
            let current = self.concurrent_current.clone();
            let pool = self.connection_pool.clone();
            let req = req.clone();

            let handle = tokio::spawn(async move {
                let host = req
                    .url
                    .split('/')
                    .nth(2)
                    .unwrap_or("unknown")
                    .to_string();

                let c = current.fetch_add(1, Ordering::Relaxed) + 1;
                let mut p = peak.load(Ordering::Relaxed);
                while c > p {
                    match peak.compare_exchange(p, c, Ordering::Relaxed, Ordering::Relaxed) {
                        Ok(_) => break,
                        Err(actual) => p = actual,
                    }
                }

                let _handle = pool.acquire(&host);
                let result = Self::do_download(&client, &req, &host, &pool).await;

                current.fetch_sub(1, Ordering::Relaxed);

                stats.total.fetch_add(1, Ordering::Relaxed);
                if let Ok(ref pkg) = result {
                    stats.total_bytes.fetch_add(pkg.size, Ordering::Relaxed);
                }

                result
            });
            handles.push(handle);
        }

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.await {
                Ok(r) => results.push(r),
                Err(e) => results.push(Err(DownloadError::NetworkError(e.to_string()))),
            }
        }
        results
    }

    async fn do_download(
        client: &reqwest::Client,
        req: &DownloadRequest,
        _host: &str,
        _pool: &ConnectionPool,
    ) -> Result<DownloadedPackage, DownloadError> {
        let start = std::time::Instant::now();

        let resp = tokio::time::timeout(
            Duration::from_secs(60),
            client.get(&req.url).header("Accept-Encoding", "gzip").send(),
        )
        .await
        .map_err(|_| DownloadError::Timeout(req.url.clone()))?
        .map_err(|e| DownloadError::NetworkError(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(DownloadError::HttpError(status.as_u16()));
        }

        let body = resp
            .bytes()
            .await
            .map_err(|e| DownloadError::NetworkError(e.to_string()))?;

        if let Some(expected) = &req.integrity {
            let actual = Sha256::digest(&body);
            let actual_hex = hex::encode(actual);
            if actual_hex != *expected {
                return Err(DownloadError::IntegrityMismatch {
                    expected: expected.clone(),
                    actual: actual_hex,
                });
            }
        }

        let duration = start.elapsed();
        Ok(DownloadedPackage {
            name: req.name.clone(),
            version: req.version.clone(),
            data: body.to_vec(),
            size: body.len(),
            duration,
        })
    }

    pub async fn download_one(
        &self,
        req: &DownloadRequest,
    ) -> Result<DownloadedPackage, DownloadError> {
        let _permit = self.semaphore.acquire_many(1).await.unwrap();
        let host = req
            .url
            .split('/')
            .nth(2)
            .unwrap_or("unknown")
            .to_string();
        let _handle = self.connection_pool.acquire(&host);
        Self::do_download(&self.client, req, &host, &self.connection_pool).await
    }
}

pub struct DownloadStatsRef {
    pub total: usize,
    pub concurrent_peak: usize,
    pub total_bytes: usize,
    pub active_connections: usize,
    pub pool_total: usize,
}

pub struct DownloadScheduler {
    max_concurrent: usize,
    per_host_limit: usize,
}

impl Default for DownloadScheduler {
    fn default() -> Self {
        Self {
            max_concurrent: 48,
            per_host_limit: 6,
        }
    }
}

impl DownloadScheduler {
    pub fn schedule(&self, requests: &[DownloadRequest]) -> Vec<Vec<DownloadRequest>> {
        let mut by_host: std::collections::HashMap<&str, Vec<&DownloadRequest>> =
            std::collections::HashMap::new();
        for req in requests {
            let host = req.host();
            by_host.entry(host).or_default().push(req);
        }

        let mut consumed: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        let mut batches = Vec::new();

        loop {
            let remaining: usize = by_host
                .iter()
                .map(|(h, v)| v.len().saturating_sub(consumed.get(h).copied().unwrap_or(0)))
                .sum();
            if remaining == 0 {
                break;
            }

            let mut batch = Vec::new();
            for (host, reqs) in &by_host {
                let c = consumed.entry(host).or_insert(0);
                for _ in 0..self.per_host_limit {
                    if batch.len() >= self.max_concurrent {
                        break;
                    }
                    if *c >= reqs.len() {
                        break;
                    }
                    if let Some(req) = reqs.get(*c) {
                        batch.push((*req).clone());
                        *c += 1;
                    }
                }
                if batch.len() >= self.max_concurrent {
                    break;
                }
            }

            if !batch.is_empty() {
                batches.push(batch);
            }
        }

        batches
    }
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub name: String,
    pub version: String,
    pub url: String,
    pub integrity: Option<String>,
}

impl DownloadRequest {
    pub fn host(&self) -> &str {
        self.url.split('/').nth(2).unwrap_or("unknown")
    }
}

#[derive(Debug, Clone)]
pub struct DownloadedPackage {
    pub name: String,
    pub version: String,
    pub data: Vec<u8>,
    pub size: usize,
    pub duration: std::time::Duration,
}

#[derive(Debug)]
pub enum DownloadError {
    HttpError(u16),
    NetworkError(String),
    Timeout(String),
    IntegrityMismatch { expected: String, actual: String },
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HttpError(code) => write!(f, "HTTP error: {}", code),
            Self::NetworkError(msg) => write!(f, "network error: {}", msg),
            Self::Timeout(url) => write!(f, "timeout: {}", url),
            Self::IntegrityMismatch { expected, actual } => {
                write!(f, "integrity mismatch: expected {}, got {}", expected, actual)
            }
        }
    }
}

impl std::error::Error for DownloadError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_request_host() {
        let req = DownloadRequest {
            name: "test".into(),
            version: "1.0".into(),
            url: "https://registry.npmjs.org/test".into(),
            integrity: None,
        };
        assert_eq!(req.host(), "registry.npmjs.org");
    }

    #[test]
    fn test_download_scheduler_empty() {
        let scheduler = DownloadScheduler::default();
        let batches = scheduler.schedule(&[]);
        assert!(batches.is_empty());
    }

    #[test]
    fn test_download_scheduler_single() {
        let scheduler = DownloadScheduler::default();
        let reqs = vec![DownloadRequest {
            name: "pkg".into(),
            version: "1.0".into(),
            url: "https://example.com/pkg".into(),
            integrity: None,
        }];
        let batches = scheduler.schedule(&reqs);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 1);
    }

    #[test]
    fn test_download_scheduler_per_host_limit() {
        let scheduler = DownloadScheduler::default();
        let reqs: Vec<DownloadRequest> = (0..10)
            .map(|i| DownloadRequest {
                name: format!("pkg{}", i),
                version: "1.0".into(),
                url: "https://example.com/pkg".into(),
                integrity: None,
            })
            .collect();
        let batches = scheduler.schedule(&reqs);
        let total: usize = batches.iter().map(|b| b.len()).sum();
        assert_eq!(total, 10);
        for batch in &batches {
            assert!(batch.len() <= scheduler.per_host_limit);
        }
    }

    #[test]
    fn test_download_scheduler_multi_host() {
        let scheduler = DownloadScheduler::default();
        let reqs = vec![
            DownloadRequest {
                name: "a".into(),
                version: "1.0".into(),
                url: "https://host1.com/a".into(),
                integrity: None,
            },
            DownloadRequest {
                name: "b".into(),
                version: "1.0".into(),
                url: "https://host2.com/b".into(),
                integrity: None,
            },
        ];
        let batches = scheduler.schedule(&reqs);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 2);
    }

    #[test]
    fn test_download_manager_default() {
        let dm = DownloadManager::new();
        let stats = dm.stats();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.concurrent_peak, 0);
    }
}
