//! Content-addressable file store (CAFS)
//!
//! Stores files content-addressed by their hash.
//! Supports reflink (copy-on-write), hardlink, and copy import methods.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tracing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    SHA256,
}

impl HashAlgorithm {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SHA256 => "sha256",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportMethod {
    Reflink,
    Hardlink,
    Copy,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub hash: String,
    pub size: u64,
    pub ref_count: u32,
    pub executable: bool,
    pub import_method: ImportMethod,
    pub created_at: u64,
}

#[derive(Debug, Clone)]
pub struct PackageEntry {
    pub package_id: String,
    pub name: String,
    pub version: String,
    pub metadata_json: String,
    pub file_hashes: Vec<String>,
}

pub struct ContentStore {
    root: PathBuf,
    algo: HashAlgorithm,
    index: RwLock<StoreIndex>,
}

struct StoreIndex {
    files: HashMap<String, FileEntry>,
    #[allow(dead_code)]
    packages: HashMap<String, PackageEntry>,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("IO error: {path}: {msg}")]
    Io { path: PathBuf, msg: String },
    #[error("cross-device link: {path}")]
    CrossDevice { path: PathBuf },
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("integrity check failed for {0}")]
    IntegrityCheck(String),
}

impl ContentStore {
    pub fn new(root: PathBuf) -> io::Result<Self> {
        let store = Self {
            root: root.clone(),
            algo: HashAlgorithm::SHA256,
            index: RwLock::new(StoreIndex {
                files: HashMap::new(),
                packages: HashMap::new(),
            }),
        };
        store.ensure_dirs()?;
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn files_dir(&self) -> PathBuf {
        self.root.join("files")
    }

    fn algo_dir(&self) -> PathBuf {
        self.files_dir().join(self.algo.as_str())
    }

    fn exec_dir(&self) -> PathBuf {
        self.root.join("exec")
    }

    fn hash_path(&self, hash: &str) -> PathBuf {
        let first2 = &hash[..2];
        self.algo_dir().join(first2).join(hash)
    }

    fn ensure_dirs(&self) -> io::Result<()> {
        fs::create_dir_all(self.algo_dir())?;
        fs::create_dir_all(self.exec_dir())?;
        Ok(())
    }

    pub fn hash_file<P: AsRef<Path>>(&self, path: P) -> io::Result<String> {
        let mut file = File::open(path.as_ref())?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hex::encode(hasher.finalize()))
    }

