use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use fxhash;
use memmap2::MmapMut;
use tracing::warn;

use crate::{binary::CacheHeader, CacheEntry, CacheError, CACHE_MAGIC, CACHE_VERSION, HEADER_SIZE, INITIAL_SIZE};

const PAIRS_PER_ENTRY: u32 = 4;
const PAIR_BYTES: u32 = PAIRS_PER_ENTRY * 4;
const HASH_BYTES: u32 = 8;

pub struct MemMapCache {
    map: MmapMut,
    header: CacheHeader,
    entry_count: AtomicU32,
    capacity: u32,
    pairs_offset: usize,
    hashes_offset: usize,
    strings_offset: usize,
}

unsafe impl Send for MemMapCache {}

impl MemMapCache {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CacheError> {
        use std::fs::OpenOptions;

        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.as_ref())?;

        let file_len = file.metadata()?.len();
        let is_new = file_len == 0;
        if is_new {
            file.set_len(INITIAL_SIZE)?;
        }

        let map = unsafe { MmapMut::map_mut(&file)? };

        if map.len() < HEADER_SIZE as usize {
            return Err(CacheError::Corruption("file too small for header".into()));
        }

        if is_new {
            let header_bytes = CacheHeader::new().to_bytes();
            unsafe {
                std::ptr::copy_nonoverlapping(header_bytes.as_ptr(), map.as_ptr() as *mut u8, HEADER_SIZE as usize);
            }
        }

        let header = CacheHeader::from_bytes(&map[..HEADER_SIZE as usize])?;
        let map_len = map.len();

        let overhead_per_entry = (PAIR_BYTES + HASH_BYTES) as usize;
        let max_entries_for_overhead = (map_len - HEADER_SIZE as usize) / overhead_per_entry;
        let capacity = (max_entries_for_overhead / 4).max(16) as u32;

        let pairs_offset = HEADER_SIZE as usize;
        let hashes_offset = pairs_offset + capacity as usize * PAIR_BYTES as usize;
        let strings_offset = hashes_offset + capacity as usize * HASH_BYTES as usize;

