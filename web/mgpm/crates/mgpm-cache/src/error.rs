use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("invalid magic bytes: expected {:?}, got {:?}", expected, actual)]
    InvalidMagic { expected: [u8; 8], actual: [u8; 8] },

    #[error("unsupported cache version: {0}, expected {expected}", expected = crate::CACHE_VERSION)]
    UnsupportedVersion(u32),

    #[error("corrupt cache: {0}")]
    Corruption(String),

    #[error("mmap error: {0}")]
    Mmap(String),

    #[error("cache full: cannot insert more entries")]
    CacheFull,
}
