//! `install/extract.rs` — CAS extraction, marker signature generation and validation.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use mgc_fetcher::extract::extract_tarball_to_cas_and_link;
use mgc_store::{ContentStore, Database, Layout};
use mgc_types::adapter::ResolvedPackage;
use mgc_types::{MgError, MgResult, PackageId};

use crate::cache::{ExtractedPackageMarker, SharedWebCache};
pub use crate::install::package_marker::*;

pub fn extracted_package_root_lock(root: &Path) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<std::collections::HashMap<PathBuf, Arc<Mutex<()>>>>> =
        OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut guard = match locks.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard
        .entry(root.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

pub fn tarball_prefetch_lock(id: &PackageId) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let key = format!("{}@{}", id.name_str(), id.version());
    let mut guard = match locks.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard
        .entry(key)
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

pub struct CasClaimContext {
    pub db: std::result::Result<Database, String>,
    pub project_key: String,
}

pub fn cas_claim_context(layout: &Layout) -> Option<CasClaimContext> {
    let db = Database::open(&layout.db_path()).map_err(|e| e.to_string());
    Some(CasClaimContext {
        db,
        project_key: layout.root().to_string_lossy().into_owned(),
    })
}

pub fn claim_ctx(ctx: Option<&CasClaimContext>) -> Option<(&Database, &str)> {
    match ctx {
        Some(ctx) => ctx.as_ref(),
        None => None,
    }
}

impl CasClaimContext {
    pub fn as_ref(&self) -> Option<(&Database, &str)> {
        match &self.db {
            Ok(db) => Some((db, &self.project_key)),
            Err(_) => None,
        }
    }
}

pub fn extracted_cache_full_validation_enabled() -> bool {
    std::env::var("MAGICORE_WEB_VALIDATE_EXTRACTED_CACHE")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
}

pub fn locate_package_dir(extract_root: &Path) -> MgResult<PathBuf> {
    let package_dir = extract_root.join("package");
    if package_dir.is_dir() {
        return Ok(package_dir);
    }

    let first_dir = std::fs::read_dir(extract_root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.is_dir());

    first_dir.ok_or_else(|| {
        MgError::Other(format!(
            "extracted tarball missing package root in '{}'",
            extract_root.display()
        ))
    })
}

pub fn ensure_extracted_package_root(
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    pkg: &ResolvedPackage,
    tarball_path: &Path,
) -> MgResult<PathBuf> {
    let fast_marker = expected_extracted_package_marker_from_path(pkg, tarball_path)?;
    let claim = cas_claim_context(layout);
    ensure_extracted_package_root_with_marker(
        layout,
        store,
        shared_cache,
        pkg,
        &fast_marker,
        |temp_root| {
            let file = std::fs::File::open(tarball_path).map_err(|err| {
                MgError::Other(format!(
                    "failed to open tarball '{}' for '{}': {}",
                    tarball_path.display(),
                    pkg.id.name_str(),
                    err
                ))
            })?;
            extract_tarball_to_cas_and_link(file, temp_root, store, claim_ctx(claim.as_ref()))
                .map_err(|e| MgError::Other(e.to_string()))
        },
    )
}

pub fn ensure_extracted_package_root_from_bytes(
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    pkg: &ResolvedPackage,
    tarball_bytes: &[u8],
) -> MgResult<PathBuf> {
    let expected_marker = expected_extracted_package_marker_from_bytes(pkg, tarball_bytes)?;
    let claim = cas_claim_context(layout);
    ensure_extracted_package_root_with_marker(
        layout,
        store,
        shared_cache,
        pkg,
        &expected_marker,
        |temp_root| {
            extract_tarball_to_cas_and_link(
                std::io::Cursor::new(tarball_bytes),
                temp_root,
                store,
                claim_ctx(claim.as_ref()),
            )
            .map_err(|e| MgError::Other(e.to_string()))
        },
    )
}

pub fn ensure_extracted_package_root_with_marker<F>(
    layout: &Layout,
    _store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    pkg: &ResolvedPackage,
    expected_marker: &ExtractedPackageMarker,
    extract_into: F,
) -> MgResult<PathBuf>
where
    F: FnOnce(&Path) -> MgResult<()>,
{
    let canonical_root = shared_cache
        .map(|shared| shared.extracted_package_root(pkg))
        .unwrap_or_else(|| local_extracted_package_root(layout, pkg));
    let canonical_lock = extracted_package_root_lock(&canonical_root);
    let _canonical_guard = match canonical_lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if canonical_root.join("package.json").exists() {
        let marker = read_extracted_package_marker(&canonical_root)?;
        if let Some(marker) = marker.as_ref() {
            if extracted_marker_matches_fast(marker, expected_marker)
                && extracted_marker_has_content_signature(marker)
                && (!extracted_cache_full_validation_enabled()
                    || extracted_content_matches(&canonical_root, marker)?)
            {
                return Ok(canonical_root);
            }
        }
    }

    let temp_root = {
        let parent = canonical_root.parent().unwrap_or(Path::new("."));
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        parent.join(format!(".mgc-extract-{}-{}", pkg.id.name_str(), ts))
    };
    if temp_root.exists() {
        std::fs::remove_dir_all(&temp_root).map_err(|err| {
            MgError::Other(format!(
                "failed to remove stale temp root '{}' for '{}': {}",
                temp_root.display(),
                pkg.id.name_str(),
                err
            ))
        })?;
    }
    let extract_result: MgResult<()> = (|| {
        extract_into(&temp_root)?;
        let package_root = locate_package_dir(&temp_root)?;
        if let Some(parent) = canonical_root.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                MgError::Other(format!(
                    "failed to create canonical parent '{}' for '{}': {}",
                    parent.display(),
                    pkg.id.name_str(),
                    err
                ))
            })?;
        }
        if canonical_root.exists() {
            std::fs::remove_dir_all(&canonical_root).map_err(|err| {
                MgError::Other(format!(
                    "failed to remove stale canonical root '{}' for '{}': {}",
                    canonical_root.display(),
                    pkg.id.name_str(),
                    err
                ))
            })?;
        }
        std::fs::rename(&package_root, &canonical_root).map_err(|err| {
            MgError::Other(format!(
                "failed to rename extracted '{}' to canonical '{}' for '{}': {}",
                package_root.display(),
                canonical_root.display(),
                pkg.id.name_str(),
                err
            ))
        })?;
        Ok(())
    })();
    if temp_root.exists() {
        let _ = std::fs::remove_dir_all(&temp_root);
    }
    extract_result?;
    write_extracted_package_marker(&canonical_root, expected_marker)?;
    Ok(canonical_root)
}