    pub fn hash_bytes(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    pub fn import_file<P: AsRef<Path>>(&self, src: P) -> io::Result<(String, ImportMethod)> {
        let src = src.as_ref();
        let hash = self.hash_file(src)?;
        let dst = self.hash_path(&hash);
        
        if dst.exists() {
            self.inc_ref(&hash)?;
            let method = self.detect_import_method(&dst);
            return Ok((hash, method));
        }

        let method = self.import_with_method(src, &dst)?;
        
        let meta = fs::metadata(src)?;
        let entry = FileEntry {
            hash: hash.clone(),
            size: meta.len(),
            ref_count: 1,
            executable: is_executable(&meta),
            import_method: method,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        {
            let mut index = self.index.write().unwrap();
            index.files.insert(hash.clone(), entry);
        }

        Ok((hash, method))
    }

    fn record_import(&self, hash: &str, src: &Path, method: ImportMethod) -> io::Result<()> {
        let meta = fs::metadata(src)?;
        let entry = FileEntry {
            hash: hash.to_string(),
            size: meta.len(),
            ref_count: 1,
            executable: is_executable(&meta),
            import_method: method,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        let mut index = self.index.write().unwrap();
        index.files.insert(hash.to_string(), entry);
        Ok(())
    }

    /// Imports a file with fallback through reflink → hardlink → copy,
    /// logging cross-device link errors and falling through to copy.
    pub fn import_file_fallback<P: AsRef<Path>>(&self, src: P) -> io::Result<(String, ImportMethod)> {
        let src = src.as_ref();
        let hash = self.hash_file(src)?;
        let dst = self.hash_path(&hash);

        if dst.exists() {
            self.inc_ref(&hash)?;
            let method = self.detect_import_method(&dst);
            return Ok((hash, method));
        }

        let parent = dst.parent().unwrap();
        fs::create_dir_all(parent)?;

        if let Some(method) = self.try_reflink(src, &dst) {
            self.record_import(&hash, src, method)?;
            return Ok((hash, method));
        }

        match self.try_hardlink(src, &dst) {
            Ok(Some(method)) => {
                return Ok((hash, method));
            }
            Err(StoreError::CrossDevice { path }) => {
                tracing::warn!("cross-device link for '{}', falling back to copy", path.display());
            }
            _ => {}
        }

        self.copy_file(src, &dst)?;
        self.record_import(&hash, src, ImportMethod::Copy)?;
        Ok((hash, ImportMethod::Copy))
    }

    /// Garbage-collects the store by removing unreferenced and orphaned files.
    /// Returns the number of files removed.
    pub fn gc(&self) -> io::Result<usize> {
        let mut removed = 0;

        let zero_ref_hashes: Vec<String> = {
            let index = self.index.read().unwrap();
            index.files.iter()
                .filter(|(_, entry)| entry.ref_count == 0)
                .map(|(hash, _)| hash.clone())
                .collect()
        };

        for hash in &zero_ref_hashes {
            let path = self.hash_path(hash);
            if path.exists() {
                fs::remove_file(&path)?;
            }
            let mut index = self.index.write().unwrap();
            index.files.remove(hash);
            removed += 1;
        }

        let index_hashes: Vec<String> = {
            let index = self.index.read().unwrap();
            index.files.keys().cloned().collect()
        };

        if let Ok(entries) = fs::read_dir(self.algo_dir()) {
            for entry in entries.flatten() {
                let first2_path = entry.path();
                if first2_path.is_dir() {
                    if let Ok(file_entries) = fs::read_dir(&first2_path) {
                        for file_entry in file_entries.flatten() {
                            let path = file_entry.path();
                            if path.is_file() {
                                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                                    if !index_hashes.contains(&filename.to_string()) {
                                        let _ = fs::remove_file(&path);
                                        removed += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(removed)
    }

    fn import_with_method(&self, src: &Path, dst: &Path) -> io::Result<ImportMethod> {
        let parent = dst.parent().unwrap();
        fs::create_dir_all(parent)?;

        if let Some(method) = self.try_reflink(src, dst) {
            return Ok(method);
        }

        match self.try_hardlink(src, dst) {
            Ok(Some(method)) => return Ok(method),
            Err(StoreError::CrossDevice { path }) => {
                tracing::warn!("cross-device link for '{}', falling back to copy", path.display());
            }
            _ => {}
        }

        self.copy_file(src, dst)?;
        Ok(ImportMethod::Copy)
    }

    #[cfg(target_os = "macos")]
    fn try_reflink(&self, src: &Path, dst: &Path) -> Option<ImportMethod> {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let src_c = CString::new(src.as_os_str().as_bytes()).ok()?;
        let dst_c = CString::new(dst.as_os_str().as_bytes()).ok()?;

        let ret = unsafe {
            libc::clonefile(src_c.as_ptr(), dst_c.as_ptr(), 0)
        };

        if ret == 0 {
            Some(ImportMethod::Reflink)
        } else {
            None
        }
    }

    #[cfg(target_os = "linux")]
    fn try_reflink(&self, src: &Path, dst: &Path) -> Option<ImportMethod> {
        use std::os::unix::fs::copy_file_range;
        
        let src_file = File::open(src).ok()?;
        let dst_file = File::create(dst).ok()?;
        
        match copy_file_range(src_file, None, dst_file, None, u64::MAX) {
            Ok(_) => Some(ImportMethod::Reflink),
            Err(_) => None,
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn try_reflink(&self, _src: &Path, _dst: &Path) -> Option<ImportMethod> {
        None
    }

    fn try_hardlink(&self, src: &Path, dst: &Path) -> Result<Option<ImportMethod>, StoreError> {
        match fs::hard_link(src, dst) {
            Ok(_) => {
                let src_meta = fs::metadata(src).ok();
                let entry = FileEntry {
                    hash: self.hash_file(dst).map_err(|e| StoreError::Io { path: dst.to_path_buf(), msg: e.to_string() })?,
                    size: src_meta.as_ref().map(|m| m.len()).unwrap_or(0),
                    ref_count: 1,
                    executable: src_meta.map(|m| is_executable(&m)).unwrap_or(false),
                    import_method: ImportMethod::Hardlink,
                    created_at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                };
                let mut index = self.index.write().unwrap();
                index.files.insert(entry.hash.clone(), entry);
                Ok(Some(ImportMethod::Hardlink))
            }
            Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
                Err(StoreError::CrossDevice { path: dst.to_path_buf() })
            }
            Err(_) => Ok(None),
        }
    }

    fn copy_file(&self, src: &Path, dst: &Path) -> io::Result<()> {
        fs::copy(src, dst)?;
        Ok(())
    }

    pub fn has_file(&self, hash: &str) -> bool {
        self.hash_path(hash).exists()
    }

    pub fn get_file(&self, hash: &str) -> io::Result<PathBuf> {
        let path = self.hash_path(hash);
        if path.exists() {
            Ok(path)
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("file not found: {}", hash),
            ))
        }
    }

    pub fn inc_ref(&self, hash: &str) -> io::Result<()> {
        let mut index = self.index.write().unwrap();
        if let Some(entry) = index.files.get_mut(hash) {
            entry.ref_count += 1;
        }
        Ok(())
    }

    pub fn dec_ref(&self, hash: &str) -> io::Result<()> {
        let mut index = self.index.write().unwrap();
        if let Some(entry) = index.files.get_mut(hash) {
            entry.ref_count = entry.ref_count.saturating_sub(1);
        }
        Ok(())
    }

    pub fn get_ref_count(&self, hash: &str) -> u32 {
        let index = self.index.read().unwrap();
        index.files.get(hash).map(|e| e.ref_count).unwrap_or(0)
    }

    pub fn delete_file(&self, hash: &str) -> io::Result<()> {
        let path = self.hash_path(hash);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        let mut index = self.index.write().unwrap();
        index.files.remove(hash);
        Ok(())
    }

    pub fn verify_integrity(&self, hash: &str, path: &Path) -> io::Result<bool> {
        let computed = self.hash_file(path)?;
        if computed == hash {
            Ok(true)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("hash mismatch: {} vs {}", hash, computed),
            ))
        }
    }

    pub fn list_files(&self) -> Vec<FileEntry> {
        let index = self.index.read().unwrap();
        index.files.values().cloned().collect()
    }

    pub fn file_count(&self) -> usize {
        let index = self.index.read().unwrap();
        index.files.len()
    }

    pub fn total_size(&self) -> u64 {
        let index = self.index.read().unwrap();
        index.files.values().map(|e| e.size).sum()
    }

    #[cfg(target_os = "macos")]
    fn detect_import_method(&self, path: &Path) -> ImportMethod {
        if let Ok(meta) = fs::metadata(path) {
            if meta.file_type().is_symlink() {
                return ImportMethod::Copy;
            }
        }
        ImportMethod::Hardlink
    }

    #[cfg(not(target_os = "macos"))]
    fn detect_import_method(&self, _path: &Path) -> ImportMethod {
        ImportMethod::Hardlink
    }
}

fn is_executable(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        (mode & 0o111) != 0
    }
    #[cfg(not(unix))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_hash_bytes() {
        let store = ContentStore::new(tempdir().unwrap().path().to_path_buf()).unwrap();
        let hash = store.hash_bytes(b"hello world");
        assert_eq!(hash.len(), 64);
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_import_file_new() {
        let temp = tempdir().unwrap();
        let store = ContentStore::new(temp.path().to_path_buf()).unwrap();
        
        let src = temp.path().join("source.txt");
        fs::write(&src, "hello world").unwrap();
        
        let (hash, method) = store.import_file(&src).unwrap();
        assert!(store.has_file(&hash));
        assert_eq!(store.get_ref_count(&hash), 1);
        assert!(matches!(method, ImportMethod::Copy | ImportMethod::Hardlink | ImportMethod::Reflink));
    }

    #[test]
    fn test_import_file_deduplication() {
        let temp = tempdir().unwrap();
        let store = ContentStore::new(temp.path().to_path_buf()).unwrap();
        
        let src1 = temp.path().join("source1.txt");
        let src2 = temp.path().join("source2.txt");
        fs::write(&src1, "same content").unwrap();
        fs::write(&src2, "same content").unwrap();
        
        let (hash1, _) = store.import_file(&src1).unwrap();
        let (hash2, _) = store.import_file(&src2).unwrap();
        
        assert_eq!(hash1, hash2);
        assert_eq!(store.get_ref_count(&hash1), 2);
    }

    #[test]
    fn test_delete_file() {
        let temp = tempdir().unwrap();
        let store = ContentStore::new(temp.path().to_path_buf()).unwrap();
        
        let src = temp.path().join("source.txt");
        fs::write(&src, "hello").unwrap();
        
        let (hash, _) = store.import_file(&src).unwrap();
        assert!(store.has_file(&hash));
        
        store.delete_file(&hash).unwrap();
        assert!(!store.has_file(&hash));
    }

    #[test]
    fn test_gc_removes_zero_ref() {
        let temp = tempdir().unwrap();
        let store = ContentStore::new(temp.path().to_path_buf()).unwrap();

        let src = temp.path().join("source.txt");
        fs::write(&src, "gc test").unwrap();

        let (hash, _) = store.import_file(&src).unwrap();
        assert_eq!(store.get_ref_count(&hash), 1);

        store.dec_ref(&hash).unwrap();
        assert_eq!(store.get_ref_count(&hash), 0);

        let removed = store.gc().unwrap();
        assert!(removed >= 1);
        assert!(!store.has_file(&hash));
    }

    #[test]
    fn test_gc_removes_orphaned() {
        let temp = tempdir().unwrap();
        let store = ContentStore::new(temp.path().to_path_buf()).unwrap();

        let orphan_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let orphan_path = store.hash_path(orphan_hash);
        fs::create_dir_all(orphan_path.parent().unwrap()).unwrap();
        fs::write(&orphan_path, "orphan data").unwrap();

        let removed = store.gc().unwrap();
        assert!(removed >= 1);
        assert!(!orphan_path.exists());
    }

    #[test]
    fn test_verify_integrity() {
        let temp = tempdir().unwrap();
        let store = ContentStore::new(temp.path().to_path_buf()).unwrap();
        
        let src = temp.path().join("source.txt");
        fs::write(&src, "hello world").unwrap();
        
        let (hash, _) = store.import_file(&src).unwrap();
        let path = store.get_file(&hash).unwrap();
        
        assert!(store.verify_integrity(&hash, &path).is_ok());
    }
}