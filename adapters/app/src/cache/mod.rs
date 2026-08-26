//! `cache/mod.rs` — Cache management for app adapter.
//! Platform-specific cache directories for Flutter, Kotlin, Swift, CocoaPods.

pub mod metadata;
pub mod prune;

use crate::language::AppLanguage;
use mgc_types::{MgError, MgResult};
use std::path::PathBuf;

/// Get app adapter cache directory for language.
pub fn cache_dir(language: AppLanguage) -> MgResult<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| MgError::Other("cannot find home directory".to_string()))?;

    match language {
        AppLanguage::Flutter => Ok(home.join(".pub-cache")),
        AppLanguage::Kotlin => Ok(home.join(".gradle/caches")),
        AppLanguage::Swift => {
            if cfg!(target_os = "macos") {
                Ok(home.join("Library/Caches/org.swift.swiftpm"))
            } else {
                dirs::cache_dir()
                    .map(|d| d.join("org.swift.swiftpm"))
                    .ok_or_else(|| MgError::Other("cannot find cache directory".to_string()))
            }
        }
        AppLanguage::ReactNative => {
            // React Native uses npm cache (delegate to web)
            Ok(home.join(".npm"))
        }
        AppLanguage::ObjC => {
            if cfg!(target_os = "macos") {
                Ok(home.join("Library/Caches/CocoaPods"))
            } else {
                Ok(home.join(".cocoapods"))
            }
        }
        AppLanguage::Multi => Ok(home.join(".mgc-cache/app")),
    }
}

/// Clear cache for specific language.
pub fn clear_cache(language: AppLanguage) -> MgResult<()> {
    let dir = cache_dir(language)?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| MgError::Other(format!("failed to remove cache: {}", e)))?;
    }
    Ok(())
}

/// Get cache size for specific language.
pub fn cache_size(language: AppLanguage) -> MgResult<u64> {
    let dir = cache_dir(language)?;
    if !dir.exists() {
        return Ok(0);
    }
    dir_size(&dir)
}

fn dir_size(path: &std::path::Path) -> MgResult<u64> {
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
