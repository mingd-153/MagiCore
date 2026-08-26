//! `cache/prune.rs` — Cache pruning for lib adapter.
//! Removes old/unused cached packages (mirrors web cache_prune.rs).

use super::metadata::{metadata_path, CacheMetadata};
use mgc_types::{MgError, MgResult};
use std::time::{SystemTime, UNIX_EPOCH};

/// Prune strategy for cache cleanup.
/// Chiến lược prune cho dọn dẹp cache.
#[derive(Debug, Clone, Copy)]
pub enum PruneStrategy {
    /// Remove entries older than N days.
    /// Xóa entries cũ hơn N ngày.
    OlderThan(u64),

    /// Keep only N most recent entries.
    /// Chỉ giữ N entries gần nhất.
    KeepRecent(usize),

    /// Remove entries until cache size is below N bytes.
    /// Xóa entries cho đến khi cache size dưới N bytes.
    MaxSize(u64),
}

/// Prune cache for specific language.
/// Prune cache cho ngôn ngữ cụ thể.
pub fn prune_cache(language: &str, strategy: PruneStrategy) -> MgResult<u64> {
    let meta_path = metadata_path(language)?;
    let mut metadata = CacheMetadata::load(&meta_path)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut removed_bytes = 0u64;
    let mut to_remove = Vec::new();

    match strategy {
        PruneStrategy::OlderThan(days) => {
            let cutoff = now - (days * 24 * 60 * 60);

            for entry in metadata.entries() {
                if entry.cached_at < cutoff {
                    to_remove.push(entry.package_id.clone());
                    removed_bytes += entry.size_bytes;
                }
            }
        }

        PruneStrategy::KeepRecent(keep_count) => {
            let mut entries: Vec<_> = metadata.entries().collect();
            entries.sort_by_key(|e| std::cmp::Reverse(e.cached_at));

            for entry in entries.iter().skip(keep_count) {
                to_remove.push(entry.package_id.clone());
                removed_bytes += entry.size_bytes;
            }
        }

        PruneStrategy::MaxSize(max_bytes) => {
            let mut entries: Vec<_> = metadata.entries().collect();
            entries.sort_by_key(|e| std::cmp::Reverse(e.cached_at));

            let mut current_size = metadata.total_size();

            for entry in entries.iter() {
                if current_size <= max_bytes {
                    break;
                }
                to_remove.push(entry.package_id.clone());
                removed_bytes += entry.size_bytes;
                current_size -= entry.size_bytes;
            }
        }
    }

    // Remove files and metadata entries
    // Xóa files và metadata entries
    for package_id in &to_remove {
        if let Some(entry) = metadata.remove(package_id) {
            if entry.file_path.exists() {
                std::fs::remove_file(&entry.file_path)
                    .map_err(|e| MgError::Other(format!("failed to remove file: {}", e)))?;
            }
        }
    }

    // Save updated metadata
    // Lưu metadata đã cập nhật
    metadata.save(&meta_path)?;

    Ok(removed_bytes)
}

/// Prune all caches (Rust + Python + TypeScript).
/// Prune tất cả caches (Rust + Python + TypeScript).
pub fn prune_all_caches(strategy: PruneStrategy) -> MgResult<u64> {
    let mut total_removed = 0u64;

    for lang in &["rust", "python", "ts"] {
        match prune_cache(lang, strategy) {
            Ok(removed) => total_removed += removed,
            Err(e) => {
                eprintln!("Warning: failed to prune {} cache: {}", lang, e);
            }
        }
    }

    Ok(total_removed)
}
