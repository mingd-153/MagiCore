//! `lockfile.rs` — Lockfile reading, writing, verification and graph reconstruction for WebAdapter.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use base64::Engine;
use chrono;
use mgc_lockfile::{LOCKFILE_SCHEMA_VERSION, Lockfile, LockfileMetadata, Package};
use mgc_store::{Layout, PackageCache};
use mgc_types::{
    Manifest, MgError, MgResult, PackageId, PackageName, Version, adapter::ResolvedGraph,
    adapter::ResolvedPackage,
};
use sha2::{Digest, Sha512};

pub fn strict_integrity_enforced() -> bool {
    std::env::var("MAGICORE_STRICT_INTEGRITY").is_ok()
        || std::env::var("MGC_STRICT_INTEGRITY").is_ok()
}

pub fn project_cache_dir(project_root: &Path) -> PathBuf {
    project_root.join(".magicore").join("cache").join("web")
}

pub fn compute_sha512_b64(bytes: &[u8]) -> String {
    let mut hasher = Sha512::new();
    hasher.update(bytes);
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

pub fn compute_tarball_integrity(bytes: &[u8]) -> String {
    format!("sha512-{}", compute_sha512_b64(bytes))
}

pub fn web_lockfile_matches_graph(lockfile: &Lockfile, graph: &ResolvedGraph) -> bool {
    // Accept both "2" (legacy on-disk locks must still short-circuit the
    // rewrite) and the current schema version ("3") that this writer emits.
    // Chấp nhận cả "2" (lock cũ trên đĩa vẫn được phép bỏ qua ghi lại) lẫn
    // version schema hiện tại ("3") mà writer này ghi ra.
    if lockfile.version != "2" && lockfile.version != LOCKFILE_SCHEMA_VERSION {
        return false;
    }

    let web_packages: Vec<_> = lockfile
        .packages
        .iter()
        .filter(|package| is_web_lock_package(lockfile, package))
        .collect();
    if web_packages.len() != graph.packages.len() {
        return false;
    }

    // Check packages match
    web_packages
        .iter()
        .zip(graph.packages.iter())
        .all(|(locked, resolved)| {
            locked.name == resolved.id.name_str()
                && locked.version == resolved.id.version().to_string()
                && locked.dependencies.len() == resolved.deps.len()
                && locked
                    .dependencies
                    .iter()
                    .zip(resolved.deps.iter())
                    .all(|(left, right)| left == &right.to_string())
                && (resolved.integrity.is_empty() || locked.integrity == resolved.integrity)
        })
}

/// v2 locks predate ecosystem tags and were web-only; in v3, ownership is
/// explicit and only `web` entries belong to this adapter.
/// (Lock v2 chưa có ecosystem nên mặc định là web; v3 chỉ nhận tag web.)
fn is_web_lock_package(lockfile: &Lockfile, package: &Package) -> bool {
    package.ecosystem == mgc_lockfile::EcosystemTag::Web
        || (lockfile.version == "2" && package.ecosystem == mgc_lockfile::EcosystemTag::Other)
}

pub fn installed_package_version(path: &Path) -> Option<Version> {
    let package_json = path.join("package.json");
    let contents = std::fs::read_to_string(package_json).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    let version = value.get("version")?.as_str()?;
    Version::parse(version).ok()
}

pub fn installed_package_matches(path: &Path, package_id: &PackageId) -> bool {
    installed_package_version(path)
        .map(|version| version == *package_id.version())
        .unwrap_or(false)
}

pub fn read_web_lockfile(project_root: &Path) -> Option<Lockfile> {
    read_web_lockfile_checked(project_root).ok().flatten()
}

pub fn read_web_lockfile_checked(project_root: &Path) -> MgResult<Option<Lockfile>> {
    let mut lockfile = read_web_lockfile_document(project_root)?;
    if let Some(lockfile) = &mut lockfile {
        scope_web_lock_packages(lockfile, &web_lock_owner_core(project_root)?);
    }
    Ok(lockfile)
}

/// Read the complete shared lock document. Only lock publication uses this;
/// operation readers must use `read_web_lockfile_checked` so sibling-core
/// Web entries cannot satisfy this project's dependency graph.
/// (Chỉ writer đọc toàn bộ lock; reader nghiệp vụ phải dùng bản đã scope.)
fn read_web_lockfile_document(project_root: &Path) -> MgResult<Option<Lockfile>> {
    let lock_path = project_root.join("mgc.lock");
    match std::fs::symlink_metadata(&lock_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(MgError::Other(format!(
                "cannot inspect lockfile '{}': {error}",
                lock_path.display()
            )));
        }
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => {
            return Err(MgError::Other(format!(
                "refusing non-regular lockfile '{}'",
                lock_path.display()
            )));
        }
    }

    let content = std::fs::read_to_string(&lock_path)
        .map_err(|e| MgError::Other(format!("Failed to read lockfile: {}", e)))?;

    // Dual-format reader: TOML chuẩn v2 (import/migrate ghi) trước, JSON-flavoured
    // (add/install path cũ) fallback — cùng 1 kiểu Lockfile nên chuyển giá vô hình.
    // (Dual-format: canonical v2 TOML first, legacy JSON-flavoured fallback.)
    let lockfile: Lockfile = match mgc_lockfile::parse_lockfile(&content) {
        Ok(lockfile) => lockfile,
        Err(_) => serde_json::from_str(&content)
            .map_err(|e| MgError::Other(format!("Failed to parse lockfile: {}", e)))?,
    };

    maybe_warn_missing_lockfile_checksum(project_root, &lockfile);
    Ok(Some(lockfile))
}

