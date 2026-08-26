//! `cache/mod.rs` — Cache management for lib adapter.
//! Mirrors web adapter cache pattern (metadata + prune).

pub mod metadata;
pub mod prune;

use mgc_types::{MgError, MgResult};
use std::path::{Path, PathBuf};

/// Get lib adapter cache directory.
/// Lấy thư mục cache của lib adapter.
///
/// - Rust: ~/.cargo/registry/cache
/// - Python: ~/.cache/pip or uv cache
/// - TypeScript: delegated to web adapter (~/.mgc-store/cache)
pub fn cache_dir(language: &str) -> MgResult<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| MgError::Other("cannot find home directory".to_string()))?;

    match language {
        "rust" => Ok(home.join(".cargo/registry/cache")),
        "python" => {
            // Prefer uv cache if available, fallback to pip cache
            if which::which("uv").is_ok() {
                Ok(home.join(".cache/uv"))
            } else {
                Ok(home.join(".cache/pip"))
            }
        }
        "ts" | "typescript" => {
            // TypeScript uses web adapter cache
            Ok(home.join(".mgc-store/cache"))
        }
        _ => Err(MgError::Other(format!(
            "unsupported language: {}",
            language
        ))),
    }
}

/// Clear cache for specific language.
/// Xóa cache cho ngôn ngữ cụ thể.
pub fn clear_cache(language: &str) -> MgResult<()> {
    let dir = cache_dir(language)?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| MgError::Other(format!("failed to remove cache: {}", e)))?;
    }
    Ok(())
}

/// Get cache size for specific language.
/// Lấy kích thước cache cho ngôn ngữ cụ thể.
pub fn cache_size(language: &str) -> MgResult<u64> {
    let dir = cache_dir(language)?;
    if !dir.exists() {
        return Ok(0);
    }
    dir_size(&dir)
}

/// Recursively calculate directory size.
/// Tính kích thước thư mục đệ quy.
fn dir_size(path: &Path) -> MgResult<u64> {
    let mut total = 0u64;
    for entry in
        std::fs::read_dir(path).map_err(|e| MgError::Other(format!("failed to read dir: {}", e)))?
    {
        let entry = entry.map_err(|e| MgError::Other(format!("failed to read entry: {}", e)))?;
        let metadata = entry
            .metadata()
            .map_err(|e| MgError::Other(format!("failed to read metadata: {}", e)))?;

        if metadata.is_dir() {
            total += dir_size(&entry.path())?;
        } else {
            total += metadata.len();
        }
    }
    Ok(total)
}
