//! Multi-registry search engine for MagiCore
//! Công cụ tìm kiếm đa registry cho MagiCore
//!
//! Search packages across multiple registries (npm, crates.io, pkg.go.dev, PyPI)
//! with smart ranking, caching, and user preference learning.
//! Tìm kiếm packages qua nhiều registry với xếp hạng thông minh, cache, học sở thích user.

pub mod cache;
pub mod client;
pub mod clients;
pub mod orchestrator;
pub mod prompt;
pub mod ranking;
pub mod types;

pub use cache::SearchCache;
pub use client::SearchClient;
pub use clients::*;
pub use orchestrator::SearchOrchestrator;
pub use prompt::prompt_selection;
pub use types::*;
