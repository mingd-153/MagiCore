//! `lockfile.rs` — Lockfile reading, writing, verification and graph reconstruction for WebAdapter.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use base64::Engine;
use mg_lockfile::{Lockfile, LockfileMetadata, Package};
use mg_store::{Layout, PackageCache};
use mg_types::{
    adapter::ResolvedGraph, adapter::ResolvedPackage, Manifest, MgError, MgResult, PackageId,
    PackageName, Version,
};
use sha2::{Digest, Sha512};
use chrono;

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

pub fn web_lockfile_matches_graph(lockfile: &Lockfile, graph: &ResolvedGraph) -> bool {
    // Check version is "2" (new schema)
    if lockfile.version != "2" || lockfile.packages.len() != graph.packages.len() {
        return false;
    }

    // Check packages match
    lockfile
        .packages
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
                && (resolved.integrity.is_empty()
                    || locked.integrity == resolved.integrity)
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
    let lock_path = project_root.join("mg.lock");
    if !lock_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&lock_path)
        .map_err(|e| MgError::Other(format!("Failed to read lockfile: {}", e)))?;
    
    let lockfile: Lockfile = serde_json::from_str(&content)
        .map_err(|e| MgError::Other(format!("Failed to parse lockfile: {}", e)))?;

    maybe_warn_missing_lockfile_checksum(project_root, &lockfile);
    Ok(Some(lockfile))
}

pub fn maybe_warn_missing_lockfile_checksum(project_root: &Path, lockfile: &Lockfile) {
    if !strict_integrity_enforced()
        || std::env::var("MEGAGATE_WEB_SKIP_LOCKFILE_CHECKSUM").is_ok()
    {
        return;
    }

    let has_locked_content = !lockfile.packages.is_empty();
    if !has_locked_content {
        return;
    }

    static WARNED: OnceLock<Mutex<std::collections::HashSet<PathBuf>>> = OnceLock::new();
    let warned = WARNED.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    let path = project_root.join("mg.lock");
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
    _state: &str,
) -> MgResult<()> {
    let lock_path = project_root.join("mg.lock");
    let mut lockfile = read_web_lockfile_checked(project_root)?
        .unwrap_or_else(|| Lockfile {
            version: "2".to_string(),
            metadata: LockfileMetadata {
                generated_at: chrono::Utc::now().to_rfc3339(),
                generator: "mg/0.4.0".to_string(),
                lockfile_hash: String::new(),
                signer: None,
            },
            packages: Vec::new(),
        });

    if web_lockfile_matches_graph(&lockfile, graph) {
        return Ok(());
    }

    let local_layout = Layout::new(project_cache_dir(project_root));
    let cache = PackageCache::new(local_layout.cache_dir())
        .map_err(|e| MgError::Store(e.to_string()))
        .ok();

    // Update metadata
    lockfile.metadata.generated_at = chrono::Utc::now().to_rfc3339();
    lockfile.metadata.generator = "mg/0.4.0".to_string();

    // Update packages
    lockfile.packages = graph
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
                name: pkg.id.name_str().to_string(),
                version: pkg.id.version().to_string(),
                resolved: pkg.tarball_url.clone(),
                integrity,
                dependencies: pkg.deps.iter().map(ToString::to_string).collect(),
            }
        })
        .collect();

    // Write lockfile
    let json = serde_json::to_string_pretty(&lockfile)
        .map_err(|e| MgError::Other(format!("JSON serialization failed: {}", e)))?;
    
    std::fs::write(&lock_path, json.as_bytes())
        .map_err(|e| MgError::Other(format!("Failed to write lockfile: {}", e)))?;

    Ok(())
}

pub fn lockfile_satisfies_manifest(lockfile: &Lockfile, manifest: &Manifest) -> bool {
    for dep in manifest.all_dependencies() {
        let Some(lp) = lockfile
            .packages
            .iter()
            .find(|p| p.name == dep.name.as_str())
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
            .find(|lp| lp.name == dep.name.as_str())
        else {
            return Ok(None);
        };
        let version = Version::parse(&lp.version).map_err(|e| MgError::Other(e.to_string()))?;
        let deps: Vec<PackageId> = lp
            .dependencies
            .iter()
            .filter_map(|d| {
                let dep_pkg = lockfile.packages.iter().find(|lp| lp.name == *d)?;
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
