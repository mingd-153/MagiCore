#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for HTTP rate limiting

use super::*;
use std::time::Duration;

#[test]
fn ratelimit_default_values() {
    let config = RateLimitConfig::default();
    assert_eq!(config.max_requests, 100);
    assert_eq!(config.period, Duration::from_secs(60));
}
