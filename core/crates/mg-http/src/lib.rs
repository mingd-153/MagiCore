pub mod cache;
pub mod methods;
pub mod offline;
pub mod proxy;
pub mod ratelimit;
pub mod retry;
pub mod timeout;
pub mod tls;
pub mod upload;

pub use methods::HttpClient;
pub use ratelimit::RateLimiter;
pub use cache::HttpCache;
pub use tls::TlsConfig;

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    // --- RetryStrategy ---

    #[test]
    fn retry_fixed_returns_same_delay_for_any_attempt() {
        let strat = super::retry::RetryStrategy::Fixed(Duration::from_secs(5));
        assert_eq!(strat.delay(0), Duration::from_secs(5));
        assert_eq!(strat.delay(1), Duration::from_secs(5));
        assert_eq!(strat.delay(99), Duration::from_secs(5));
    }

    #[test]
    fn retry_exponential_doubles_each_attempt() {
        let strat = super::retry::RetryStrategy::Exponential {
            base: Duration::from_secs(1),
            max: Duration::from_secs(60),
        };
        assert_eq!(strat.delay(0), Duration::from_secs(1));
        assert_eq!(strat.delay(1), Duration::from_secs(2));
        assert_eq!(strat.delay(2), Duration::from_secs(4));
        assert_eq!(strat.delay(3), Duration::from_secs(8));
    }

    #[test]
    fn retry_exponential_clamps_to_max() {
        let strat = super::retry::RetryStrategy::Exponential {
            base: Duration::from_secs(1),
            max: Duration::from_secs(10),
        };
        assert_eq!(strat.delay(4), Duration::from_secs(10));
        assert_eq!(strat.delay(10), Duration::from_secs(10));
    }

    #[test]
    fn retry_exponential_small_base_clamps() {
        let strat = super::retry::RetryStrategy::Exponential {
            base: Duration::from_millis(500),
            max: Duration::from_secs(2),
        };
        assert_eq!(strat.delay(0), Duration::from_millis(500));
        assert_eq!(strat.delay(1), Duration::from_millis(1000));
        assert_eq!(strat.delay(2), Duration::from_secs(2));
    }

    // --- RateLimitConfig ---

    #[test]
    fn ratelimit_default_values() {
        let config = super::ratelimit::RateLimitConfig::default();
        assert_eq!(config.max_requests, 100);
        assert_eq!(config.period, Duration::from_secs(60));
    }

    #[test]
    fn ratelimit_default_field_order() {
        let config = super::ratelimit::RateLimitConfig::default();
        assert!(config.max_requests > 0);
        assert!(config.period.as_nanos() > 0);
    }

    // --- CacheEntry ---

    #[test]
    fn cache_entry_fresh_is_valid() {
        let entry = super::cache::CacheEntry {
            data: vec![1, 2, 3],
            timestamp: SystemTime::now(),
            ttl: Duration::from_secs(60),
        };
        assert!(entry.is_valid());
    }

    #[test]
    fn cache_entry_expired_is_invalid() {
        let past = SystemTime::now()
            .checked_sub(Duration::from_secs(120))
            .expect("system time anomaly");
        let entry = super::cache::CacheEntry {
            data: Vec::new(),
            timestamp: past,
            ttl: Duration::from_secs(60),
        };
        assert!(!entry.is_valid());
    }

    #[test]
    fn cache_entry_future_timestamp_is_invalid() {
        let future = SystemTime::now()
            .checked_add(Duration::from_secs(60))
            .expect("time overflow");
        let entry = super::cache::CacheEntry {
            data: Vec::new(),
            timestamp: future,
            ttl: Duration::from_secs(60),
        };
        assert!(!entry.is_valid());
    }

    #[test]
    fn cache_entry_zero_ttl_is_expired() {
        let entry = super::cache::CacheEntry {
            data: Vec::new(),
            timestamp: SystemTime::now(),
            ttl: Duration::ZERO,
        };
        assert!(!entry.is_valid());
    }
}
