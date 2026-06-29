use std::fs;
use std::io::{self, Read, Write, Seek};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use sha2::{Digest, Sha256};

use super::index::{PackageInfo, StoreError, StoreIndex};

#[derive(Debug, Clone)]
pub struct IntegrityHash {
    pub hash: String,
    pub shard: String,
    pub filename: String,
    pub is_executable: bool,
}

impl IntegrityHash {
    pub fn from_bytes(data: &[u8], executable: bool) -> Self {
        let hash = hex::encode(Sha256::digest(data));
        let shard = hash[..2].to_string();
        let filename = if executable {
            format!("{}-exec", hash)
        } else {
            hash.clone()
        };
        Self {
            hash,
            shard,
            filename,
            is_executable: executable,
        }
    }

    pub fn cas_path(&self, cas_root: &Path) -> PathBuf {
        cas_root.join(&self.shard).join(&self.filename)
    }
}

pub struct ContentStore {
    index: Box<dyn StoreIndex>,
    cas_path: PathBuf,
}

pub struct TarballEntry {
    pub path: String,
    pub data: Vec<u8>,
    pub executable: bool,
}

impl ContentStore {
    pub fn new(index: Box<dyn StoreIndex>, cas_path: PathBuf) -> io::Result<Self> {
        // Validate CAS root is not a symlink
        if cas_path.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "CAS root path is a symlink",
            ));
        }

        let store = Self { index, cas_path };
        store.ensure_dirs()?;

        // Set restrictive permissions on CAS root (owner-only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&store.cas_path)?.permissions();
            perms.set_mode(0o700);
            fs::set_permissions(&store.cas_path, perms)?;
        }

        Ok(store)
    }

    pub fn cas_path(&self) -> &Path {
        &self.cas_path
    }

    pub fn index(&self) -> &dyn StoreIndex {
        &*self.index
    }

    fn ensure_dirs(&self) -> io::Result<()> {
        for i in 0..256u16 {
            let shard = format!("{:02x}", i);
            let dir = self.cas_path.join(&shard);
            if !dir.exists() {
                fs::create_dir_all(&dir)?;
            }
        }
        Ok(())
    }

    fn check_symlink_in_cas(&self, dest: &Path) -> Result<(), StoreError> {
        let relative = dest.strip_prefix(&self.cas_path).map_err(|_| {
            StoreError::Io {
                path: dest.to_path_buf(),
                msg: "path outside CAS root".to_string(),
            }
        })?;
        for ancestor in relative.ancestors() {
            let full = self.cas_path.join(ancestor);
            if full.is_symlink() {
                return Err(StoreError::Io {
                    path: full,
                    msg: "symlink detected in CAS path".to_string(),
                });
            }
        }
        Ok(())
    }

    fn check_symlink_ancestors(path: &Path) -> Result<(), StoreError> {
        // Check if path itself is a symlink
        if path.is_symlink() {
            return Err(StoreError::Io {
                path: path.to_path_buf(),
                msg: "destination path is a symlink".to_string(),
            });
        }

        // Check parent directory only (not all ancestors up to root)
        // This avoids false positives on system symlinks like /var -> /private/var on macOS
        if let Some(parent) = path.parent() {
            if parent.is_symlink() {
                return Err(StoreError::Io {
                    path: parent.to_path_buf(),
                    msg: "destination parent is a symlink".to_string(),
                });
            }
        }

        Ok(())
    }

    pub fn import_file(&self, path: &Path) -> Result<IntegrityHash, StoreError> {
        // Check source path for symlinks (prevents importing unintended files via symlinks)
        Self::check_symlink_ancestors(path)?;

        let metadata = fs::metadata(path).map_err(|e| StoreError::Io {
            path: path.to_path_buf(),
            msg: e.to_string(),
        })?;
        let is_exec = is_executable(&metadata);

        let mut data = Vec::with_capacity(metadata.len() as usize + 1);
        fs::File::open(path)?.read_to_end(&mut data)?;

        self.import_bytes(&data, is_exec)
    }

    pub fn import_bytes(&self, data: &[u8], is_executable: bool) -> Result<IntegrityHash, StoreError> {
        let hash = IntegrityHash::from_bytes(data, is_executable);
        let cas_path = hash.cas_path(&self.cas_path);

        match self.try_create_write(&cas_path, data, is_executable) {
            Ok(true) => {
                let (name, version) = extract_name_version(&cas_path);

                let info = PackageInfo {
                    name,
                    version,
                    integrity: hash.hash.clone(),
                    shard: hash.shard.clone(),
                    filename: hash.filename.clone(),
                    is_executable: hash.is_executable,
                    manifest_json: None,
                    metadata: None,
                    size_bytes: data.len() as u64,
                    compressed_size_bytes: 0,
                    created_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                };
                self.index.add_package(&info)?;
            }
            Ok(false) => {}
            Err(e) => return Err(e),
        }

        Ok(hash)
    }

    fn try_create_write(&self, dest: &Path, data: &[u8], executable: bool) -> Result<bool, StoreError> {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| StoreError::Io {
                path: parent.to_path_buf(),
                msg: e.to_string(),
            })?;
        }

        self.check_symlink_in_cas(dest)?;

        match fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(dest)
        {
            Ok(file) => {
                write_all_verify_and_set_perms(file, dest, data, executable)?;
                Ok(true)
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                self.check_symlink_in_cas(dest)?;
                Ok(false)
            }
            Err(e) => Err(StoreError::Io {
                path: dest.to_path_buf(),
                msg: e.to_string(),
            }),
        }
    }

    pub fn export_to(&self, hash: &IntegrityHash, dest: &Path) -> Result<(), StoreError> {
        let src = hash.cas_path(&self.cas_path);

        if !src.exists() {
            return Err(StoreError::NotFound(hash.hash.clone()));
        }

        self.check_symlink_in_cas(&src)?;

        // Check destination path for symlinks (prevents symlink attacks on export)
        Self::check_symlink_ancestors(dest)?;

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| StoreError::Io {
                path: parent.to_path_buf(),
                msg: e.to_string(),
            })?;
        }

        match fs::hard_link(&src, dest) {
            Ok(_) => {}
            Err(e) => {
                fs::copy(&src, dest).map_err(|ce| StoreError::Io {
                    path: dest.to_path_buf(),
                    msg: format!("hardlink failed ({}), copy also failed: {}", e, ce),
                })?;
            }
        }

        if !self.verify(hash)? {
            let _ = fs::remove_file(dest);
            return Err(StoreError::IntegrityCheck(hash.hash.clone()));
        }

        Ok(())
    }

    pub fn verify(&self, hash: &IntegrityHash) -> Result<bool, StoreError> {
        let cas_path = hash.cas_path(&self.cas_path);

        if !cas_path.exists() {
            return Ok(false);
        }

        let mut file = fs::File::open(&cas_path).map_err(|e| StoreError::Io {
            path: cas_path.clone(),
            msg: e.to_string(),
        })?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = file.read(&mut buf).map_err(|e| StoreError::Io {
                path: cas_path.clone(),
                msg: e.to_string(),
            })?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let computed = hex::encode(hasher.finalize());

        Ok(computed == hash.hash)
    }

    pub fn contains(&self, hash: &IntegrityHash) -> Result<bool, StoreError> {
        if self.index.package_exists(&hash.hash)? {
            let cas_path = hash.cas_path(&self.cas_path);
            return Ok(cas_path.exists());
        }
        Ok(false)
    }

    pub fn remove(&self, hash: &IntegrityHash) -> Result<(), StoreError> {
        let cas_path = hash.cas_path(&self.cas_path);
        if cas_path.exists() {
            fs::remove_file(&cas_path)?;
        }
        self.index.delete_package(&hash.hash)?;
        Ok(())
    }

    pub fn import_tarball_entries(&self, entries: &[TarballEntry]) -> Result<Vec<IntegrityHash>, StoreError> {
        let mut hashes = Vec::with_capacity(entries.len());
        for entry in entries {
            let hash = self.import_bytes(&entry.data, entry.executable)?;
            hashes.push(hash);
        }
        Ok(hashes)
    }
}

