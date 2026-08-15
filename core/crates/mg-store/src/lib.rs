/// Package store and cache management for MegaGate
///
/// Content-addressable storage, package caching, and tarball extraction.
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

/// Trả về đường dẫn global store: `~/.megagate/store/v3`
pub fn default_store_root() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".megagate")
        .join("store")
        .join("v3")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_creation() {
        let temp = tempfile::tempdir().unwrap();
        let store = ContentStore::new(temp.path().to_path_buf()).unwrap();
        assert!(store.root().exists());
    }
}
