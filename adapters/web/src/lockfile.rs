//! `lockfile.rs` — Lockfile reading, writing, verification and graph reconstruction for WebAdapter.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use base64::Engine;
use mg_lockfile::{serialization, LockPackage, Lockfile, ResolutionMeta};
use mg_store::{Layout, PackageCache};
use mg_types::{
    adapter::ResolvedGraph, adapter::ResolvedPackage, Manifest, MgError, MgResult, PackageId,
    PackageName, Version,
};
use sha2::{Digest, Sha512};

use crate::manifest::{atomic_write, atomic_write_if_changed};

pub fn strict_integrity_enforced() -> bool {
    std::env::var("MEGAGATE_STRICT_INTEGRITY").is_ok()
        || std::env::var("MG_STRICT_INTEGRITY").is_ok()
}

pub fn project_cache_dir(project_root: &Path) -> PathBuf {
    project_root.join(".megagate").join("cache").join("web")
}

pub fn compute_sha512_b64(bytes: &[u8]) -> String {
    let mut hasher = Sha512::new();
    hasher.update(bytes);
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

pub fn compute_tarball_integrity(bytes: &[u8]) -> String {
    format!("sha512-{}", compute_sha512_b64(bytes))
}

pub fn web_lockfile_matches_graph(lockfile: &Lockfile, graph: &ResolvedGraph, state: &str) -> bool {
    if lockfile.version != 1
        || lockfile.core != "web"
        || lockfile.resolution.state != state
        || lockfile.resolution.store != "megagate"
        || lockfile.resolution.package_count != graph.packages.len()
        || lockfile.packages.len() != graph.packages.len()
    {
        return false;
    }

    lockfile
        .packages
        .iter()
        .zip(graph.packages.iter())
        .all(|(locked, resolved)| {
            locked.name == resolved.id.name_str()
                && locked.version == resolved.id.version().to_string()
                && locked.direct == resolved.direct
                && locked.dev == resolved.dev
                && locked.dependencies.len() == resolved.deps.len()
                && locked
                    .dependencies
                    .iter()
                    .zip(resolved.deps.iter())
                    .all(|(left, right)| left == &right.to_string())
                && (resolved.integrity.is_empty()
                    || locked.integrity.as_deref() == Some(resolved.integrity.as_str()))
        })
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
    let lock = mg_lockfile::read_lockfile_checked(project_root)
        .map_err(|err| MgError::Other(err.to_string()))?;
    if let Some(lockfile) = &lock {
        maybe_warn_missing_lockfile_checksum(project_root, lockfile);
    }
    Ok(lock)
}

pub fn maybe_warn_missing_lockfile_checksum(project_root: &Path, lockfile: &Lockfile) {
    if !strict_integrity_enforced()
        || std::env::var("MEGAGATE_WEB_SKIP_LOCKFILE_CHECKSUM").is_ok()
        || mg_lockfile::lockfile_checksum_path(project_root).exists()
    {
        return;
    }

    let has_locked_content = lockfile.resolution.state == "locked"
        || lockfile.resolution.package_count > 0
        || !lockfile.packages.is_empty();
    if !has_locked_content {
        return;
    }

    static WARNED: OnceLock<Mutex<std::collections::HashSet<PathBuf>>> = OnceLock::new();
    let warned = WARNED.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    let path = mg_lockfile::lockfile_path(project_root);
    let mut guard = match warned.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if guard.insert(path) {
        eprintln!(
            "WARNING: Lockfile checksum file (mg.lock.sha256) not found - cannot verify integrity"
        );
    }
}

pub fn write_web_lockfile_with_state(
    project_root: &Path,
    graph: &ResolvedGraph,
    state: &str,
) -> MgResult<()> {
    let lock_path = project_root.join("mg.lock");
    let mut lockfile = read_web_lockfile_checked(project_root)?
        .unwrap_or_else(|| Lockfile::new("web", "frontend"));

    if web_lockfile_matches_graph(&lockfile, graph, state) {
        let checksum_path = mg_lockfile::lockfile_checksum_path(project_root);
        if checksum_path.exists() {
            return Ok(());
        }
    }

    let local_layout = Layout::new(project_cache_dir(project_root));
    let cache = PackageCache::new(local_layout.cache_dir())
        .map_err(|e| MgError::Store(e.to_string()))
        .ok();

    lockfile.version = 1;
    lockfile.core = "web".to_string();
    lockfile.resolution = ResolutionMeta {
        state: state.to_string(),
        store: "megagate".to_string(),
        package_count: graph.packages.len(),
    };
    lockfile.packages = graph
        .packages
        .iter()
        .map(|pkg| {
            let integrity = if pkg.integrity.is_empty() {
                if let Some(ref cache) = cache {
                    if let Ok(Some(bytes)) = cache.get_tarball(&pkg.id) {
                        Some(compute_tarball_integrity(&bytes))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                Some(pkg.integrity.clone())
            };
            LockPackage {
                name: pkg.id.name_str().to_string(),
                version: pkg.id.version().to_string(),
                integrity,
                direct: pkg.direct,
                dev: pkg.dev,
                dependencies: pkg.deps.iter().map(ToString::to_string).collect(),
                peer_deps: pkg.peer_deps.iter().map(ToString::to_string).collect(),
            }
        })
        .collect();

    mg_lockfile::LockfileSigner::sign(&mut lockfile)
        .map_err(|e| MgError::Other(format!("lockfile signing failed: {e}")))?;

    let toml = serialization::to_toml(&lockfile)?;
    let lockfile_changed = atomic_write_if_changed(&lock_path, toml.as_bytes())?;
    let checksum = mg_lockfile::lockfile_checksum(toml.as_bytes());
    let checksum_path = mg_lockfile::lockfile_checksum_path(project_root);
    let checksum_changed = std::fs::read_to_string(&checksum_path)
        .map(|existing| existing.trim() != checksum)
        .unwrap_or(true);
    if lockfile_changed || checksum_changed {
        atomic_write(&checksum_path, checksum.as_bytes())?;
    }

    Ok(())
}

pub fn lockfile_satisfies_manifest(lockfile: &Lockfile, manifest: &Manifest) -> bool {
    for dep in manifest.all_dependencies() {
        let Some(lp) = lockfile
            .packages
            .iter()
            .find(|lp| lp.name == dep.name.as_str())
        else {
            return false;
        };
        let Ok(ver) = Version::parse(&lp.version) else {
            return false;
        };
        if !dep.range.matches(&ver) {
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
            .find(|lp| lp.name == dep.name.as_str())
        else {
            return Ok(None);
        };
        let version =
            Version::parse(&lp.version).map_err(|e| MgError::Other(e.to_string()))?;
        let deps: Vec<PackageId> = lp
            .dependencies
            .iter()
            .filter_map(|d| {
                let dep_pkg = lockfile.packages.iter().find(|lp| lp.name == *d)?;
                let v = Version::parse(&dep_pkg.version).ok()?;
                Some(PackageId::new(PackageName::new(d).ok()?, v))
            })
            .collect();
        let peer_deps: Vec<PackageId> = lp
            .peer_deps
            .iter()
            .filter_map(|d| {
                let dep_pkg = lockfile.packages.iter().find(|lp| lp.name == *d)?;
                let v = Version::parse(&dep_pkg.version).ok()?;
                Some(PackageId::new(PackageName::new(d).ok()?, v))
            })
            .collect();
        packages.push(ResolvedPackage {
            id: PackageId::new(dep.name.clone(), version),
            integrity: lp.integrity.clone().unwrap_or_default(),
            tarball_url: String::new(),
            deps,
            peer_deps,
            direct: manifest.find_dep(dep.name.as_str()).is_some(),
            dev: manifest.dev_dependencies.iter().any(|d| d.name == dep.name),
        });
    }
    Ok(Some(ResolvedGraph { packages }))
}
