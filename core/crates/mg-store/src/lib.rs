/// Package store and cache management for MegaGate
///
/// Content-addressable storage, package caching, and tarball extraction.
pub mod cache;
pub mod cas;
pub mod database;
pub mod layout;

pub use cache::PackageCache;
pub use cas::{ContentStore, IntegrityHash};
pub use database::{Database, DatabaseEntry};
pub use layout::Layout;

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