fn scope_web_lock_packages(lockfile: &mut Lockfile, owner_core: &str) {
    let legacy_web_lock = lockfile.version == "2" && owner_core == "web";
    lockfile.packages.retain(|package| {
        package.ecosystem != mgc_lockfile::EcosystemTag::Web
            || package.owner_core.as_deref() == Some(owner_core)
            || (owner_core == "web" && package.owner_core.is_none())
            || (legacy_web_lock && package.ecosystem == mgc_lockfile::EcosystemTag::Other)
    });
}

pub fn maybe_warn_missing_lockfile_checksum(project_root: &Path, lockfile: &Lockfile) {
    if !strict_integrity_enforced() || std::env::var("MAGICORE_WEB_SKIP_LOCKFILE_CHECKSUM").is_ok()
    {
        return;
    }

    let has_locked_content = !lockfile.packages.is_empty();
    if !has_locked_content {
        return;
    }

    static WARNED: OnceLock<Mutex<std::collections::HashSet<PathBuf>>> = OnceLock::new();
    let warned = WARNED.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    let path = project_root.join("mgc.lock");
    let mut guard = match warned.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if guard.insert(path) {
        eprintln!(
            "WARNING: Lockfile checksum file (mgc.lock.sha256) not found - cannot verify integrity"
        );
    }
}

