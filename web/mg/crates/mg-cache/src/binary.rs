use crate::{CacheError, CACHE_MAGIC, CACHE_VERSION};

pub const HEADER_SIZE: u32 = 32;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CacheHeader {
    pub magic: [u8; 8],
    pub version: u32,
    pub entry_count: u32,
    pub string_table_size: u32,
    pub hash_table_size: u32,
    pub metadata_size: u32,
    pub reserved: [u8; 4],
}

impl CacheHeader {
    pub fn new() -> Self {
        Self {
            magic: *CACHE_MAGIC,
            version: CACHE_VERSION,
            entry_count: 0,
            string_table_size: 0,
            hash_table_size: 0,
            metadata_size: 0,
            reserved: [0u8; 4],
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CacheError> {
        if bytes.len() < HEADER_SIZE as usize {
            return Err(CacheError::Corruption("header too short".into()));
        }
        let mut magic = [0u8; 8];
        magic.copy_from_slice(&bytes[..8]);
        if &magic != CACHE_MAGIC {
            return Err(CacheError::InvalidMagic);
        }
        let mut buf4 = [0u8; 4];
        buf4.copy_from_slice(&bytes[8..12]);
        let version = u32::from_le_bytes(buf4);
        if version != CACHE_VERSION {
            return Err(CacheError::UnsupportedVersion(version));
        }
        buf4.copy_from_slice(&bytes[12..16]);
        let entry_count = u32::from_le_bytes(buf4);
        buf4.copy_from_slice(&bytes[16..20]);
        let string_table_size = u32::from_le_bytes(buf4);
        buf4.copy_from_slice(&bytes[20..24]);
        let hash_table_size = u32::from_le_bytes(buf4);
        buf4.copy_from_slice(&bytes[24..28]);
        let metadata_size = u32::from_le_bytes(buf4);
        let mut reserved = [0u8; 4];
        reserved.copy_from_slice(&bytes[28..32]);
        Ok(Self {
            magic,
            version,
            entry_count,
            string_table_size,
            hash_table_size,
            metadata_size,
            reserved,
        })
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        buf[..8].copy_from_slice(&self.magic);
        buf[8..12].copy_from_slice(&self.version.to_le_bytes());
        buf[12..16].copy_from_slice(&self.entry_count.to_le_bytes());
        buf[16..20].copy_from_slice(&self.string_table_size.to_le_bytes());
        buf[20..24].copy_from_slice(&self.hash_table_size.to_le_bytes());
        buf[24..28].copy_from_slice(&self.metadata_size.to_le_bytes());
        buf[28..32].copy_from_slice(&self.reserved);
        buf
    }

    pub fn total_size(&self) -> u64 {
        HEADER_SIZE as u64
            + self.string_table_size as u64
            + self.hash_table_size as u64
            + self.metadata_size as u64
    }
}

impl Default for CacheHeader {
    fn default() -> Self {
        Self::new()
    }
}
