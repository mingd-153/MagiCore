//! HTTP methods wrapper — GET/PUT/POST/PATCH/DELETE chung (12 §11)
//! (Wrapper chung cho reqwest — retry/ratelimit/cache tích hợp sẵn)

use crate::{
    cache::HttpCache,
    ratelimit::{RateLimitConfig, RateLimiter},
    retry::RetryStrategy,
    timeout::{TimeoutConfig, apply_timeouts},
    tls::TlsConfig,
};
use anyhow::Result;
use reqwest::{Client, ClientBuilder, RequestBuilder, Response, StatusCode, header::HeaderMap};
use std::time::Duration;

/// HTTP client with built-in retry, rate limit, cache
#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    retry: RetryStrategy,
    ratelimit: Option<RateLimiter>,
    cache: Option<HttpCache>,
    auth: Option<(String, String)>,
}

impl HttpClient {
    pub fn new() -> Result<Self> {
        Self::with_security(&TimeoutConfig::default(), &TlsConfig::default())
    }

    /// Build an HTTP client with explicit timeout and TLS policy.
    /// Tạo client từ policy rõ ràng để tránh config bảo mật bị giữ nhưng không dùng.
    pub fn with_security(timeout: &TimeoutConfig, tls: &TlsConfig) -> Result<Self> {
        Self::with_builder(Client::builder(), timeout, tls)
    }

    /// Build a client that refuses redirects for identity and credential requests.
    /// Tạo client từ chối redirect cho request định danh và credential.
    pub fn with_security_no_redirects(timeout: &TimeoutConfig, tls: &TlsConfig) -> Result<Self> {
        Self::with_builder(
            Client::builder().redirect(reqwest::redirect::Policy::none()),
            timeout,
            tls,
        )
    }

    fn with_builder(
        builder: ClientBuilder,
        timeout: &TimeoutConfig,
        tls: &TlsConfig,
    ) -> Result<Self> {
        let builder = apply_timeouts(builder, timeout);
        let builder = tls.apply(builder)?;
        let client = builder
            .build()
            .map_err(|e| anyhow::anyhow!("build reqwest client: {}", e))?;
        Ok(Self::from_client(client))
    }

    fn from_client(client: Client) -> Self {
        Self {
            client,
            retry: RetryStrategy::Exponential {
                base: Duration::from_secs(1),
                max: Duration::from_secs(30),
            },
            ratelimit: None,
            cache: None,
            auth: None,
        }
    }

    /// Attach a static auth header (e.g. ("authorization", "Bearer <token>")) to every request
    pub fn with_auth(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.auth = Some((name.into(), value.into()));
        self
    }

    pub fn with_retry(mut self, strategy: RetryStrategy) -> Self {
        self.retry = strategy;
        self
    }

    pub fn with_ratelimit(mut self, max_req: u32, per: Duration) -> Self {
        self.ratelimit = Some(RateLimiter::new(RateLimitConfig {
            max_requests: max_req,
            period: per,
        }));
        self
    }

    pub fn with_cache(mut self, cache: HttpCache) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Execute request with retry + rate limit
    pub async fn execute(&self, req: RequestBuilder) -> Result<Response> {
        // Rate limit
        if let Some(ref rl) = self.ratelimit {
            rl.wait().await;
        }

        // Bounded retries (CI fix 2026-09-11): a dead endpoint previously
        // retried FOREVER on connect errors (is_connect), hanging every
        // consumer. Three attempts, then the error surfaces — callers own
        // fail-closed policy.
        // Retry hữu hạn (fix CI): endpoint chết trước đây retry VÔ HẠN
        // với connect error (is_connect), treo mọi consumer. Ba lần thử
        // rồi lỗi nổi lên — caller giữ chính sách fail-closed.
        const MAX_RETRIES: u32 = 3;
        let mut attempt = 0;
        loop {
            let mut req = req
                .try_clone()
                .ok_or_else(|| anyhow::anyhow!("request not cloneable"))?;
            if let Some((name, value)) = &self.auth {
                req = req.header(name, value);
            }
            let resp = req.send().await;

            match resp {
                Ok(r) if self.should_retry(r.status()) => {
                    if attempt >= MAX_RETRIES {
                        return Ok(r);
                    }
                    let delay = self.retry.delay(attempt);
                    tracing::warn!("HTTP {} - retry in {:?}", r.status(), delay);
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    continue;
                }
                Ok(r) => return Ok(r),
                Err(e) if self.is_retryable_error(&e) => {
                    if attempt >= MAX_RETRIES {
                        return Err(e.into());
                    }
                    let delay = self.retry.delay(attempt);
                    let error_kind = if e.is_timeout() {
                        "timeout"
                    } else if e.is_connect() {
                        "connect"
                    } else {
                        "request"
                    };
                    tracing::warn!(error_kind, "HTTP request failed; retry in {:?}", delay);
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn should_retry(&self, status: StatusCode) -> bool {
        matches!(status.as_u16(), 429 | 500..=599)
    }

    fn is_retryable_error(&self, e: &reqwest::Error) -> bool {
        e.is_timeout() || e.is_connect() || e.is_request()
    }

    // Convenience methods
    pub async fn get(&self, url: &str) -> Result<Response> {
        self.execute(self.client.get(url)).await
    }

    /// GET a response while enforcing a hard byte limit on its body.
    /// Tải GET với giới hạn byte cứng cho response body.
    pub async fn get_bytes_limited(&self, url: &str, max_bytes: usize) -> Result<(u16, Vec<u8>)> {
        let mut response = self.execute(self.client.get(url)).await?;
        let status = response.status().as_u16();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if body.len() > max_bytes || chunk.len() > max_bytes - body.len() {
                return Err(anyhow::anyhow!("HTTP response exceeds byte limit"));
            }
            body.extend_from_slice(&chunk);
        }
        Ok((status, body))
    }

    pub async fn put(&self, url: &str, body: Vec<u8>) -> Result<Response> {
        self.execute(self.client.put(url).body(body)).await
    }

    pub async fn post(&self, url: &str, body: Vec<u8>) -> Result<Response> {
        self.execute(self.client.post(url).body(body)).await
    }

    pub async fn patch(&self, url: &str, body: Vec<u8>) -> Result<Response> {
        self.execute(self.client.patch(url).body(body)).await
    }

    pub async fn patch_with_timeout(
        &self,
        url: &str,
        body: Vec<u8>,
        timeout: Duration,
    ) -> Result<Response> {
        self.execute(self.client.patch(url).timeout(timeout).body(body))
            .await
    }

    /// Send PATCH with additional headers and a per-request timeout.
    /// Gửi PATCH kèm header bổ sung và timeout riêng cho request.
    pub async fn patch_with_timeout_and_headers(
        &self,
        url: &str,
        body: Vec<u8>,
        timeout: Duration,
        headers: HeaderMap,
    ) -> Result<Response> {
        self.execute(
            self.client
                .patch(url)
                .timeout(timeout)
                .headers(headers)
                .body(body),
        )
        .await
    }

    pub async fn delete(&self, url: &str) -> Result<Response> {
        self.execute(self.client.delete(url)).await
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new().expect("HttpClient::new")
    }
}

/// Opaque debug: the inner reqwest client / limiter / cache carry no
/// stable debug form, and consumers (resolver protocols) derive Debug.
/// (Debug mờ: client/limiter/cache trong không có dạng debug ổn định.)
impl std::fmt::Debug for HttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpClient").finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "test/methods_test.rs"]
mod tests;
