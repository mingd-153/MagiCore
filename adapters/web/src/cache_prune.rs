// Cache pruning for core-web — removes stale and over-quota cache entries safely.
// Dọn cache cho core-web — tách quota/GC khỏi API cache chính để dễ audit.
use std::path::{Path, PathBuf};

use mgc_store::Layout;
use mgc_types::{MgError, MgResult};
use walkdir::WalkDir;

use crate::cache::current_unix_secs;
use crate::manifest::atomic_write;

pub fn shared_cache_prune_interval_secs() -> u64 {
    std::env::var("MAGICORE_WEB_CACHE_PRUNE_INTERVAL_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ttl| *ttl > 0)
        .unwrap_or(6 * 60 * 60)
}

pub fn shared_cache_max_age_secs() -> u64 {
    std::env::var("MAGICORE_WEB_CACHE_MAX_AGE_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ttl| *ttl > 0)
        .unwrap_or(7 * 24 * 60 * 60)
}

pub fn shared_cache_max_bytes() -> u64 {
    std::env::var("MAGICORE_WEB_CACHE_MAX_BYTES")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(2 * 1024 * 1024 * 1024)
}

pub fn shared_cache_prune_stamp_path(root: &Path) -> PathBuf {
    root.join(".gc-stamp")
}

pub fn shared_cache_prune_due(root: &Path) -> bool {
    let stamp = shared_cache_prune_stamp_path(root);
    let Ok(metadata) = std::fs::metadata(&stamp) else {
        return true;
    };
    let Ok(modified) = metadata.modified() else {
        return true;
    };
    let Ok(elapsed) = modified.elapsed() else {
        return true;
    };
    elapsed >= std::time::Duration::from_secs(shared_cache_prune_interval_secs())
}

pub fn write_shared_cache_prune_stamp(root: &Path) -> MgResult<()> {
    let stamp = shared_cache_prune_stamp_path(root);
    let data = current_unix_secs().to_string();
    atomic_write(&stamp, data.as_bytes())?;
    Ok(())
}

pub fn prune_project_local_cache(layout: &Layout) {
    let max_age = std::time::Duration::from_secs(shared_cache_max_age_secs());
    let _ = prune_old_files_under(&layout.cache_dir(), max_age);
    let _ = prune_unlinked_old_cas_files_under(&layout.cas_dir(), max_age);
    let _ = prune_old_files_under(&layout.root().join("resolutions"), max_age);
}

pub fn prune_unlinked_old_cas_files_under(
    root: &Path,
    max_age: std::time::Duration,
) -> MgResult<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut directories = Vec::new();
    for entry in WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_dir() {
            directories.push(entry.path().to_path_buf());
            continue;
        }
        if !entry.file_type().is_file()
            || !path_is_older_than(entry.path(), max_age)
            || !file_has_no_external_hardlinks(entry.path())
        {
            continue;
        }
        let _ = std::fs::remove_file(entry.path());
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for dir in directories {
        remove_dir_if_empty(&dir);
    }
    Ok(())
}

pub fn file_has_no_external_hardlinks(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path)
            .map(|metadata| metadata.nlink() <= 1)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

pub fn prune_old_files_under(root: &Path, max_age: std::time::Duration) -> MgResult<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut directories = Vec::new();
    for entry in WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_dir() {
            directories.push(entry.path().to_path_buf());
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        if path_is_older_than(entry.path(), max_age) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for dir in directories {
        remove_dir_if_empty(&dir);
    }
    Ok(())
}

pub fn prune_old_package_dirs_under(
    root: &Path,
    max_age: std::time::Duration,
    pinned_package_roots: &std::collections::HashSet<PathBuf>,
) -> MgResult<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut directories = Vec::new();
    for entry in WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_dir() {
            directories.push(entry.path().to_path_buf());
        }
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for dir in directories {
        if pinned_package_roots.contains(&canonical_or_original(&dir)) {
            continue;
        }
        let marker = dir.join(".magicore-package-root.json");
        let package_json = dir.join("package.json");
        if marker.exists() || package_json.exists() {
            if path_is_older_than(
                if marker.exists() {
                    &marker
                } else {
                    &package_json
                },
                max_age,
            ) {
                let _ = std::fs::remove_dir_all(&dir);
            }
        } else {
            remove_dir_if_empty(&dir);
        }
    }
    Ok(())
}

