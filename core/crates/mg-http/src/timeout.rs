//! Timeout configuration per request type (12 §4)
//! (Cấu hình timeout chi tiết cho từng loại request)

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Timeout configuration cho các loại request khác nhau
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// Connect timeout (mặc định 10s)
    #[serde(default = "default_connect")]
    pub connect: Duration,
    /// Header read timeout (mặc định 30s)
    #[serde(default = "default_header_read")]
    pub header_read: Duration,
    /// Total request timeout cho metadata/small body <1MB (mặc định 30s)
    #[serde(default = "default_request")]
    pub request: Duration,
    /// Download lớn (streaming) — KHÔNG timeout
    #[serde(default)]
    pub download_stream: Option<Duration>,
    /// Upload chunk timeout (mặc định 60s/chunk)
    #[serde(default = "default_upload_chunk")]
    pub upload_chunk: Duration,
    /// DNS resolution timeout (mặc định 5s)
    #[serde(default = "default_dns")]
    pub dns: Duration,
}

fn default_connect() -> Duration { Duration::from_secs(10) }
fn default_header_read() -> Duration { Duration::from_secs(30) }
fn default_request() -> Duration { Duration::from_secs(30) }
fn default_upload_chunk() -> Duration { Duration::from_secs(60) }
fn default_dns() -> Duration { Duration::from_secs(5) }

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            connect: default_connect(),
            header_read: default_header_read(),
            request: default_request(),
            download_stream: None,
            upload_chunk: default_upload_chunk(),
            dns: default_dns(),
        }
    }
}

/// Timeout builder cho reqwest
pub fn apply_timeouts(builder: reqwest::ClientBuilder, config: &TimeoutConfig) -> reqwest::ClientBuilder {
    builder
        .connect_timeout(config.connect)
        .timeout(config.request)
        .tcp_keepalive(Some(std::time::Duration::from_secs(30)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_config_defaults() {
        let cfg = TimeoutConfig::default();
        assert_eq!(cfg.connect.as_secs(), 10);
        assert_eq!(cfg.request.as_secs(), 30);
        assert_eq!(cfg.upload_chunk.as_secs(), 60);
    }
}