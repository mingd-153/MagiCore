//! Rate limiting middleware — fixed window per client IP
//! (Rate limit: cửa sổ cố định theo IP client, in-memory)

use axum::{
    extract::{ConnectInfo, Request},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Cấu hình rate limit
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Số request tối đa trong cửa sổ — 0 = tắt
    pub max_requests: usize,
    /// Độ dài cửa sổ (giây)
    pub window_secs: u64,
}

struct WindowState {
    started: Instant,
    count: usize,
}

/// Bộ đếm in-memory — ponytail: đơn giản theo IP, đủ cho registry private;
/// nâng lên per-token + hậu trường Redis khi cần scale
pub struct RateLimiter {
    config: RateLimitConfig,
    windows: Mutex<HashMap<String, WindowState>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            windows: Mutex::new(HashMap::new()),
        }
    }

    /// Kiểm tra + tăng bộ đếm cho client; false = vượt giới hạn
    pub fn allow(&self, client: &str) -> bool {
        let mut windows = self.windows.lock().unwrap();
        let now = Instant::now();
        let state = windows.entry(client.to_string()).or_insert(WindowState {
            started: now,
            count: 0,
        });
        if state.started.elapsed().as_secs() >= self.config.window_secs {
            state.started = now;
            state.count = 0;
        }
        state.count += 1;
        state.count <= self.config.max_requests
    }
}

/// Axum middleware gắn rate limit
pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let limiter = request
        .extensions()
        .get::<Arc<RateLimiter>>()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    if !limiter.allow(&addr.ip().to_string()) {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(next.run(request).await)
}
