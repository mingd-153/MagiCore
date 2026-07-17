use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::integrity::{IntegrityHash, TarballEntry};
use super::lifecycle::{ensure_cas_dirs, set_cas_root_permissions, validate_cas_root};
use super::security::check_symlink_ancestors;
use super::write::{
    stream_write_verify_and_set_perms, write_all_verify_and_set_perms, STREAM_THRESHOLD,
};
use crate::cas::integrity;

#[derive(Debug)]
pub enum StoreError {
    Io { path: PathBuf, msg: String },
    NotFound(String),
    HashMismatch { expected: String, actual: String },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, msg } => write!(f, "IO error at {}: {msg}", path.display()),
            Self::NotFound(hash) => write!(f, "content not found: {hash}"),
            Self::HashMismatch { expected, actual } => {
                write!(f, "hash mismatch: expected {expected}, got {actual}")
            }
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io {
            path: PathBuf::new(),
            msg: e.to_string(),
        }
    }
}

#[derive(Clone)]
pub struct ContentStore {
    root: PathBuf,
}

impl ContentStore {
    pub fn new(root: PathBuf) -> Result<Self, StoreError> {
        validate_cas_root(&root)?;
        ensure_cas_dirs(&root)?;
        set_cas_root_permissions(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn import_file(&self, src: &Path) -> Result<IntegrityHash, StoreError> {
        if src.is_symlink() {
            return Err(StoreError::Io {
                path: src.to_path_buf(),
                msg: "source path is a symlink".to_string(),
            });
        }

        let meta = fs::metadata(src)?;
        let is_exec = is_executable(src);
        let use_streaming = meta.len() as usize >= STREAM_THRESHOLD;

        let hash = if use_streaming {
            let file = fs::File::open(src)?;
            let reader = BufReader::new(file);
            let tmp = self.tmp_path("import-file");
            fs::create_dir_all(tmp.parent().expect("tmp path has parent"))?;
            let writer = fs::File::create_new(&tmp)?;
            let hash = stream_write_verify_and_set_perms(writer, &tmp, reader, is_exec)?;
            let dest = hash.cas_path(&self.root);

            if dest.exists() {
                fs::remove_file(&tmp)?;
                return Ok(hash);
            }

            fs::create_dir_all(dest.parent().expect("dest path has parent"))?;
            fs::rename(&tmp, &dest).map_err(|e| StoreError::Io {
                path: dest.clone(),
                msg: format!("move streamed file into CAS failed: {e}"),
            })?;
            hash
        } else {
            let data = fs::read(src)?;
            let hash = IntegrityHash::from_bytes(&data, is_exec);
            let dest = hash.cas_path(&self.root);
            if dest.exists() {
                return Ok(hash);
            }

            fs::create_dir_all(dest.parent().expect("dest path has parent"))?;
            let tmp = self.tmp_path("import-bytes");
            fs::create_dir_all(tmp.parent().expect("tmp path has parent"))?;
            let writer = fs::File::create(&tmp)?;
            write_all_verify_and_set_perms(writer, &tmp, &data, is_exec)?;
            fs::rename(&tmp, &dest)?;
            hash
        };

        Ok(hash)
    }

    pub fn import_bytes(&self, data: &[u8]) -> Result<IntegrityHash, StoreError> {
        self.import_bytes_with_exec(data, false)
    }

    pub fn import_bytes_with_exec(
        &self,
        data: &[u8],
        executable: bool,
    ) -> Result<IntegrityHash, StoreError> {
        let hash = IntegrityHash::from_bytes(data, executable);
        self.write_bytes_with_hash(data, &hash, executable)
    }

    pub fn import_bytes_with_hash(
        &self,
        data: &[u8],
        hash_hex: &str,
        executable: bool,
    ) -> Result<IntegrityHash, StoreError> {
        let hash = IntegrityHash::from_hash_str(hash_hex, executable);
        self.write_bytes_with_hash(data, &hash, executable)
    }

    fn write_bytes_with_hash(
        &self,
        data: &[u8],
        hash: &IntegrityHash,
        executable: bool,
    ) -> Result<IntegrityHash, StoreError> {
        let dest = hash.cas_path(&self.root);
        if dest.exists() {
            return Ok(hash.clone());
        }

        fs::create_dir_all(dest.parent().expect("dest path has parent"))?;
        let writer = fs::File::create_new(&dest)?;

        let actual = if data.len() >= STREAM_THRESHOLD {
            let cursor = std::io::Cursor::new(data);
            let reader = BufReader::new(cursor);
            stream_write_verify_and_set_perms(writer, &dest, reader, executable)
        } else {
            write_all_verify_and_set_perms(writer, &dest, data, executable)
        }?;

        if actual.hash != hash.hash {
            let _ = fs::remove_file(&dest);
            return Err(StoreError::HashMismatch {
                expected: hash.hash.clone(),
                actual: actual.hash,
            });
        }

        Ok(hash.clone())
    }

    pub fn export_to(&self, hash: &IntegrityHash, dest: &Path) -> Result<(), StoreError> {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
            // Check parent directory for symlinks (dest doesn't exist yet)
            check_symlink_ancestors(parent)?;
        }

        let src = hash.cas_path(&self.root);
        if !src.exists() {
            return Err(StoreError::NotFound(hash.hash.clone()));
        }

        // Verify source integrity before export
        let actual_hash = self.verify(&src)?;
        if actual_hash.hash != hash.hash {
            return Err(StoreError::HashMismatch {
                expected: hash.hash.clone(),
                actual: actual_hash.hash,
            });
        }

        if dest.exists() {
            let dest_hash = self.verify(dest)?;
            if dest_hash.hash == hash.hash {
                return Ok(());
            }
            return Err(StoreError::Io {
                path: dest.to_path_buf(),
                msg: "destination already exists".to_string(),
            });
        }

        if let Err(e) = fs::hard_link(&src, dest) {
            tracing::debug!("hardlink failed (cross-device?), falling back to copy: {e}");
            fs::copy(&src, dest)?;
        }

        // Verify destination matches source
        let dest_hash = self.verify(dest)?;
        if dest_hash.hash != hash.hash {
            let _ = fs::remove_file(dest);
            return Err(StoreError::HashMismatch {
                expected: hash.hash.clone(),
                actual: dest_hash.hash,
            });
        }

        Ok(())
    }

    pub fn verify(&self, path: &Path) -> Result<IntegrityHash, StoreError> {
        let meta = fs::metadata(path)?;
        if meta.file_type().is_symlink() {
            return Err(StoreError::Io {
                path: path.to_path_buf(),
                msg: "path is a symlink, refusing to verify".to_string(),
            });
        }
        if !meta.is_file() {
            return Err(StoreError::Io {
                path: path.to_path_buf(),
                msg: "path is not a regular file".to_string(),
            });
        }
        let data = fs::read(path)?;
        Ok(IntegrityHash::from_bytes(&data, false))
    }

    pub fn contains(&self, hash: &IntegrityHash) -> bool {
        hash.cas_path(&self.root).exists()
    }

    pub fn remove(&self, hash: &IntegrityHash) -> Result<(), StoreError> {
        let path = hash.cas_path(&self.root);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    pub fn import_tarball_entries(
        &self,
        entries: Vec<TarballEntry>,
    ) -> Result<Vec<IntegrityHash>, StoreError> {
        let mut imported = Vec::with_capacity(entries.len());
        for entry in entries {
            let hash = integrity::IntegrityHash::from_bytes(&entry.data, entry.executable);
            let dest = hash.cas_path(&self.root);

            if dest.exists() {
                imported.push(hash);
                continue;
            }

            fs::create_dir_all(dest.parent().expect("dest path has parent"))?;
            let writer = fs::File::create_new(&dest)?;
            write_all_verify_and_set_perms(writer, &dest, &entry.data, entry.executable)?;
            imported.push(hash);
        }
        Ok(imported)
    }

    fn tmp_path(&self, prefix: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tid = {
            let tid = std::thread::current().id();
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&tid, &mut hasher);
            std::hash::Hasher::finish(&hasher)
        };
        self.root.join("tmp").join(format!(
            "{prefix}-{}-{tid:x}-{nanos}-{count}",
            std::process::id()
        ))
    }
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            return (meta.permissions().mode() & 0o111) != 0;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_store_creation() {
        let dir = tempdir().unwrap();
        let store = ContentStore::new(dir.path().to_path_buf()).unwrap();
        assert!(store.root().exists());
    }

    #[test]
    fn test_import_bytes_and_contains() {
        let dir = tempdir().unwrap();
        let store = ContentStore::new(dir.path().to_path_buf()).unwrap();
        let hash = store.import_bytes(b"hello world").unwrap();
        assert!(store.contains(&hash));
    }

    #[test]
    fn test_import_file() {
        let dir = tempdir().unwrap();
        let store = ContentStore::new(dir.path().to_path_buf()).unwrap();
        let src = dir.path().join("source.txt");
        fs::write(&src, b"hello world").unwrap();
        let hash = store.import_file(&src).unwrap();
        assert!(store.contains(&hash));
    }

    #[test]
    fn test_import_large_file_uses_content_hash_path() {
        let dir = tempdir().unwrap();
        let store = ContentStore::new(dir.path().to_path_buf()).unwrap();
        let src = dir.path().join("large.bin");
        let data = vec![42u8; STREAM_THRESHOLD + 1];
        fs::write(&src, &data).unwrap();

        let hash = store.import_file(&src).unwrap();
        let expected = IntegrityHash::from_bytes(&data, false);

        assert_eq!(hash, expected);
        assert!(expected.cas_path(store.root()).exists());
    }

    #[test]
    fn test_import_bytes_with_hash_rejects_mismatch() {
        let dir = tempdir().unwrap();
        let store = ContentStore::new(dir.path().to_path_buf()).unwrap();
        let wrong = IntegrityHash::from_bytes(b"different", false);

        let err = store
            .import_bytes_with_hash(b"actual", &wrong.hash, false)
            .unwrap_err();

        assert!(matches!(err, StoreError::HashMismatch { .. }));
    }

    #[test]
    fn test_deduplication() {
        let dir = tempdir().unwrap();
        let store = ContentStore::new(dir.path().to_path_buf()).unwrap();
        let h1 = store.import_bytes(b"same content").unwrap();
        let h2 = store.import_bytes(b"same content").unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_export_and_verify() {
        let dir = tempdir().unwrap();
        let store = ContentStore::new(dir.path().to_path_buf()).unwrap();
        let hash = store.import_bytes(b"export test").unwrap();
        let dest = dir.path().join("exported.txt");
        store.export_to(&hash, &dest).unwrap();
        assert!(dest.exists());
        let content = fs::read_to_string(&dest).unwrap();
        assert_eq!(content, "export test");
    }

    #[test]
    fn test_remove() {
        let dir = tempdir().unwrap();
        let store = ContentStore::new(dir.path().to_path_buf()).unwrap();
        let hash = store.import_bytes(b"remove me").unwrap();
        assert!(store.contains(&hash));
        store.remove(&hash).unwrap();
        assert!(!store.contains(&hash));
    }
}
