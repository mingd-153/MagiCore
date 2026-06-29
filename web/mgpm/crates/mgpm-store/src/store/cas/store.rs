use std::fs;
use std::path::{Path, PathBuf};

use tracing;

use super::integrity::{IntegrityHash, TarballEntry};
use super::lifecycle::{ensure_cas_dirs, set_cas_root_permissions, validate_cas_root};
use super::security::check_symlink_ancestors;
use super::write::write_all_verify_and_set_perms;
use crate::store::index::{StoreError, StoreIndex};

pub struct ContentStore {
    root: PathBuf,
    index: Box<dyn StoreIndex>,
}

impl ContentStore {
    pub fn new(root: PathBuf, index: Box<dyn StoreIndex>) -> Result<Self, StoreError> {
        validate_cas_root(&root)?;
        ensure_cas_dirs(&root)?;
        set_cas_root_permissions(&root)?;
        Ok(Self { root, index })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn index(&self) -> &dyn StoreIndex {
        self.index.as_ref()
    }

    pub fn import_file(&self, src: &Path) -> Result<IntegrityHash, StoreError> {
        if src.is_symlink() {
            return Err(StoreError::Io {
                path: src.to_path_buf(),
                msg: "source path is a symlink".to_string(),
            });
        }

        let data = fs::read(src).map_err(|e| StoreError::Io {
            path: src.to_path_buf(),
            msg: e.to_string(),
        })?;

        let is_exec = is_executable(src);
        let hash = IntegrityHash::from_bytes(&data, is_exec);
        let dest = hash.cas_path(&self.root);

        if dest.exists() {
            return Ok(hash);
        }

        fs::create_dir_all(dest.parent().unwrap()).map_err(|e| StoreError::Io {
            path: dest.parent().unwrap().to_path_buf(),
            msg: e.to_string(),
        })?;

        let writer = fs::File::create_new(&dest).map_err(|e| StoreError::Io {
            path: dest.clone(),
            msg: e.to_string(),
        })?;

        write_all_verify_and_set_perms(writer, &dest, &data, is_exec)?;
        Ok(hash)
    }

    pub fn import_bytes(&self, data: &[u8]) -> Result<IntegrityHash, StoreError> {
        self.import_bytes_with_exec(data, false)
    }

    pub fn import_bytes_with_exec(&self, data: &[u8], executable: bool) -> Result<IntegrityHash, StoreError> {
        let hash = IntegrityHash::from_bytes(data, executable);
        let dest = hash.cas_path(&self.root);

        if dest.exists() {
            return Ok(hash);
        }

        fs::create_dir_all(dest.parent().unwrap()).map_err(|e| StoreError::Io {
            path: dest.parent().unwrap().to_path_buf(),
            msg: e.to_string(),
        })?;

        let writer = fs::File::create_new(&dest).map_err(|e| StoreError::Io {
            path: dest.clone(),
            msg: e.to_string(),
        })?;

        write_all_verify_and_set_perms(writer, &dest, data, executable)?;
        Ok(hash)
    }

    pub fn export_to(&self, hash: &IntegrityHash, dest: &Path) -> Result<(), StoreError> {
        check_symlink_ancestors(dest)?;

        if dest.exists() {
            return Err(StoreError::Io {
                path: dest.to_path_buf(),
                msg: "destination already exists".to_string(),
            });
        }

        let src = hash.cas_path(&self.root);
        if !src.exists() {
            return Err(StoreError::NotFound(hash.hash.clone()));
        }

        if let Err(e) = fs::hard_link(&src, dest) {
            tracing::debug!("hardlink failed (cross-device?), falling back to copy: {e}");
            fs::copy(&src, dest)?;
        }

        let actual_hash = self.verify(dest)?;
        if actual_hash.hash != hash.hash {
            let _ = fs::remove_file(dest);
            return Err(StoreError::HashMismatch {
                expected: hash.hash.clone(),
                actual: actual_hash.hash,
            });
        }

        Ok(())
    }

    pub fn verify(&self, path: &Path) -> Result<IntegrityHash, StoreError> {
        let data = fs::read(path).map_err(|e| StoreError::Io {
            path: path.to_path_buf(),
            msg: e.to_string(),
        })?;
        Ok(IntegrityHash::from_bytes(&data, false))
    }

    pub fn contains(&self, hash: &IntegrityHash) -> bool {
        hash.cas_path(&self.root).exists()
    }

    pub fn remove(&self, hash: &IntegrityHash) -> Result<(), StoreError> {
        let path = hash.cas_path(&self.root);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| StoreError::Io {
                path: path.clone(),
                msg: e.to_string(),
            })?;
        }
        Ok(())
    }

    pub fn import_tarball_entries(
        &self,
        entries: Vec<TarballEntry>,
    ) -> Result<Vec<IntegrityHash>, StoreError> {
        let mut imported = Vec::with_capacity(entries.len());
        for entry in entries {
            let hash = IntegrityHash::from_bytes(&entry.data, entry.executable);
            let dest = hash.cas_path(&self.root);

            if dest.exists() {
                imported.push(hash);
                continue;
            }

            fs::create_dir_all(dest.parent().unwrap()).map_err(|e| StoreError::Io {
                path: dest.parent().unwrap().to_path_buf(),
                msg: e.to_string(),
            })?;

            let writer = fs::File::create_new(&dest).map_err(|e| StoreError::Io {
                path: dest.clone(),
                msg: e.to_string(),
            })?;

            write_all_verify_and_set_perms(writer, &dest, &entry.data, entry.executable)?;
            imported.push(hash);
        }
        Ok(imported)
    }
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            let mode = meta.permissions().mode();
            return (mode & 0o111) != 0;
        }
    }
    false
}

impl std::fmt::Debug for ContentStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContentStore")
            .field("root", &self.root)
            .finish()
    }
}