pub fn write_web_lockfile_with_state(
    project_root: &Path,
    graph: &ResolvedGraph,
    registry_url: &str,
) -> MgResult<()> {
    let lock_path = project_root.join("mgc.lock");
    let owner_core = web_lock_owner_core(project_root)?;
    let mut lockfile = read_web_lockfile_document(project_root)?.unwrap_or_else(|| Lockfile {
        // Deliberate v3 bump (Phase 1): newly written web locks target the
        // canonical v3 schema.
        // Nâng lên v3 có chủ đích (Phase 1): lock web mới ghi nhắm schema
        // canonical v3.
        version: LOCKFILE_SCHEMA_VERSION.to_string(),
        metadata: LockfileMetadata {
            generated_at: chrono::Utc::now().to_rfc3339(),
            generator: format!("mgc/{}", env!("CARGO_PKG_VERSION")),
            lockfile_hash: String::new(),
            signer: None,
            dependency_ownership: Vec::new(),
        },
        // Web lockfiles carry no imported root graph — the manifest is
        // the root source of truth; root_dependencies stays empty (P0
        // finding #6: the field exists for IMPORTERS, web keeps []).
        // Lockfile web không mang root graph import — manifest là nguồn
        // chân lý của root; root_dependencies giữ rỗng (P0 finding #6:
        // trường này dành cho IMPORTER, web để []).
        root_dependencies: Vec::new(),
        packages: Vec::new(),
        workspace: None,
        optimizer_profile: None,
    });

    let mut scoped_lock = lockfile.clone();
    scope_web_lock_packages(&mut scoped_lock, &owner_core);
    if web_lockfile_matches_graph(&scoped_lock, graph) {
        return Ok(());
    }
    mgc_lockfile::ensure_lockfile_mutation_allowed(&lock_path)
        .map_err(|error| MgError::Other(error.to_string()))?;

    let local_layout = Layout::new(project_cache_dir(project_root));
    let cache = PackageCache::new(local_layout.cache_dir())
        .map_err(|e| MgError::Store(e.to_string()))
        .ok();

    // Update metadata
    lockfile.metadata.generated_at = chrono::Utc::now().to_rfc3339();
    lockfile.metadata.generator = format!("mgc/{}", env!("CARGO_PKG_VERSION"));

    // Update packages
    let next_web_packages: Vec<Package> = graph
        .packages
        .iter()
        .map(|pkg| {
            let integrity = if pkg.integrity.is_empty() {
                if let Some(ref cache) = cache {
                    if let Ok(Some(bytes)) = cache.get_tarball(&pkg.id) {
                        compute_tarball_integrity(&bytes)
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                pkg.integrity.clone()
            };

            Package {
                owner_core: Some(owner_core.clone()),
                name: pkg.id.name_str().to_string(),
                version: pkg.id.version().to_string(),
                resolved: pkg.tarball_url.clone(),
                integrity,
                dependencies: pkg.deps.iter().map(ToString::to_string).collect(),
                ecosystem: mgc_lockfile::EcosystemTag::Web,
                // Web is the one MGC-native engine — every entry records its
                // registry source and CAS ref so the lock owns the graph.
                // (Web là engine mgc-native duy nhất — mỗi entry ghi registry
                // source và CAS ref để lock sở hữu graph.)
                registry: Some(format!("npm://{registry_url}")),
                ..Package::default()
            }
        })
        .collect();
    let legacy_v2 = lockfile.version == "2";
    lockfile.packages.retain(|package| {
        !(package.ecosystem == mgc_lockfile::EcosystemTag::Web
            && (package.owner_core.as_deref() == Some(owner_core.as_str())
                || (owner_core == "web" && package.owner_core.is_none()))
            || (legacy_v2 && package.ecosystem == mgc_lockfile::EcosystemTag::Other))
    });
    lockfile.packages.extend(next_web_packages);
    lockfile.version = LOCKFILE_SCHEMA_VERSION.to_string();

    // Write lockfile — CANONICAL TOML v2 (một format duy nhất cho mọi đường ghi;
    // reader vẫn đọc được JSON-flavoured cũ từ các bản trước)
    // (Write CANONICAL TOML v2 — single format across all writers)
    let toml_content = mgc_lockfile::writer::serialize_lockfile(&lockfile)
        .map_err(|e| MgError::Other(format!("TOML serialization failed: {}", e)))?;

    atomic_write_web_lockfile(&lock_path, toml_content.as_bytes())?;

    Ok(())
}

/// Resolve the caller core from the project signature so embedded web
/// engines (Cloud/CDK, App/React Native) do not relabel or prune another
/// core's package entries. Missing metadata retains the historical Web
/// default; malformed metadata fails closed.
/// (Lấy core caller từ chữ ký project để engine web nhúng không ghi đè core khác.)
fn web_lock_owner_core(project_root: &Path) -> MgResult<String> {
    let marker = mgc_config::project::ProjectConfig::read_core_marker(project_root)
        .map_err(|error| MgError::Other(format!("cannot determine lock owner core: {error}")))?;
    let owner = match marker {
        Some(core) => core,
        None => match mgc_config::project::ProjectConfig::load(project_root)
            .map_err(|error| MgError::Other(format!("cannot read project core owner: {error}")))?
        {
            Some(config) => canonical_core_name(&config.ecosystem),
            None => "web".to_string(),
        },
    };
    if !mgc_config::project::ProjectConfig::KNOWN_CORES.contains(&owner.as_str()) {
        return Err(MgError::Other(format!(
            "refusing to write lock entries for unknown core owner '{owner}'"
        )));
    }
    Ok(owner)
}

fn canonical_core_name(value: &str) -> String {
    let core = value.trim().to_ascii_lowercase();
    if core == "cloud" {
        "clo".to_string()
    } else {
        core
    }
}

/// Atomically publish the unified lockfile. The same-directory staging
/// file prevents crashes from leaving a truncated lock visible to other
/// ecosystem adapters.
/// (Ghi lockfile hợp nhất nguyên tử để crash không để lại file cụt.)
fn atomic_write_web_lockfile(path: &Path, bytes: &[u8]) -> MgResult<()> {
    mgc_lockfile::ensure_lockfile_mutation_allowed(path)
        .map_err(|error| MgError::Other(error.to_string()))?;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NONCE: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| MgError::Other("mgc.lock path has no valid filename".to_string()))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| MgError::Other(format!("system clock is invalid: {error}")))?
        .as_nanos();
    let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(
        ".{filename}.web.{}.{}.{nonce}.tmp",
        std::process::id(),
        timestamp
    ));

    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options
            .open(&temp)
            .map_err(|error| MgError::Other(format!("cannot create lock staging file: {error}")))?;
        file.write_all(bytes)
            .map_err(|error| MgError::Other(format!("cannot write lock staging file: {error}")))?;
        file.sync_all()
            .map_err(|error| MgError::Other(format!("cannot sync lock staging file: {error}")))?;
        mgc_lockfile::atomic::atomic_replace_file(&temp, path).map_err(|error| {
            MgError::Other(format!("cannot atomically replace mgc.lock: {error}"))
        })?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| MgError::Other(format!("cannot sync lock directory: {error}")))?;
        Ok(())
    })();

    if let Err(write_error) = result {
        match std::fs::remove_file(&temp) {
            Ok(()) => Err(write_error),
            Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {
                Err(write_error)
            }
            Err(cleanup_error) => Err(MgError::Other(format!(
                "{write_error}; additionally failed to remove staging file '{}': {cleanup_error}",
                temp.display()
            ))),
        }
    } else {
        result
    }
}

