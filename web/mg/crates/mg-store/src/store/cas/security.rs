use std::path::Path;

use super::super::index::StoreError;

/// Checks for symlinks within CAS paths (relative to CAS root).
/// Used for CAS internal paths to prevent symlink attacks.
pub fn check_symlink_in_cas(cas_root: &Path, dest: &Path) -> Result<(), StoreError> {
    let relative = dest.strip_prefix(cas_root).map_err(|_| StoreError::Io {
        path: dest.to_path_buf(),
        msg: "path outside CAS root".to_string(),
    })?;
    for ancestor in relative.ancestors() {
        let full = cas_root.join(ancestor);
        if full.is_symlink() {
            return Err(StoreError::Io {
                path: full,
                msg: "symlink detected in CAS path".to_string(),
            });
        }
    }
    Ok(())
}

/// Checks for symlinks in arbitrary paths (export destinations, import sources).
/// Only checks the path itself and its parent directory.
/// Avoids checking all ancestors up to root to prevent false positives
/// on system symlinks like /var -> /private/var on macOS.
pub fn check_symlink_ancestors(path: &Path) -> Result<(), StoreError> {
    if path.is_symlink() {
        return Err(StoreError::Io {
            path: path.to_path_buf(),
            msg: "destination path is a symlink".to_string(),
        });
    }

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
