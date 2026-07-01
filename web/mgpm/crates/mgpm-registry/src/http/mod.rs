pub mod pool;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use pool::ConnectionPool;

const MAX_CONCURRENT: usize = 48;
const PER_HOST_LIMIT: usize = 6;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub name: String,
    pub version: String,
    pub url: String,
    pub integrity: Option<String>,
}

impl DownloadRequest {
    /// Extract host from URL for connection pool tracking.
    /// Security: use `validate_url()` before download to ensure safe scheme.
    pub fn host(&self) -> &str {
        self.url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("unknown")
    }

    pub fn validate_url(&self) -> Result<(), DownloadError> {
        let parsed = url::Url::parse(&self.url)
            .map_err(|_| DownloadError::NetworkError(format!("invalid URL: {}", self.url)))?;
        match parsed.scheme() {
            "https" | "http" => Ok(()),
            scheme => Err(DownloadError::NetworkError(
                format!("unsupported URL scheme: {} (only http/https allowed)", scheme)
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadedPackage {
    pub name: String,
    pub version: String,
    pub data: Vec<u8>,
    pub size: usize,
    pub duration: Duration,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum DownloadError {
    #[error("HTTP error: {0}")]
    HttpError(u16),
    #[error("network error: {0}")]
    NetworkError(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("integrity mismatch: expected {expected}, got {actual}")]
    IntegrityMismatch { expected: String, actual: String },
    #[error("partial download: expected {expected} bytes, got {actual}")]
    Truncated { expected: u64, actual: u64 },
}

impl From<reqwest::Error> for DownloadError {
    fn from(e: reqwest::Error) -> Self {
        Self::NetworkError(e.to_string())
    }
}

pub struct DownloadScheduler {
    max_concurrent: usize,
    per_host_limit: usize,
}

impl Default for DownloadScheduler {
    fn default() -> Self {
        Self {
            max_concurrent: MAX_CONCURRENT,
            per_host_limit: PER_HOST_LIMIT,
        }
    }
}

impl DownloadScheduler {
    pub fn new(max_concurrent: usize, per_host_limit: usize) -> Self {
        Self { max_concurrent, per_host_limit }
    }

    pub fn schedule(&self, requests: &[DownloadRequest]) -> Vec<Vec<DownloadRequest>> {
        let mut by_host: std::collections::HashMap<&str, Vec<&DownloadRequest>> = std::collections::HashMap::new();
        for req in requests {
            let host = req.host();
            by_host.entry(host).or_default().push(req);
        }

        let mut batches = Vec::new();
        let mut current_batch = Vec::new();
        let mut host_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();

        loop {
            let mut added = false;
            for (host, reqs) in &by_host {
                let count = host_counts.entry(host).or_insert(0);
                if *count < self.per_host_limit
                    && current_batch.len() < self.max_concurrent
                    && *count < reqs.len()
                {
                    if let Some(req) = reqs.get(*count) {
                        current_batch.push((*req).clone());
                        *count += 1;
                        added = true;
                    }
                }
            }

            if current_batch.len() >= self.max_concurrent || !added {
                if !current_batch.is_empty() {
                    batches.push(std::mem::take(&mut current_batch));
                }
                if !added {
                    break;
                }
            }
        }

        batches
    }
}

pub struct DownloadManager {
    client: reqwest::Client,
    semaphore: Arc<Semaphore>,
    connection_pool: ConnectionPool,
}

impl DownloadManager {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(PER_HOST_LIMIT)
            .pool_idle_timeout(Duration::from_secs(30))
            .tcp_keepalive(Duration::from_secs(60))
            .tcp_nodelay(true)
            .user_agent(concat!("mgpm/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT)),
            connection_pool: ConnectionPool::new(),
        }
    }

    pub fn with_client(client: reqwest::Client, max_concurrent: usize) -> Self {
        Self {
            client,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            connection_pool: ConnectionPool::new(),
        }
    }

    pub fn connection_pool(&self) -> &ConnectionPool {
        &self.connection_pool
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    pub async fn download_to_file(
        &self,
        url: &str,
        output_path: &Path,
    ) -> Result<(u64, [u8; 32]), DownloadError> {
        DownloadRequest {
            name: String::new(),
            version: String::new(),
            url: url.to_string(),
            integrity: None,
        }.validate_url()?;
        let _permit = self.semaphore.acquire().await.map_err(|_| DownloadError::NetworkError("semaphore closed".into()))?;

        let response = tokio::time::timeout(DOWNLOAD_TIMEOUT, self.client.get(url).send())
            .await
            .map_err(|_| DownloadError::Timeout(url.to_string()))?
            .map_err(|e| DownloadError::NetworkError(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            return Err(DownloadError::HttpError(status.as_u16()));
        }

        let expected_len = response.content_length();
        let mut file = tokio::fs::File::create(output_path).await
            .map_err(|e| DownloadError::NetworkError(e.to_string()))?;
        let mut stream = response.bytes_stream();
        let mut hasher = Sha256::new();
        let mut downloaded = 0u64;

        while let Some(chunk) = futures_util::StreamExt::next(&mut stream).await {
            let chunk = chunk.map_err(|e| DownloadError::NetworkError(e.to_string()))?;
            hasher.update(&chunk);
            file.write_all(&chunk).await
                .map_err(|e| DownloadError::NetworkError(e.to_string()))?;
            downloaded += chunk.len() as u64;
        }

        file.flush().await
            .map_err(|e| DownloadError::NetworkError(e.to_string()))?;
        drop(_permit);

        if let Some(expected) = expected_len {
            if downloaded != expected {
                return Err(DownloadError::Truncated { expected, actual: downloaded });
            }
        }

        let raw_hash: [u8; 32] = hasher.finalize().into();

        Ok((downloaded, raw_hash))
    }

    pub async fn download_batch(
        &self,
        requests: &[DownloadRequest],
    ) -> Vec<Result<DownloadedPackage, DownloadError>> {
        let client = self.client.clone();
        let semaphore = self.semaphore.clone();
        let scheduler = DownloadScheduler::default();
        let batches = scheduler.schedule(requests);

        let mut index_map = std::collections::HashMap::new();
        for (i, req) in requests.iter().enumerate() {
            index_map.insert((req.name.clone(), req.version.clone()), i);
        }

        let mut result_map: std::collections::HashMap<usize, Result<DownloadedPackage, DownloadError>> = std::collections::HashMap::new();
        let mut next_error_idx = requests.len();

        for batch in &batches {
            let mut set = JoinSet::new();

            for req in batch {
                let permit = semaphore.clone().acquire_owned().await;
                match permit {
                    Ok(_permit) => {
                        let client = client.clone();
                        let req = req.clone();
                        set.spawn(async move {
                            let result = Self::download_one_inner(&client, req, Some(DOWNLOAD_TIMEOUT)).await;
                            drop(_permit);
                            result
                        });
                    }
                    Err(_) => {
                        result_map.insert(next_error_idx, Err(DownloadError::NetworkError("semaphore closed".into())));
                        next_error_idx += 1;
                    }
                }
            }

            while let Some(res) = set.join_next().await {
                match res {
                    Ok(result) => {
                        if let Ok(ref pkg) = result {
                            let key = (pkg.name.clone(), pkg.version.clone());
                            if let Some(&idx) = index_map.get(&key) {
                                result_map.insert(idx, result);
                            }
                        } else if let Err(ref _err) = result {
                            result_map.insert(next_error_idx, result);
                            next_error_idx += 1;
                        }
                    }
                    Err(e) => {
                        result_map.insert(next_error_idx, Err(DownloadError::NetworkError(e.to_string())));
                        next_error_idx += 1;
                    }
                }
            }
        }

        let mut all_results = Vec::with_capacity(requests.len());
        for i in 0..requests.len() {
            all_results.push(
                result_map.remove(&i).unwrap_or(Err(DownloadError::NetworkError("download failed".into())))
            );
        }

        all_results
    }

    pub async fn download_one(
        &self,
        req: &DownloadRequest,
    ) -> Result<DownloadedPackage, DownloadError> {
        let _permit = self.semaphore.acquire().await.map_err(|_| DownloadError::NetworkError("semaphore closed".into()))?;
        let handle = self.connection_pool.acquire(req.host());
        let result = Self::download_one_inner(&self.client, req.clone(), Some(DOWNLOAD_TIMEOUT)).await;
        self.connection_pool.release(handle);
        drop(_permit);
        result
    }

    async fn download_one_inner(
        client: &reqwest::Client,
        req: DownloadRequest,
        timeout: Option<Duration>,
    ) -> Result<DownloadedPackage, DownloadError> {
        req.validate_url()?;
        let start = std::time::Instant::now();

        let response = if let Some(t) = timeout {
            tokio::time::timeout(t, client.get(&req.url).send())
                .await
                .map_err(|_| DownloadError::Timeout(req.url.clone()))?
                .map_err(|e| DownloadError::NetworkError(e.to_string()))?
        } else {
            client.get(&req.url).send().await?
        };

        let status = response.status();
        if !status.is_success() {
            return Err(DownloadError::HttpError(status.as_u16()));
        }

        let content_length = response.content_length();
        let body = response.bytes().await?.to_vec();
        let duration = start.elapsed();

        if let Some(expected) = content_length {
            let actual = body.len() as u64;
            if actual != expected {
                return Err(DownloadError::Truncated { expected, actual });
            }
        }

        if let Some(ref expected) = req.integrity {
            let actual_hash = Sha256::digest(&body);
            let actual_hex = hex::encode(actual_hash);
            if actual_hex != *expected {
                return Err(DownloadError::IntegrityMismatch {
                    expected: expected.clone(),
                    actual: actual_hex,
                });
            }
        }

        Ok(DownloadedPackage {
            name: req.name,
            version: req.version,
            size: body.len(),
            data: body,
            duration,
        })
    }
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_single_host() {
        let scheduler = DownloadScheduler::new(48, 6);
        let requests: Vec<DownloadRequest> = (0..10)
            .map(|i| DownloadRequest {
                name: format!("pkg{}", i),
                version: "1.0.0".into(),
                url: format!("https://registry.npmjs.org/pkg{}/1.0.0", i),
                integrity: None,
            })
            .collect();

        let batches = scheduler.schedule(&requests);
        assert!(!batches.is_empty());
        for batch in &batches {
            assert!(batch.len() <= 6, "batch exceeds per-host limit");
        }
    }

    #[test]
    fn test_scheduler_multi_host() {
        let scheduler = DownloadScheduler::new(48, 6);
        let requests = vec![
            DownloadRequest {
                name: "pkg1".into(), version: "1.0.0".into(),
                url: "https://registry.npmjs.org/pkg1".into(), integrity: None,
            },
            DownloadRequest {
                name: "pkg2".into(), version: "1.0.0".into(),
                url: "https://registry.yarnpkg.com/pkg2".into(), integrity: None,
            },
        ];

        let batches = scheduler.schedule(&requests);
        let total: usize = batches.iter().map(|b| b.len()).sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn test_host_extraction() {
        let req = DownloadRequest {
            name: "test".into(), version: "1.0.0".into(),
            url: "https://registry.npmjs.org/test/1.0.0".into(), integrity: None,
        };
        assert_eq!(req.host(), "registry.npmjs.org");

        let req_http = DownloadRequest {
            name: "test".into(), version: "1.0.0".into(),
            url: "http://example.com/pkg".into(), integrity: None,
        };
        assert_eq!(req_http.host(), "example.com");
    }

    #[test]
    fn test_scheduler_empty() {
        let scheduler = DownloadScheduler::new(48, 6);
        let batches = scheduler.schedule(&[]);
        assert!(batches.is_empty());
    }

    #[test]
    fn test_connection_pool_basic() {
        let pool = ConnectionPool::new();
        assert!(pool.can_connect("example.com"));

        let handle = pool.acquire("example.com");
        assert_eq!(pool.stats().active, 1);

        pool.release(handle);
        let stats = pool.stats();
        assert_eq!(stats.active, 0);

        pool.cleanup();
    }
}
