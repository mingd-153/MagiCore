//! `cache/mod.rs` — Cache management for lib adapter.
//! Mirrors web adapter cache pattern (metadata + prune).

pub mod metadata;
pub mod prune;

use mgc_types::{MgError, MgResult};
use std::path::{Path, PathBuf};

/// Get the MagiCore-owned cache directory for a library ecosystem.
/// Lấy cache do MagiCore sở hữu cho ecosystem thư viện.
pub fn cache_dir(language: &str) -> MgResult<PathBuf> {
    let globals = mgc_platform::paths::GlobalPaths::new()
        .map_err(|e| MgError::Other(format!("cannot resolve MagiCore paths: {e}")))?;

    match language {
        "rust" => Ok(globals.store.join("cargo")),
        "python" => Ok(globals.store.join("pypi")),
        "ts" | "typescript" => {
            // TypeScript cache is isolated under the MagiCore global cache.
            // Cache TypeScript được cách ly trong cache global của MagiCore.
            Ok(globals.cache.join("web"))
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
