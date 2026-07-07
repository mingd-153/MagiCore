use fxhash;
use memmap2::MmapMut;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use tracing::warn;

use crate::{
    binary::CacheHeader, CacheEntry, CacheError, CACHE_MAGIC, CACHE_VERSION, HEADER_SIZE,
    INITIAL_SIZE,
};

const PAIRS_PER_ENTRY: u32 = 8;
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
                std::ptr::copy_nonoverlapping(
                    header_bytes.as_ptr(),
                    map.as_ptr() as *mut u8,
                    HEADER_SIZE as usize,
                );
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
            std::slice::from_raw_parts(
                self.map.as_ptr().add(self.hashes_offset) as *const u64,
                count,
            )
        }
    }

    fn pairs_slice(&self) -> &[u32] {
        let count = self.entry_count.load(Ordering::Acquire) as usize;
        if count == 0 {
            return &[];
        }
        unsafe {
            let count_u32 = count * PAIRS_PER_ENTRY as usize;
            std::slice::from_raw_parts(
                self.map.as_ptr().add(self.pairs_offset) as *const u32,
                count_u32,
            )
        }
    }

    fn read_entry<'a>(&'a self, idx: usize) -> CacheEntry<'a> {
        let pairs = self.pairs_slice();
        let base_idx = idx * PAIRS_PER_ENTRY as usize;

        let name_offset = pairs[base_idx] as usize;
        let name_len = pairs[base_idx + 1] as usize;
        let ver_offset = pairs[base_idx + 2] as usize;
        let ver_len = pairs[base_idx + 3] as usize;
        let int_offset = pairs[base_idx + 4] as usize;
        let int_len = pairs[base_idx + 5] as usize;
        let data_offset = pairs[base_idx + 6] as usize;
        let data_len = pairs[base_idx + 7] as usize;

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
            let integrity = if int_len > 0 {
                let ptr = base.add(self.strings_offset + int_offset);
                let bytes = std::slice::from_raw_parts(ptr, int_len);
                std::str::from_utf8_unchecked(bytes)
            } else {
                ""
            };
            let data = if data_len > 0 {
                let ptr = base.add(self.strings_offset + data_offset);
                std::slice::from_raw_parts(ptr, data_len)
            } else {
                &[]
            };
            CacheEntry {
                name,
                version,
                integrity,
                data,
            }
        }
    }

    fn write_entry(&mut self, idx: usize, entry: &CacheEntry<'_>) -> Result<(), CacheError> {
        let hash = fxhash::hash64(entry.name);
        let map_len = self.map.len();

        let name_bytes = entry.name.as_bytes();
        let version_bytes = entry.version.as_bytes();
        let integrity_bytes = entry.integrity.as_bytes();
        let data_bytes = entry.data;

        let str_offset = self.header.string_table_size as usize;
        let name_off = str_offset;
        let ver_off = name_off + name_bytes.len();
        let int_off = ver_off + version_bytes.len();
        let data_off = int_off + integrity_bytes.len();
        let total_len = data_off + data_bytes.len();

        let strings_end = self.strings_offset + total_len;
        if strings_end > map_len {
            warn!("cache write out of bounds: {strings_end} > {map_len}");
            return Err(CacheError::CacheFull);
        }

        unsafe {
            let base = self.map.as_ptr() as *mut u8;

            let pair_base = base.add(self.pairs_offset) as *mut u32;
            let pi = idx * PAIRS_PER_ENTRY as usize;

            pair_base.add(pi).write(name_off as u32);
            pair_base.add(pi + 1).write(name_bytes.len() as u32);
            pair_base.add(pi + 2).write(ver_off as u32);
            pair_base.add(pi + 3).write(version_bytes.len() as u32);
            pair_base.add(pi + 4).write(int_off as u32);
            pair_base.add(pi + 5).write(integrity_bytes.len() as u32);
            pair_base.add(pi + 6).write(data_off as u32);
            pair_base.add(pi + 7).write(data_bytes.len() as u32);

            let str_base = base.add(self.strings_offset + name_off);
            std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), str_base, name_bytes.len());
            if !version_bytes.is_empty() {
                std::ptr::copy_nonoverlapping(
                    version_bytes.as_ptr(),
                    str_base.add(name_bytes.len()),
                    version_bytes.len(),
                );
            }
            if !integrity_bytes.is_empty() {
                std::ptr::copy_nonoverlapping(
                    integrity_bytes.as_ptr(),
                    str_base.add(name_bytes.len() + version_bytes.len()),
                    integrity_bytes.len(),
                );
            }
            if !data_bytes.is_empty() {
                std::ptr::copy_nonoverlapping(
                    data_bytes.as_ptr(),
                    str_base.add(name_bytes.len() + version_bytes.len() + integrity_bytes.len()),
                    data_bytes.len(),
                );
            }

            let hash_ptr = base.add(self.hashes_offset) as *mut u64;
            hash_ptr.add(idx).write(hash);
        }

        self.header.string_table_size += total_len as u32;
        self.header.hash_table_size =
            self.entry_count.load(Ordering::Acquire) * HASH_BYTES;

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
        let path = dir.path().join("test.mg_cache");
        let cache = MemMapCache::open(&path).unwrap();
        assert_eq!(cache.entry_count(), 0);
    }

    #[test]
    fn test_cache_insert_and_get() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.mg_cache");
        let mut cache = MemMapCache::open(&path).unwrap();

        let entry = CacheEntry {
            name: "lodash",
            version: "4.17.21",
            integrity: "sha512-abc123",
            data: b"{\"name\":\"lodash\"}",
        };
        cache.insert(entry).unwrap();
        cache.flush().unwrap();

        let result = cache.get("lodash");
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.name, "lodash");
        assert_eq!(r.version, "4.17.21");
        assert_eq!(r.integrity, "sha512-abc123");
        assert_eq!(r.data, b"{\"name\":\"lodash\"}");
    }

    #[test]
    fn test_cache_get_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.mg_cache");
        let cache = MemMapCache::open(&path).unwrap();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_cache_multiple_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.mg_cache");
        let mut cache = MemMapCache::open(&path).unwrap();

        for i in 0..10 {
            let name = format!("pkg-{i}");
            let version = format!("1.{i}.0");
            let integrity = format!("sha256-{i}");
            let data = format!("{{\"name\":\"pkg-{i}\"}}");
            let entry = CacheEntry {
                name: &name,
                version: &version,
                integrity: &integrity,
                data: data.as_bytes(),
            };
            cache.insert(entry).unwrap();
        }
        assert_eq!(cache.entry_count(), 10);

        for i in 0..10 {
            let name = format!("pkg-{i}");
            let entry = cache.get(&name).unwrap();
            assert_eq!(entry.version, format!("1.{i}.0"));
            assert_eq!(entry.integrity, format!("sha256-{i}"));
            assert_eq!(entry.data, format!("{{\"name\":\"pkg-{i}\"}}").as_bytes());
        }
    }

    #[test]
    fn test_cache_empty_fields() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.mg_cache");
        let mut cache = MemMapCache::open(&path).unwrap();

        let entry = CacheEntry {
            name: "empty-test",
            version: "",
            integrity: "",
            data: &[],
        };
        cache.insert(entry).unwrap();

        let result = cache.get("empty-test").unwrap();
        assert_eq!(result.name, "empty-test");
        assert_eq!(result.version, "");
        assert_eq!(result.integrity, "");
        assert_eq!(result.data, &[]);
    }

    #[test]
    fn test_cache_flush_and_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.mg_cache");
        {
            let mut cache = MemMapCache::open(&path).unwrap();
            let entry = CacheEntry {
                name: "persist-test",
                version: "1.0.0",
                integrity: "sha256-persist",
                data: b"persistent-data",
            };
            cache.insert(entry).unwrap();
            cache.flush().unwrap();
        }
        {
            let cache = MemMapCache::open(&path).unwrap();
            let result = cache.get("persist-test").unwrap();
            assert_eq!(result.name, "persist-test");
            assert_eq!(result.version, "1.0.0");
            assert_eq!(result.integrity, "sha256-persist");
            assert_eq!(result.data, b"persistent-data");
        }
    }
}
