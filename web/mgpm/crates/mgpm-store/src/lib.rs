//! MGPM Store Crate
//!
//! Content-addressable storage, package caching, and tarball extraction.

pub mod cache;
pub mod store;
pub mod tarball;

pub use cache::{CachedPackage, PackageCache};
pub use store::{
    CasContentStore, ContentStore, FileEntry, GlobalVirtualStore, GvsGcReport, GvsPackageInfo, GvsFileInfo, GvsStats,
    HashAlgorithm, ImportMethod, IntegrityHash, PackageEntry, PackageInfo, ProjectInfo,
    SqliteStore, StoreError, StoreIndex, StoreReport, StoreVerifier, TarballEntry,
};
pub use tarball::{EntryType, ExtractedEntry, TarballError, TarballExtractor};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_creation() {
        let temp = tempfile::tempdir().unwrap();
        let store = ContentStore::new(temp.path().to_path_buf()).unwrap();
        assert_eq!(store.file_count(), 0);
    }
}
