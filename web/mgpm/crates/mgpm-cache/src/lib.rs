pub mod binary;
pub mod error;
pub mod memmap;

pub use binary::CacheHeader;
pub use error::CacheError;
pub use memmap::MemMapCache;

pub const CACHE_MAGIC: &[u8; 8] = b"MGPMCACH";
pub const CACHE_VERSION: u32 = 1;

pub const INITIAL_SIZE: u64 = 1_048_576;

pub const HEADER_SIZE: u32 = 32;

pub const PAIRS_PER_ENTRY: u32 = 4;
pub const PAIR_BYTES: u32 = PAIRS_PER_ENTRY * 4;
pub const HASH_BYTES: u32 = 8;

#[derive(Debug, Clone, Copy)]
pub struct CacheEntry<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
}
