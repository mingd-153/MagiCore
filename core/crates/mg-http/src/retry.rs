/// Retry strategies for HTTP requests
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum RetryStrategy {
    /// Fixed delay between retries
    Fixed(Duration),
    /// Exponential backoff
    Exponential { base: Duration, max: Duration },
}

impl RetryStrategy {
    pub fn delay(&self, attempt: u32) -> Duration {
        match self {
            Self::Fixed(d) => *d,
            Self::Exponential { base, max } => {
                let delay = base.mul_f64(2_f64.powi(attempt as i32));
                delay.min(*max)
            }
        }
    }
}