pub fn lockfile_satisfies_manifest(lockfile: &Lockfile, manifest: &Manifest) -> bool {
    for dep in manifest.all_dependencies() {
        let Some(lp) = lockfile
            .packages
            .iter()
            .find(|p| is_web_lock_package(lockfile, p) && p.name == dep.name.as_str())
        else {
            return false;
        };

        let Ok(version) = Version::parse(&lp.version) else {
            return false;
        };

        if !dep.range.matches(&version) {
            return false;
        }
    }
    true
}

pub fn build_graph_from_lockfile(
    lockfile: &Lockfile,
    manifest: &Manifest,
) -> MgResult<Option<ResolvedGraph>> {
    let mut packages = Vec::new();
    for dep in manifest.all_dependencies() {
        let Some(lp) = lockfile
            .packages
            .iter()
            .find(|lp| is_web_lock_package(lockfile, lp) && lp.name == dep.name.as_str())
        else {
            return Ok(None);
        };
        let version = Version::parse(&lp.version).map_err(|e| MgError::Other(e.to_string()))?;
        let deps: Vec<PackageId> = lp
            .dependencies
            .iter()
            .filter_map(|d| {
                let dep_pkg = lockfile
                    .packages
                    .iter()
                    .find(|lp| is_web_lock_package(lockfile, lp) && lp.name == *d)?;
                let v = Version::parse(&dep_pkg.version).ok()?;
                Some(PackageId::new(PackageName::new(d).ok()?, v))
            })
            .collect();

        packages.push(ResolvedPackage {
            id: PackageId::new(dep.name.clone(), version),
            integrity: lp.integrity.clone(),
            tarball_url: lp.resolved.clone(),
            deps,
            peer_deps: Vec::new(), // peer_deps removed from new schema
            direct: manifest.find_dep(dep.name.as_str()).is_some(),
            dev: manifest.dev_dependencies.iter().any(|d| d.name == dep.name),
        });
    }
    Ok(Some(ResolvedGraph { packages }))
}
