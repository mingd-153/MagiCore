//! Multi-registry search engine for MegaGate
//! Công cụ tìm kiếm đa registry cho MegaGate
//!
//! Search packages across multiple registries (npm, crates.io, pkg.go.dev, PyPI)
//! with smart ranking, caching, and user preference learning.
//! Tìm kiếm packages qua nhiều registry với xếp hạng thông minh, cache, học sở thích user.

pub mod types;
pub mod client;
pub mod orchestrator;
pub mod ranking;
pub mod cache;
pub mod prompt;
pub mod clients;

pub use types::*;
pub use client::SearchClient;
pub use orchestrator::SearchOrchestrator;
pub use cache::SearchCache;
pub use prompt::prompt_selection;
pub use clients::*;