fn write_all_verify_and_set_perms(mut writer: fs::File, dest: &Path, data: &[u8], executable: bool) -> Result<(), StoreError> {
    // Write data
    writer.write_all(data).map_err(|e| StoreError::Io {
        path: dest.to_path_buf(),
        msg: e.to_string(),
    })?;

    // Verify content using SAME file handle (no TOCTOU)
    writer.flush().map_err(|e| StoreError::Io {
        path: dest.to_path_buf(),
        msg: e.to_string(),
    })?;
    writer.seek(std::io::SeekFrom::Start(0)).map_err(|e| StoreError::Io {
        path: dest.to_path_buf(),
        msg: e.to_string(),
    })?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    let mut read_bytes = 0usize;
    loop {
        let n = writer.read(&mut buf).map_err(|e| StoreError::Io {
            path: dest.to_path_buf(),
            msg: e.to_string(),
        })?;
        if n == 0 {
            break;
        }
        read_bytes += n;
        hasher.update(&buf[..n]);
    }
    if read_bytes != data.len() {
        return Err(StoreError::IntegrityCheck("size mismatch after write".into()));
    }
    let computed = hex::encode(hasher.finalize());
    let expected = hex::encode(Sha256::digest(data));
    if computed != expected {
        return Err(StoreError::IntegrityCheck("hash mismatch after write".into()));
    }

    // Set executable bit if needed
    apply_executable_bit(&writer, dest, executable)?;

    Ok(())
}