pub fn prune_old_metadata_dirs_under(root: &Path, max_age: std::time::Duration) -> MgResult<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(root).map_err(|err| {
        MgError::Other(format!(
            "failed to read metadata cache root '{}': {}",
            root.display(),
            err
        ))
    })? {
        let Ok(entry) = entry else {
            continue;
        };
        let dir = entry.path();
        let metadata_json = dir.join("metadata.json");
        if metadata_json.exists() && path_is_older_than(&metadata_json, max_age) {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub enum CachePruneEntryKind {
    File,
    Directory,
}

#[derive(Debug)]
pub struct CachePruneEntry {
    pub path: PathBuf,
    pub bytes: u64,
    pub modified: std::time::SystemTime,
    pub kind: CachePruneEntryKind,
}

pub fn prune_shared_cache_to_quota(
    root: &Path,
    max_bytes: u64,
    pinned_package_roots: &std::collections::HashSet<PathBuf>,
) -> MgResult<()> {
    if max_bytes == 0 || !root.exists() {
        return Ok(());
    }

    let mut entries = Vec::new();
    collect_prunable_files(&root.join("cache"), &mut entries);
    collect_prunable_files(&root.join("metadata"), &mut entries);
    collect_prunable_files(&root.join("resolutions"), &mut entries);
    collect_prunable_package_dirs(&root.join("packages"), &mut entries, pinned_package_roots);

    let mut total: u64 = entries.iter().map(|entry| entry.bytes).sum();
    if total <= max_bytes {
        return Ok(());
    }

    entries.sort_by_key(|entry| entry.modified);
    for entry in entries {
        if total <= max_bytes {
            break;
        }
        let removed = match entry.kind {
            CachePruneEntryKind::File => std::fs::remove_file(&entry.path).is_ok(),
            CachePruneEntryKind::Directory => std::fs::remove_dir_all(&entry.path).is_ok(),
        };
        if removed {
            total = total.saturating_sub(entry.bytes);
        }
    }

    cleanup_empty_dirs(&root.join("cache"));
    cleanup_empty_dirs(&root.join("metadata"));
    cleanup_empty_dirs(&root.join("resolutions"));
    cleanup_empty_dirs(&root.join("packages"));
    Ok(())
}

pub fn collect_prunable_files(root: &Path, entries: &mut Vec<CachePruneEntry>) {
    if !root.exists() {
        return;
    }
    for entry in WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        entries.push(CachePruneEntry {
            path: entry.path().to_path_buf(),
            bytes: metadata.len(),
            modified: metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
            kind: CachePruneEntryKind::File,
        });
    }
}

pub fn collect_prunable_package_dirs(
    root: &Path,
    entries: &mut Vec<CachePruneEntry>,
    pinned_package_roots: &std::collections::HashSet<PathBuf>,
) {
    if !root.exists() {
        return;
    }
    for entry in WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path();
        if pinned_package_roots.contains(&canonical_or_original(path)) {
            continue;
        }
        let marker = path.join(".magicore-package-root.json");
        if !marker.exists() {
            continue;
        }
        let modified = std::fs::metadata(&marker)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        entries.push(CachePruneEntry {
            path: path.to_path_buf(),
            bytes: directory_size(path),
            modified,
            kind: CachePruneEntryKind::Directory,
        });
    }
}

pub fn directory_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .sum()
}

pub fn cleanup_empty_dirs(root: &Path) {
    if !root.exists() {
        return;
    }
    let mut directories: Vec<PathBuf> = WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir())
        .map(|entry| entry.path().to_path_buf())
        .collect();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for dir in directories {
        remove_dir_if_empty(&dir);
    }
}

pub fn path_is_older_than(path: &Path, max_age: std::time::Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|elapsed| elapsed >= max_age)
}

pub fn remove_dir_if_empty(path: &Path) {
    let _ = std::fs::remove_dir(path);
}

pub fn path_to_cache_ref_string(path: &Path) -> String {
    canonical_or_original(path).to_string_lossy().into_owned()
}

pub fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
