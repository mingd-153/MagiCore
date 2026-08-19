//! `cache.rs` — Shared Web Cache, Metadata LRU Cache and CAS management for WebAdapter.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use lru::LruCache;
use mg_store::{Layout, PackageCache};
use mg_resolver::DependencyError;
use mg_types::{
    adapter::{ResolvedGraph, ResolvedPackage},
    MgError, MgResult, PackageId, PackageName,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::manifest::atomic_write;
use crate::native;

const MAX_METADATA_CACHE_ENTRIES: usize = 2048;
const METADATA_CACHE_TTL_SECS: u64 = 6 * 60 * 60;
const RESOLUTION_CACHE_TTL_SECS: u64 = 300;

pub struct MetadataCache {
    cache: Mutex<LruCache<String, (Arc<native::npm_registry::PackageMetadata>, Instant)>>,
    ttl: Duration,
}

impl MetadataCache {
    pub fn new() -> Self {
        let max_entries = std::env::var("MEGAGATE_WEB_METADATA_CACHE_MAX_ENTRIES")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(MAX_METADATA_CACHE_ENTRIES);
        let ttl_secs = std::env::var("MEGAGATE_WEB_METADATA_CACHE_TTL_SECS")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(METADATA_CACHE_TTL_SECS);
        Self {
            cache: Mutex::new(LruCache::new(
                std::num::NonZeroUsize::new(max_entries).unwrap(),
            )),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    pub fn get(&self, key: &str) -> Option<Arc<native::npm_registry::PackageMetadata>> {
        let mut cache = self.cache.lock().unwrap();
        if let Some((meta, instant)) = cache.get(key) {
            if instant.elapsed() < self.ttl {
                return Some(Arc::clone(meta));
            } else {
                cache.pop(key);
            }
        }
        None
    }

    pub fn insert(&self, key: String, meta: Arc<native::npm_registry::PackageMetadata>) {
        let mut cache = self.cache.lock().unwrap();
        cache.put(key, (meta, Instant::now()));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedWebCache {
    pub root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMetadataEnvelope {
    pub fetched_at: u64,
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub stale_retry_after: Option<u64>,
    pub metadata: native::npm_registry::PackageMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedResolutionEnvelope {
    pub cache_version: u32,
    pub registry_url: String,
    pub fetched_at: u64,
    pub graph: ResolvedGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractedPackageMarker {
    #[serde(default)]
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub integrity: Option<String>,
    pub tarball_sha256: String,
    #[serde(default)]
    pub file_count: u64,
    #[serde(default)]
    pub unpacked_size: u64,
    #[serde(default)]
    pub file_tree_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TarballContentSignature {
    pub file_count: u64,
    pub unpacked_size: u64,
    pub file_tree_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SharedCacheProjectRef {
    pub schema_version: u32,
    pub project_root: String,
    pub updated_at: u64,
    pub package_roots: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CachedMetadataRecord {
    pub fetched_at: u64,
    pub etag: Option<String>,
    pub stale_retry_after: Option<u64>,
    pub metadata: native::npm_registry::PackageMetadata,
}

impl SharedWebCache {
    pub fn discover() -> Option<Self> {
        if let Ok(path) = std::env::var("MEGAGATE_SHARED_CACHE_DIR") {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                let candidate = Self {
                    root: PathBuf::from(trimmed),
                };
                if candidate.is_usable() {
                    candidate.maybe_prune_once_per_process();
                    return Some(candidate);
                }
                return None;
            }
        }

        dirs::cache_dir().and_then(|root| {
            let candidate = Self {
                root: root.join("megagate").join("web"),
            };
            if candidate.is_usable() {
                candidate.maybe_prune_once_per_process();
                Some(candidate)
            } else {
                None
            }
        })
    }

    pub fn is_usable(&self) -> bool {
        if std::fs::create_dir_all(&self.root).is_err() {
            return false;
        }

        let probe = self.root.join(".mg-write-probe");
        match std::fs::create_dir(&probe) {
            Ok(()) => {
                let _ = std::fs::remove_dir(&probe);
                true
            }
            Err(_) => false,
        }
    }

    pub fn package_cache(&self) -> anyhow::Result<PackageCache> {
        let layout = Layout::new(self.root.clone());
        std::fs::create_dir_all(layout.root())?;
        PackageCache::new(layout.cache_dir())
    }

    pub fn extracted_package_root(&self, pkg: &ResolvedPackage) -> PathBuf {
        shared_extracted_package_root(&self.root, pkg)
    }

    pub fn project_ref_path(&self, project_root: &Path) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(path_to_cache_ref_string(project_root).as_bytes());
        let key = hex::encode(hasher.finalize());
        self.root
            .join("refs")
            .join("projects")
            .join(format!("{key}.json"))
    }

    pub fn write_project_ref(
        &self,
        project_root: &Path,
        package_roots: impl IntoIterator<Item = PathBuf>,
    ) -> MgResult<()> {
        let path = self.project_ref_path(project_root);
        let mut roots = package_roots
            .into_iter()
            .map(|path| path_to_cache_ref_string(&path))
            .collect::<Vec<_>>();
        roots.sort();
        roots.dedup();
        let payload = serde_json::to_vec_pretty(&SharedCacheProjectRef {
            schema_version: 1,
            project_root: path_to_cache_ref_string(project_root),
            updated_at: current_unix_secs(),
            package_roots: roots,
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&path, &payload)
    }

    pub fn metadata_path(&self, package: &str, registry_url: &str) -> PathBuf {
        let reg_key: String = registry_url
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect();
        self.root
            .join("metadata")
            .join(reg_key)
            .join(package)
            .join("metadata.json")
    }

    pub fn resolution_path(&self, key: &str) -> PathBuf {
        self.root.join("resolutions").join(format!("{key}.json"))
    }

    pub fn read_resolution(&self, key: &str, registry_url: &str) -> MgResult<Option<ResolvedGraph>> {
        let path = self.resolution_path(key);
        if !path.exists() {
            return Ok(None);
        }

        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(_) => return Ok(None),
        };
        let envelope = match serde_json::from_str::<CachedResolutionEnvelope>(&contents) {
            Ok(envelope) => envelope,
            Err(_) => {
                let _ = std::fs::remove_file(&path);
                return Ok(None);
            }
        };
        if envelope.cache_version != 1 || envelope.registry_url != registry_url {
            return Ok(None);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if now.saturating_sub(envelope.fetched_at) > RESOLUTION_CACHE_TTL_SECS {
            let _ = std::fs::remove_file(&path);
            return Ok(None);
        }
        Ok(Some(envelope.graph))
    }

    pub fn write_resolution(
        &self,
        key: &str,
        registry_url: &str,
        graph: &ResolvedGraph,
    ) -> MgResult<()> {
        let path = self.resolution_path(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_vec(&CachedResolutionEnvelope {
            cache_version: 1,
            registry_url: registry_url.to_string(),
            fetched_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            graph: graph.clone(),
        })?;
        atomic_write(&path, &payload)
    }

    pub fn read_metadata(
        &self,
        package: &str,
        registry_url: &str,
    ) -> Result<Option<CachedMetadataRecord>, DependencyError> {
        let path = self.metadata_path(package, registry_url);
        if !path.exists() {
            return Ok(None);
        }

        let contents = std::fs::read_to_string(&path).map_err(|e| {
            DependencyError(format!(
                "failed to read cached metadata for '{}': {}",
                package, e
            ))
        })?;
        if let Ok(envelope) = serde_json::from_str::<CachedMetadataEnvelope>(&contents) {
            return Ok(Some(CachedMetadataRecord {
                fetched_at: envelope.fetched_at,
                etag: envelope.etag.clone(),
                stale_retry_after: envelope.stale_retry_after,
                metadata: envelope.metadata,
            }));
        }

        let metadata = serde_json::from_str(&contents).map_err(|e| {
            DependencyError(format!(
                "failed to parse cached metadata for '{}': {}",
                package, e
            ))
        })?;
        Ok(Some(CachedMetadataRecord {
            fetched_at: 0,
            etag: None,
            stale_retry_after: None,
            metadata,
        }))
    }

    pub fn write_metadata(
        &self,
        package: &str,
        metadata: &native::npm_registry::PackageMetadata,
        etag: Option<String>,
        registry_url: &str,
    ) -> Result<(), DependencyError> {
        self.write_metadata_record(
            package,
            metadata,
            etag,
            current_unix_secs(),
            None,
            registry_url,
        )
    }

    pub fn write_metadata_record(
        &self,
        package: &str,
        metadata: &native::npm_registry::PackageMetadata,
        etag: Option<String>,
        fetched_at: u64,
        stale_retry_after: Option<u64>,
        registry_url: &str,
    ) -> Result<(), DependencyError> {
        let path = self.metadata_path(package, registry_url);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                DependencyError(format!(
                    "failed to create metadata cache dir for '{}': {}",
                    package, e
                ))
            })?;
        }
        let payload = serde_json::to_vec(&CachedMetadataEnvelope {
            fetched_at,
            etag,
            stale_retry_after,
            metadata: metadata.clone(),
        })
        .map_err(|e| {
            DependencyError(format!(
                "failed to serialize cached metadata for '{}': {}",
                package, e
            ))
        })?;
        atomic_write(&path, &payload).map_err(|e| {
            DependencyError(format!(
                "failed to write cached metadata for '{}': {}",
                package, e
            ))
        })?;
        Ok(())
    }

    pub fn maybe_prune(&self) {
        if !shared_cache_prune_due(&self.root) {
            return;
        }

        let pinned_roots = read_shared_cache_pinned_package_roots(&self.root);
        let max_age = std::time::Duration::from_secs(shared_cache_max_age_secs());
        let _ = prune_old_files_under(&self.root.join("cache"), max_age);
        let _ = prune_old_files_under(&self.root.join("resolutions"), max_age);
        let _ = prune_old_package_dirs_under(&self.root.join("packages"), max_age, &pinned_roots);
        let _ = prune_old_metadata_dirs_under(&self.root.join("metadata"), max_age);
        let _ = prune_shared_cache_to_quota(&self.root, shared_cache_max_bytes(), &pinned_roots);
        let _ = write_shared_cache_prune_stamp(&self.root);
    }

    pub fn maybe_prune_once_per_process(&self) {
        static PRUNED_ROOTS: OnceLock<Mutex<std::collections::HashSet<PathBuf>>> = OnceLock::new();
        let pruned_roots =
            PRUNED_ROOTS.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
        let mut guard = match pruned_roots.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        if !guard.insert(self.root.clone()) {
            return;
        }
        drop(guard);
        self.maybe_prune();
    }
}

pub fn shared_extracted_package_root(root: &Path, pkg: &ResolvedPackage) -> PathBuf {
    let safe_name = pkg.id.name_str().replace('/', "__").replace('@', "");
    let cache_key = extracted_package_cache_key(pkg);
    root.join("packages")
        .join(safe_name)
        .join(cache_key)
        .join("package")
}

pub fn extracted_package_cache_key(pkg: &ResolvedPackage) -> String {
    if pkg.integrity.is_empty() {
        return pkg.id.version().to_string();
    }
    let fs_safe = pkg
        .integrity
        .replace('/', "_")
        .replace('+', "-")
        .replace('=', "");
    format!("{}-{}", pkg.id.version(), fs_safe)
}

pub fn metadata_record_is_fresh(record: &CachedMetadataRecord) -> bool {
    if record.fetched_at == 0 {
        return false;
    }
    current_unix_secs().saturating_sub(record.fetched_at) <= metadata_ttl_secs()
}

pub fn metadata_record_retry_deferred(record: &CachedMetadataRecord) -> bool {
    record
        .stale_retry_after
        .is_some_and(|retry_after| retry_after > current_unix_secs())
}

pub fn metadata_record_is_usable_stale(record: &CachedMetadataRecord) -> bool {
    if record.fetched_at == 0 {
        return true;
    }
    current_unix_secs().saturating_sub(record.fetched_at) <= metadata_max_stale_fallback_secs()
}

pub fn metadata_ttl_secs() -> u64 {
    std::env::var("MEGAGATE_WEB_METADATA_TTL_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(6 * 60 * 60)
}

pub fn metadata_max_stale_fallback_secs() -> u64 {
    std::env::var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ttl| *ttl > 0)
        .unwrap_or(24 * 60 * 60)
}

pub fn metadata_stale_retry_ttl_secs() -> u64 {
    std::env::var("MEGAGATE_WEB_METADATA_STALE_RETRY_TTL_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ttl| *ttl > 0)
        .unwrap_or(30)
}

pub fn shared_cache_prune_interval_secs() -> u64 {
    std::env::var("MEGAGATE_WEB_CACHE_PRUNE_INTERVAL_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ttl| *ttl > 0)
        .unwrap_or(6 * 60 * 60)
}

pub fn download_concurrency_limit() -> usize {
    std::env::var("MEGAGATE_WEB_DOWNLOAD_CONCURRENCY")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|limit| *limit > 0)
        .unwrap_or(24)
}

pub fn metadata_concurrency_limit() -> usize {
    std::env::var("MEGAGATE_WEB_METADATA_CONCURRENCY")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|limit| *limit > 0)
        .unwrap_or(24)
}

pub fn resolve_prefetch_enabled() -> bool {
    std::env::var("MEGAGATE_WEB_RESOLVE_PREFETCH")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
}

pub fn shared_cache_max_age_secs() -> u64 {
    std::env::var("MEGAGATE_WEB_CACHE_MAX_AGE_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ttl| *ttl > 0)
        .unwrap_or(7 * 24 * 60 * 60)
}

pub fn shared_cache_max_bytes() -> u64 {
    std::env::var("MEGAGATE_WEB_CACHE_MAX_BYTES")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(2 * 1024 * 1024 * 1024)
}

pub fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn next_stale_retry_after() -> u64 {
    current_unix_secs().saturating_add(metadata_stale_retry_ttl_secs())
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

pub fn prune_unlinked_old_cas_files_under(root: &Path, max_age: std::time::Duration) -> MgResult<()> {
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
        let marker = dir.join(".megagate-package-root.json");
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

pub fn read_shared_cache_pinned_package_roots(root: &Path) -> std::collections::HashSet<PathBuf> {
    let refs_root = root.join("refs").join("projects");
    let mut pinned = std::collections::HashSet::new();
    if !refs_root.exists() {
        return pinned;
    }

    let Ok(entries) = std::fs::read_dir(&refs_root) else {
        return pinned;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(reference) = serde_json::from_str::<SharedCacheProjectRef>(&contents) else {
            let _ = std::fs::remove_file(&path);
            continue;
        };
        if reference.schema_version != 1 {
            continue;
        }
        let project_root = PathBuf::from(&reference.project_root);
        if !project_root.exists() {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        for package_root in reference.package_roots {
            pinned.insert(PathBuf::from(package_root));
        }
    }
    pinned
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
        let marker = path.join(".megagate-package-root.json");
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

pub async fn load_metadata_with_fallback(
    package: &PackageName,
    registry: &native::npm_registry::NpmRegistry,
    shared_cache: Option<&SharedWebCache>,
) -> Result<Arc<native::npm_registry::PackageMetadata>, DependencyError> {
    load_metadata_by_name_with_fallback(package.as_str(), registry, shared_cache).await
}

pub async fn load_metadata_by_name_with_fallback(
    package: &str,
    registry: &native::npm_registry::NpmRegistry,
    shared_cache: Option<&SharedWebCache>,
) -> Result<Arc<native::npm_registry::PackageMetadata>, DependencyError> {
    let registry_url = registry.registry_url().to_string();
    let cached = if let Some(shared_cache) = shared_cache {
        shared_cache.read_metadata(package, &registry_url)?
    } else {
        None
    };

    if let Some(cached) = cached.as_ref() {
        if metadata_record_is_fresh(cached) {
            return Ok(Arc::new(cached.metadata.clone()));
        }

        if metadata_record_retry_deferred(cached) && metadata_record_is_usable_stale(cached) {
            return Ok(Arc::new(cached.metadata.clone()));
        }

        if let Some(etag) = &cached.etag {
            match registry
                .fetch_metadata_conditional(package, Some(etag))
                .await
            {
                Ok(None) => {
                    if let Some(shared_cache) = shared_cache {
                        let _ = shared_cache.write_metadata(
                            package,
                            &cached.metadata,
                            Some(etag.clone()),
                            &registry_url,
                        );
                    }
                    return Ok(Arc::new(cached.metadata.clone()));
                }
                Ok(Some((metadata, new_etag))) => {
                    if let Some(shared_cache) = shared_cache {
                        let _ = shared_cache.write_metadata(
                            package,
                            &metadata,
                            Some(new_etag),
                            &registry_url,
                        );
                    }
                    return Ok(Arc::new(metadata));
                }
                Err(_) => {
                    if !metadata_record_is_usable_stale(cached) {
                        return Err(DependencyError(format!(
                            "npm metadata refresh failed for '{}' and cached metadata is too old to reuse",
                            package
                        )));
                    }
                    if let Some(shared_cache) = shared_cache {
                        let _ = shared_cache.write_metadata_record(
                            package,
                            &cached.metadata,
                            cached.etag.clone(),
                            cached.fetched_at,
                            Some(next_stale_retry_after()),
                            &registry_url,
                        );
                    }
                    return Ok(Arc::new(cached.metadata.clone()));
                }
            }
        }
    }

    match registry.fetch_metadata_with_etag(package).await {
        Ok((metadata, etag)) => {
            if let Some(shared_cache) = shared_cache {
                let _ = shared_cache.write_metadata_record(
                    package,
                    &metadata,
                    etag,
                    current_unix_secs(),
                    None,
                    &registry_url,
                );
            }
            Ok(Arc::new(metadata))
        }
        Err(e) => {
            if let Some(cached) = cached {
                if metadata_record_is_usable_stale(&cached) {
                    if let Some(shared_cache) = shared_cache {
                        let _ = shared_cache.write_metadata_record(
                            package,
                            &cached.metadata,
                            cached.etag.clone(),
                            cached.fetched_at,
                            Some(next_stale_retry_after()),
                            &registry_url,
                        );
                    }
                    return Ok(Arc::new(cached.metadata));
                }
            }
            Err(DependencyError(format!(
                "failed to fetch metadata for '{}': {}",
                package, e
            )))
        }
    }
}