        Ok(Self {
            map,
            header,
            entry_count: AtomicU32::new(header.entry_count),
            capacity,
            pairs_offset,
            hashes_offset,
            strings_offset,
        })
    }

    pub fn get(&self, key: &str) -> Option<CacheEntry<'_>> {
        let hash = fxhash::hash64(key);
        let hashes = self.hashes_slice();
        for (idx, &h) in hashes.iter().enumerate() {
            if h == hash {
                return Some(self.read_entry(idx));
            }
        }
        None
    }

    pub fn insert(&mut self, entry: CacheEntry<'_>) -> Result<(), CacheError> {
        let idx = self.entry_count.fetch_add(1, Ordering::SeqCst) as usize;
        if idx >= self.capacity as usize {
            self.entry_count.fetch_sub(1, Ordering::SeqCst);
            return Err(CacheError::CacheFull);
        }
        self.write_entry(idx, &entry)?;
        self.update_header();
        Ok(())
    }

    pub fn flush(&self) -> Result<(), CacheError> {
        self.map.flush()?;
        Ok(())
    }

    pub fn entry_count(&self) -> u32 {
        self.entry_count.load(Ordering::Acquire)
    }

    fn hashes_slice(&self) -> &[u64] {
        let count = self.entry_count.load(Ordering::Acquire) as usize;
        if count == 0 {
            return &[];
        }
        unsafe {
            std::slice::from_raw_parts(self.map.as_ptr().add(self.hashes_offset) as *const u64, count)
        }
    }

    fn pairs_slice(&self) -> &[u32] {
        let count = self.entry_count.load(Ordering::Acquire) as usize;
        if count == 0 {
            return &[];
        }
        unsafe {
            let count_u32 = count * PAIRS_PER_ENTRY as usize;
            std::slice::from_raw_parts(self.map.as_ptr().add(self.pairs_offset) as *const u32, count_u32)
        }
    }

    fn read_entry<'a>(&'a self, idx: usize) -> CacheEntry<'a> {
        let pairs = self.pairs_slice();
        let base_idx = idx * PAIRS_PER_ENTRY as usize;

        let name_offset = pairs[base_idx] as usize;
        let name_len = pairs[base_idx + 1] as usize;
        let ver_offset = pairs[base_idx + 2] as usize;
        let ver_len = pairs[base_idx + 3] as usize;

        unsafe {
            let base = self.map.as_ptr();
            let name = if name_len > 0 {
                let ptr = base.add(self.strings_offset + name_offset);
                let bytes = std::slice::from_raw_parts(ptr, name_len);
                std::str::from_utf8_unchecked(bytes)
            } else {
                ""
            };
            let version = if ver_len > 0 {
                let ptr = base.add(self.strings_offset + ver_offset);
                let bytes = std::slice::from_raw_parts(ptr, ver_len);
                std::str::from_utf8_unchecked(bytes)
            } else {
                ""
            };
            CacheEntry { name, version, integrity: "", data: &[] }
        }
    }

    fn write_entry(&mut self, idx: usize, entry: &CacheEntry<'_>) -> Result<(), CacheError> {
        let hash = fxhash::hash64(entry.name);
        let map_len = self.map.len();

        let name_bytes = entry.name.as_bytes();
        let version_bytes = entry.version.as_bytes();
        let str_data_offset = self.header.string_table_size as usize;

        let name_end = str_data_offset + name_bytes.len();
        let version_end = name_end + version_bytes.len();

        if self.strings_offset + version_end > map_len {
            warn!("cache write out of bounds: {} > {}", self.strings_offset + version_end, map_len);
            return Err(CacheError::CacheFull);
        }

        unsafe {
            let base = self.map.as_ptr() as *mut u8;

            let pair_base = base.add(self.pairs_offset) as *mut u32;
            let pair_idx = idx * PAIRS_PER_ENTRY as usize;

            pair_base.add(pair_idx).write(str_data_offset as u32);
            pair_base.add(pair_idx + 1).write(name_bytes.len() as u32);
            pair_base.add(pair_idx + 2).write(name_end as u32);
            pair_base.add(pair_idx + 3).write(version_bytes.len() as u32);

            let data_ptr = base.add(self.strings_offset + str_data_offset);
            std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), data_ptr, name_bytes.len());
            std::ptr::copy_nonoverlapping(version_bytes.as_ptr(), data_ptr.add(name_bytes.len()), version_bytes.len());

            let hash_ptr = base.add(self.hashes_offset) as *mut u64;
            hash_ptr.add(idx).write(hash);
        }

        self.header.string_table_size += (name_bytes.len() + version_bytes.len()) as u32;
        self.header.hash_table_size = (self.entry_count.load(Ordering::Acquire)) * HASH_BYTES;

        Ok(())
    }

    fn update_header(&self) {
        unsafe {
            let base = self.map.as_ptr() as *mut u8;
            let count = self.entry_count.load(Ordering::Acquire);
            let header_bytes = CacheHeader {
                magic: *CACHE_MAGIC,
                version: CACHE_VERSION,
                entry_count: count,
                string_table_size: self.header.string_table_size,
                hash_table_size: self.header.hash_table_size,
                metadata_size: self.header.metadata_size,
                reserved: [0u8; 4],
            }
            .to_bytes();
            std::ptr::copy_nonoverlapping(header_bytes.as_ptr(), base, HEADER_SIZE as usize);
        }
    }
}

impl Drop for MemMapCache {
    fn drop(&mut self) {
        if let Err(e) = self.flush() {
            warn!("failed to flush cache on drop: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_cache_create_and_open() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.mgpm_cache");
        let cache = MemMapCache::open(&path).unwrap();
        assert_eq!(cache.entry_count(), 0);
    }

    #[test]
    fn test_cache_insert_and_get() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.mgpm_cache");
        let mut cache = MemMapCache::open(&path).unwrap();

        let entry = CacheEntry {
            name: "lodash",
            version: "4.17.21",
            integrity: "",
            data: &[],
        };
        cache.insert(entry).unwrap();
        cache.flush().unwrap();

        let result = cache.get("lodash");
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "lodash");
        assert_eq!(result.unwrap().version, "4.17.21");
    }

    #[test]
    fn test_cache_get_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.mgpm_cache");
        let cache = MemMapCache::open(&path).unwrap();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_cache_multiple_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.mgpm_cache");
        let mut cache = MemMapCache::open(&path).unwrap();

        for i in 0..10 {
            let name = format!("pkg-{i}");
            let version = format!("1.{i}.0");
            let entry = CacheEntry {
                name: &name,
                version: &version,
                integrity: "",
                data: &[],
            };
            cache.insert(entry).unwrap();
        }
        assert_eq!(cache.entry_count(), 10);

        for i in 0..10 {
            let name = format!("pkg-{i}");
            let entry = cache.get(&name).unwrap();
            assert_eq!(entry.version, format!("1.{i}.0"));
        }
    }
}
