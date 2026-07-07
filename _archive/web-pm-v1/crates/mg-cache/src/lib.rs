pub mod binary;
pub mod error;
pub mod etag;
pub mod memmap;

pub use binary::CacheHeader;
pub use error::CacheError;
pub use etag::ETagStore;
pub use memmap::MemMapCache;

pub const CACHE_MAGIC: &[u8; 8] = b"MGPMCACH";
pub const CACHE_VERSION: u32 = 2;

pub const INITIAL_SIZE: u64 = 1_048_576;

pub const HEADER_SIZE: u32 = 32;

#[derive(Debug, Clone, Copy)]
pub struct CacheEntry<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub integrity: &'a str,
    pub data: &'a [u8],
}
