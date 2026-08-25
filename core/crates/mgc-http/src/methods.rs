//! HTTP methods wrapper — GET/PUT/POST/PATCH/DELETE chung (12 §11)
//! (Wrapper chung cho reqwest — retry/ratelimit/cache tích hợp sẵn)

use crate::{
    cache::HttpCache,
    ratelimit::{RateLimitConfig, RateLimiter},
    retry::RetryStrategy,
    timeout::{apply_timeouts, TimeoutConfig},
    tls::TlsConfig,
};
use anyhow::Result;
use reqwest::{Client, RequestBuilder, Response, StatusCode};
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
        let builder = apply_timeouts(Client::builder(), timeout);
        let builder = tls.apply(builder)?;
        let client = builder
            .build()
            .map_err(|e| anyhow::anyhow!("build reqwest client: {}", e))?;
        Ok(Self {
            client,
            retry: RetryStrategy::Exponential {
                base: Duration::from_secs(1),
                max: Duration::from_secs(30),
            },
            ratelimit: None,
            cache: None,
            auth: None,
        })
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
                    let delay = self.retry.delay(attempt);
                    tracing::warn!("HTTP {} - retry in {:?}", r.status(), delay);
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    continue;
                }
                Ok(r) => return Ok(r),
                Err(e) if self.is_retryable_error(&e) => {
                    let delay = self.retry.delay(attempt);
                    tracing::warn!("HTTP error: {} - retry in {:?}", e, delay);
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

    pub async fn delete(&self, url: &str) -> Result<Response> {
        self.execute(self.client.delete(url)).await
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new().expect("HttpClient::new")
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn http_client_default_works() {
        let client = HttpClient::new().unwrap();
        assert!(client.retry.delay(0).as_secs() >= 1);
    }
}
