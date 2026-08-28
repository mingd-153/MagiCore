use std::fs;
use std::path::Path;

use super::store::StoreError;

pub fn validate_cas_root(root: &Path) -> Result<(), StoreError> {
    if root.exists() {
        let meta = fs::metadata(root).map_err(|e| StoreError::Io {
            path: root.to_path_buf(),
            msg: e.to_string(),
        })?;
        if meta.file_type().is_symlink() {
            return Err(StoreError::Io {
                path: root.to_path_buf(),
                msg: "CAS root is a symlink".to_string(),
            });
        }
        if !meta.is_dir() {
            return Err(StoreError::Io {
                path: root.to_path_buf(),
                msg: "CAS root is not a directory".to_string(),
            });
        }
    }
    Ok(())
}

pub fn ensure_cas_dirs(root: &Path) -> Result<(), StoreError> {
    let dirs = [
        root.join("files").join("blake3"),
        root.join("compiled").join("blake3"),
    ];
    for dir in &dirs {
        fs::create_dir_all(dir).map_err(|e| StoreError::Io {
            path: dir.clone(),
            msg: e.to_string(),
        })?;
    }
    Ok(())
}

pub fn set_cas_root_permissions(root: &Path) -> Result<(), StoreError> {
    if !root.exists() {
        return Ok(());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(root, perms).map_err(|e| StoreError::Io {
            path: root.to_path_buf(),
            msg: e.to_string(),
        })?;
    }

    Ok(())
}

