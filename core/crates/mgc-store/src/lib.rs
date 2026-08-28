#![cfg_attr(test, allow(clippy::unwrap_used))]
//! Package store and cache management for MagiCore
//!
//! Content-addressable storage, package caching, and tarball extraction.

pub mod cache;
pub mod cas;
pub mod database;
pub mod index;
pub mod layout;

pub use cache::PackageCache;
pub use cas::{CompiledCache, CompiledModule, ContentStore, IntegrityHash};
pub use database::{Database, DatabaseEntry};
pub use index::{FileEntry, StoreIndex};
pub use layout::Layout;

/// Trả về đường dẫn global store: `~/.magicore/store/v3`
/// (env `MAGICORE_STORE_ROOT` override — tests + custom setups)
pub fn default_store_root() -> std::path::PathBuf {
    if let Ok(override_dir) = std::env::var("MAGICORE_STORE_ROOT") {
        if !override_dir.is_empty() {
            return std::path::PathBuf::from(override_dir);
        }
    }
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".magicore")
        .join("store")
        .join("v3")
}

