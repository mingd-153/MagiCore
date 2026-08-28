//! `cache.rs` — Shared Web Cache, Metadata LRU Cache and CAS management for WebAdapter.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use mgc_resolver::DependencyError;
use mgc_store::{Layout, PackageCache};
use mgc_types::{
    adapter::{ResolvedGraph, ResolvedPackage},
    MgResult,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::manifest::atomic_write;
use crate::native;

pub use crate::cache_metadata::*;
pub use crate::cache_prune::*;

const RESOLUTION_CACHE_TTL_SECS: u64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedWebCache {
    pub root: PathBuf,
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

impl SharedWebCache {
    pub fn discover() -> Option<Self> {
        if let Ok(path) = std::env::var("MAGICORE_SHARED_CACHE_DIR") {
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
                root: root.join("magicore").join("web"),
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

        let probe = self.root.join(".mgc-write-probe");
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

    pub fn read_resolution(
        &self,
        key: &str,
        registry_url: &str,
    ) -> MgResult<Option<ResolvedGraph>> {
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

pub fn download_concurrency_limit() -> usize {
    std::env::var("MAGICORE_WEB_DOWNLOAD_CONCURRENCY")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|limit| *limit > 0)
        .unwrap_or(24)
}

pub fn resolve_prefetch_enabled() -> bool {
    std::env::var("MAGICORE_WEB_RESOLVE_PREFETCH")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
}

pub fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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
