/// Rate limiting for HTTP requests
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

/// Token bucket rate limiter
#[derive(Debug, Clone)]
pub struct RateLimiter {
    config: RateLimitConfig,
    requests: Arc<Mutex<VecDeque<Instant>>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            requests: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub async fn wait(&self) {
        loop {
            let wait_time = {
                let mut requests = self.requests.lock().expect("lock poisoned");
                let now = Instant::now();
                let cutoff = now - self.config.period;

                // Remove old requests
                while let Some(&front) = requests.front() {
                    if front < cutoff {
                        requests.pop_front();
                    } else {
                        break;
                    }
                }

                if (requests.len() as u32) < self.config.max_requests {
                    requests.push_back(now);
                    return;
                }

                // Need to wait
                let oldest = requests.front().copied().unwrap_or(now);
                self.config
                    .period
                    .saturating_sub(now.duration_since(oldest))
            };
            tokio::time::sleep(wait_time).await;
        }
    }
}

#[cfg(test)]
#[path = "test/ratelimit_test.rs"]
mod tests;
