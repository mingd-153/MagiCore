//! CAS (Content Addressable Storage) module
//!
//! Provides content-addressed file storage with SQLite indexing.
//! Features: deduplication, hardlink export, integrity verification, symlink protection.

pub mod integrity;
pub mod lifecycle;
pub mod security;
pub mod write;

mod store;

pub use integrity::{IntegrityHash, TarballEntry};
pub use store::ContentStore;

#[cfg(test)]
pub mod tests;
