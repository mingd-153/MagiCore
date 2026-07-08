/// Rate limiting for HTTP requests
use std::time::Duration;

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per period
    pub max_requests: u32,
    /// Time period
    pub period: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            period: Duration::from_secs(60),
        }
    }
}
