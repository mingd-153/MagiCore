use std::fs;
use std::path::Path;

use super::store::StoreError;

/// Check if any ancestor of the path is a symlink (potential TOCTOU attack).
/// The path must exist (canonicalize is called on it).
pub fn check_symlink_ancestors(path: &Path) -> Result<(), StoreError> {
    let canonical = path.canonicalize().map_err(|e| StoreError::Io {
        path: path.to_path_buf(),
        msg: format!("cannot canonicalize path: {e}"),
    })?;

    // Walk ancestors and check for symlinks
    let mut current = Some(canonical.as_path());
    while let Some(p) = current {
        if let Ok(meta) = fs::symlink_metadata(p) {
            if meta.file_type().is_symlink() {
                return Err(StoreError::Io {
                    path: p.to_path_buf(),
                    msg: "symlink detected in path ancestry".to_string(),
                });
            }
        }
        current = p.parent();
    }

    Ok(())
}

/// Check that the path does not contain `..` components (path traversal).
pub fn check_path_traversal(path: &Path) -> Result<(), StoreError> {
    for component in path.components() {
        if let std::path::Component::ParentDir = component {
            return Err(StoreError::Io {
                path: path.to_path_buf(),
                msg: "path contains parent directory traversal (..)".to_string(),
            });
        }
    }
    Ok(())
}