fn apply_executable_bit(writer: &fs::File, dest: &Path, executable: bool) -> Result<(), StoreError> {
    if !executable {
        return Ok(());
    }
    set_executable_bit(writer, dest)
}

#[cfg(unix)]
fn set_executable_bit(writer: &fs::File, _dest: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::PermissionsExt;
    let meta = writer.metadata().map_err(|e| StoreError::Io {
        path: _dest.to_path_buf(),
        msg: e.to_string(),
    })?;
    let mode = meta.permissions().mode();
    let mut perms = meta.permissions();
    perms.set_mode(mode | 0o111);
    // Use the file handle to set permissions (no TOCTOU)
    writer.set_permissions(perms).map_err(|e| StoreError::Io {
        path: _dest.to_path_buf(),
        msg: e.to_string(),
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable_bit(_writer: &fs::File, _dest: &Path) -> Result<(), StoreError> {
    Ok(())
}

fn is_executable(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        let mode = meta.permissions().mode();
        (mode & 0o111) != 0
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn extract_name_version(_path: &Path) -> (String, String) {
    ("unknown".to_string(), "0.0.0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::sqlite::SqliteStore;
    use tempfile::tempdir;

    fn create_test_store() -> (ContentStore, tempfile::TempDir) {
        let cas_dir = tempdir().unwrap();
        let sqlite = SqliteStore::open_in_memory().unwrap();
        let store = ContentStore::new(Box::new(sqlite), cas_dir.path().to_path_buf()).unwrap();
        (store, cas_dir)
    }

    #[test]
    fn test_import_and_verify() {
        let (store, _dir) = create_test_store();
        let data = b"hello cas store";
        let hash = store.import_bytes(data, false).unwrap();
        assert_eq!(hash.hash.len(), 64);
        assert!(store.verify(&hash).unwrap());
    }

    #[test]
    fn test_import_file() {
        let (store, _dir) = create_test_store();
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, b"file content").unwrap();

        let hash = store.import_file(&file_path).unwrap();
        assert_eq!(hash.shard.len(), 2);
        assert!(store.contains(&hash).unwrap());
    }

    #[test]
    fn test_import_bytes_deduplication() {
        let (store, _dir) = create_test_store();
        let data = b"dedup content";
        let h1 = store.import_bytes(data, false).unwrap();
        let h2 = store.import_bytes(data, false).unwrap();
        assert_eq!(h1.hash, h2.hash);
    }

    #[test]
    fn test_export_and_verify() {
        let (store, _dir) = create_test_store();
        let data = b"export test data";
        let hash = store.import_bytes(data, false).unwrap();

        let temp = tempdir().unwrap();
        let dest = temp.path().join("exported.txt");
        store.export_to(&hash, &dest).unwrap();
        assert!(dest.exists());

        let exported = fs::read(&dest).unwrap();
        assert_eq!(exported, data);
    }

    #[test]
    fn test_executable_file() {
        let (store, _dir) = create_test_store();
        let data = b"#!/bin/bash\necho hello";
        let hash = store.import_bytes(data, true).unwrap();
        assert!(hash.is_executable);
        assert!(hash.filename.ends_with("-exec"));

        let cas_path = hash.cas_path(store.cas_path());
        assert!(cas_path.exists());

        #[cfg(unix)]
        {
            let meta = fs::metadata(&cas_path).unwrap();
            let mode = meta.permissions().mode();
            assert!(mode & 0o111 != 0, "executable bit should be set");
        }
    }

    #[test]
    fn test_contains() {
        let (store, _dir) = create_test_store();
        let data = b"contains test";
        let hash = store.import_bytes(data, false).unwrap();
        assert!(store.contains(&hash).unwrap());

        let fake = IntegrityHash::from_bytes(b"nonexistent", false);
        assert!(!store.contains(&fake).unwrap());
    }

    #[test]
    fn test_remove() {
        let (store, _dir) = create_test_store();
        let data = b"remove test";
        let hash = store.import_bytes(data, false).unwrap();
        assert!(store.contains(&hash).unwrap());

        store.remove(&hash).unwrap();
        assert!(!store.contains(&hash).unwrap());

        let cas_path = hash.cas_path(store.cas_path());
        assert!(!cas_path.exists());
    }

    #[test]
    fn test_export_nonexistent() {
        let (store, _dir) = create_test_store();
        let hash = IntegrityHash::from_bytes(b"ghost", false);
        let temp = tempdir().unwrap();
        let result = store.export_to(&hash, &temp.path().join("out.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_file() {
        let (store, _dir) = create_test_store();
        let hash = store.import_bytes(b"", false).unwrap();
        assert!(store.verify(&hash).unwrap());
    }

    #[test]
    fn test_tarball_batch_import() {
        let (store, _dir) = create_test_store();
        let entries = vec![
            TarballEntry {
                path: "package1/index.js".to_string(),
                data: b"console.log('hello')".to_vec(),
                executable: false,
            },
            TarballEntry {
                path: "package2/bin/cli.js".to_string(),
                data: b"#!/usr/bin/env node\nconsole.log('cli')".to_vec(),
                executable: true,
            },
        ];

        let hashes = store.import_tarball_entries(&entries).unwrap();
        assert_eq!(hashes.len(), 2);

        for h in &hashes {
            assert!(store.contains(h).unwrap());
        }
    }

    #[test]
    fn test_reimport_after_file_deleted() {
        let (store, _dir) = create_test_store();
        let data = b"reimport me";
        let hash = store.import_bytes(data, false).unwrap();

        let cas_path = hash.cas_path(store.cas_path());
        fs::remove_file(&cas_path).unwrap();
        assert!(!cas_path.exists());

        let h2 = store.import_bytes(data, false).unwrap();
        assert_eq!(hash.hash, h2.hash);
        assert!(cas_path.exists());
    }

    #[test]
    fn test_verify_fails_for_corrupted_file() {
        let (store, _dir) = create_test_store();
        let data = b"original content";
        let hash = store.import_bytes(data, false).unwrap();

        let cas_path = hash.cas_path(store.cas_path());
        fs::write(&cas_path, b"tampered content").unwrap();

        assert!(!store.verify(&hash).unwrap());
    }

    #[test]
    fn test_integrity_hash_from_bytes() {
        let hash = IntegrityHash::from_bytes(b"test", false);
        assert_eq!(hash.hash.len(), 64);
        assert_eq!(hash.shard.len(), 2);
        assert_eq!(hash.filename, hash.hash);
        assert!(!hash.is_executable);

        let exec_hash = IntegrityHash::from_bytes(b"test", true);
        assert!(exec_hash.filename.ends_with("-exec"));
        assert!(exec_hash.is_executable);
    }

    #[test]
    fn test_cas_path_layout() {
        let hash = IntegrityHash::from_bytes(b"hello", false);
        let root = Path::new("/cas");
        let path = hash.cas_path(root);
        assert_eq!(
            path.parent().unwrap().parent().unwrap(),
            root
        );
        assert_eq!(
            path.parent().unwrap().file_name().unwrap(),
            hash.shard.as_str()
        );
        assert_eq!(path.file_name().unwrap(), hash.filename.as_str());
    }

    #[test]
    fn test_ensure_dirs_creates_all_shards() {
        let cas_dir = tempdir().unwrap();
        let sqlite = SqliteStore::open_in_memory().unwrap();
        let store =
            ContentStore::new(Box::new(sqlite), cas_dir.path().to_path_buf()).unwrap();

        let mut count = 0;
        for entry in fs::read_dir(store.cas_path()).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_dir() {
                count += 1;
            }
        }
        assert_eq!(count, 256);
    }

    #[test]
    #[cfg(unix)]
    fn test_symlink_in_cas_path_rejected() {
        let (store, _dir) = create_test_store();
        let data = b"test data";

        let cas_path = store.cas_path().join("ab").join("deadbeef");
        let fake_symlink_dir = store.cas_path().join("ff");
        let _ = fs::remove_dir_all(&fake_symlink_dir);
        std::os::unix::fs::symlink("/etc", &fake_symlink_dir).unwrap();

        let result = store.import_bytes(data, false);
        assert!(
            result.is_ok(),
            "normal import should still work: {:?}",
            result
        );
    }
}
