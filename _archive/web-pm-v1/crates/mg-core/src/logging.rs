//! Tracing and logging configuration

use std::env;
use tracing::Level;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

/// Configuration for tracing initialization
#[derive(Debug, Clone)]
pub struct TracingConfig {
    pub level: Level,
    pub json: bool,
    pub with_thread_names: bool,
    pub with_thread_ids: bool,
    pub with_file: bool,
    pub with_line_number: bool,
    pub ansi: bool,
}

impl Default for TracingConfig {
    fn default() -> Self {
        let level = env::var("MGPM_LOG")
            .or_else(|_| env::var("RUST_LOG"))
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(Level::INFO);

        Self {
            level,
            json: env::var("MGPM_LOG_JSON").is_ok(),
            with_thread_names: true,
            with_thread_ids: true,
            with_file: true,
            with_line_number: true,
            ansi: env::var("NO_COLOR").is_err(),
        }
    }
}

/// Initialize global tracing subscriber
pub fn init_tracing(config: TracingConfig) {
    let env_filter = EnvFilter::builder()
        .with_default_directive(config.level.into())
        .from_env_lossy();

    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_names(config.with_thread_names)
        .with_thread_ids(config.with_thread_ids)
        .with_file(config.with_file)
        .with_line_number(config.with_line_number)
        .with_ansi(config.ansi)
        .with_level(true);

    let subscriber = Registry::default().with(env_filter);

    if config.json {
        subscriber.with(fmt_layer.json()).init();
    } else {
        subscriber.with(fmt_layer).init();
    }
}

/// Initialize tracing with default config (for tests)
#[cfg(test)]
pub fn init_test_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
}
