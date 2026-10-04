//! `lockfile.rs` — Lockfile reading, writing, verification and graph reconstruction for WebAdapter.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use base64::Engine;
use chrono;
use mgc_lockfile::{LOCKFILE_SCHEMA_VERSION, Lockfile, LockfileMetadata, Package};
use mgc_store::{Layout, PackageCache};
use mgc_types::{
    Manifest, MgError, MgResult, PackageId, Version, adapter::ResolvedGraph,
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
    let bytes = match mgc_lockfile::read_lockfile_bytes(&lock_path) {
        Ok(bytes) => bytes,
        Err(mgc_lockfile::LockfileError::IoError(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            return Ok(None);
        }
        Err(error) => {
            return Err(MgError::Other(format!("Failed to read lockfile: {error}")));
        }
    };
    let content = String::from_utf8(bytes)
        .map_err(|error| MgError::Other(format!("Lockfile is not valid UTF-8: {error}")))?;

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

/// Fresh v3 lock skeleton (extracted from the write path so the
/// diverged-v4 branch shares the exact missing-lock default).
/// (Khung lock v3 mới — nhánh v4 lệch dùng chung default với lock thiếu.)
fn fresh_v3_lockfile() -> Lockfile {
    Lockfile {
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
        root_dependencies_by_owner: Default::default(),
        packages: Vec::new(),
        workspace: None,
        optimizer_profile: None,
    }
}

/// v4 document on disk, if the lockfile parses as version 4 — `None`
/// for missing/legacy/unreadable-version locks (legacy reader owns them).
/// (Tài liệu v4 trên đĩa — `None` cho lock thiếu/legacy.)
/// Verify v4 digest and signature math before an adapter consumes or
/// preserves the document. Trust policy remains Warn, matching install.
/// Xác minh digest và chữ ký v4 trước khi adapter dùng hoặc giữ tài liệu;
/// chính sách tin cậy là Warn, đồng nhất với install.
pub(crate) fn verify_v4_document_integrity(
    doc: &mgc_lockfile::canonical::LockfileV4,
) -> MgResult<mgc_lockfile::policy::V4VerifyReport> {
    let report = mgc_lockfile::policy::verify_v4_math(doc)
        .map_err(|error| MgError::Other(format!("Failed to verify v4 lockfile: {error}")))?;
    mgc_lockfile::policy::enforce_policy(&report, mgc_lockfile::policy::LockPolicyMode::Warn, &[])
        .map_err(|error| MgError::Other(format!("Failed to verify v4 lockfile: {error}")))?;
    Ok(report)
}

fn read_v4_document_if_present(
    project_root: &Path,
) -> MgResult<Option<mgc_lockfile::canonical::LockfileV4>> {
    let lock_path = project_root.join("mgc.lock");
    let bytes = match mgc_lockfile::read_lockfile_bytes(&lock_path) {
        Ok(bytes) => bytes,
        Err(mgc_lockfile::LockfileError::IoError(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            return Ok(None);
        }
        Err(error) => {
            return Err(MgError::Other(format!("Failed to read lockfile: {error}")));
        }
    };
    let content = String::from_utf8(bytes)
        .map_err(|error| MgError::Other(format!("Lockfile is not valid UTF-8: {error}")))?;
    if !mgc_lockfile::detect_lockfile_version(&content).is_ok_and(|version| version == 4) {
        return Ok(None);
    }
    let doc = mgc_lockfile::canonical::parse_v4_document(&content)
        .map_err(|error| MgError::Other(format!("Failed to parse lockfile: {error}")))?;
    verify_v4_document_integrity(&doc)?;
    Ok(Some(doc))
}

/// Material-field equality between a replayed v4 graph and the installed
/// graph (id/integrity/URL/deps/peer sets). Flags the writer drops
/// (`direct`, `dev`) are excluded — v3 cannot persist them, so they can
/// never prove divergence. Order-insensitive: adjacency is a set.
/// (Bằng nhau trên trường material mà writer giữ; bỏ qua flag writer
/// không lưu; không phụ thuộc thứ tự.)
fn resolved_graphs_match_material(first: &ResolvedGraph, second: &ResolvedGraph) -> bool {
    use std::collections::HashSet;
    if first.packages.len() != second.packages.len() {
        return false;
    }
    let key_of = |package: &ResolvedPackage| {
        let mut deps: Vec<String> = package.deps.iter().map(|id| id.to_string()).collect();
        deps.sort();
        let mut peers: Vec<String> = package.peer_deps.iter().map(|id| id.to_string()).collect();
        peers.sort();
        (
            package.id.to_string(),
            package.integrity.clone(),
            package.tarball_url.clone(),
            deps,
            peers,
        )
    };
    let first_set: HashSet<_> = first.packages.iter().map(key_of).collect();
    let second_set: HashSet<_> = second.packages.iter().map(key_of).collect();
    first_set == second_set
}

pub fn write_web_lockfile_with_state(
    project_root: &Path,
    graph: &ResolvedGraph,
    registry_url: &str,
) -> MgResult<()> {
    let lock_path = project_root.join("mgc.lock");
    let owner_core = web_lock_owner_core(project_root)?;
    // v4 on disk: a replayed graph must round-trip byte-identically
    // instead of being rewritten (rebuilding v4 from ResolvedGraph would
    // drop variant/source identity — forbidden flattening). A diverged
    // graph falls back to a fresh v3 write with a LOUD downgrade warning
    // (same shape as the missing-lock default below, never silent).
    // (Lock v4 trên đĩa: graph replay phải giữ nguyên byte; graph lệch
    // thì ghi mới v3 kèm cảnh báo DOWNGRADE rõ ràng, không bao giờ lặng.)
    let v4_on_disk = read_v4_document_if_present(project_root)?;
    if let Some(doc) = &v4_on_disk {
        let replayed = mgc_lockfile::v4_graph::graph_from_v4_lockfile(doc, &owner_core)
            .map_err(|error| MgError::Other(error.to_string()))?;
        if resolved_graphs_match_material(&replayed, graph) {
            return Ok(());
        }
        eprintln!(
            "WARNING: installed graph diverged from the v4 mgc.lock — rewriting as v3 (variant/source identity of the old v4 pins is not preserved; re-run `mgc migrate lock --to v4` for a lossless v4)"
        );
    }
    // A diverged v4 must NOT go through the legacy reader (it cannot
    // parse v4); it takes the same fresh-v3 default as a missing lock.
    // (v4 lệch không qua reader legacy — đi default v3 mới như lock thiếu.)
    let mut lockfile = match v4_on_disk {
        Some(_) => fresh_v3_lockfile(),
        None => read_web_lockfile_document(project_root)?.unwrap_or_else(fresh_v3_lockfile),
    };

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
                peers: (!pkg.peer_deps.is_empty())
                    .then(|| pkg.peer_deps.iter().map(ToString::to_string).collect()),
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
    let direct_roots: Vec<String> = graph
        .packages
        .iter()
        .filter(|package| package.direct)
        .map(|package| package.id.to_string())
        .collect();
    mgc_lockfile::update_owner_root_pins(
        &mut lockfile,
        &owner_core,
        mgc_lockfile::EcosystemTag::Web,
        direct_roots,
    );
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
pub(crate) fn web_lock_owner_core(project_root: &Path) -> MgResult<String> {
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
    owner_core: &str,
) -> MgResult<Option<ResolvedGraph>> {
    let mut packages_by_id = std::collections::HashMap::new();
    let mut ids_by_name: std::collections::HashMap<String, Vec<PackageId>> =
        std::collections::HashMap::new();
    for package in lockfile
        .packages
        .iter()
        .filter(|package| is_web_lock_package(lockfile, package))
    {
        let id = PackageId::parse(&format!("{}@{}", package.name, package.version))?;
        if packages_by_id.insert(id.clone(), package).is_some() {
            // An ambiguous exact identity cannot safely reconstruct edges.
            // (Identity trùng khiến không thể phục hồi cạnh một cách an toàn.)
            return Ok(None);
        }
        ids_by_name
            .entry(id.name_str().to_string())
            .or_default()
            .push(id);
    }

    let mut pinned_roots = std::collections::HashSet::new();
    if let Some(roots) = lockfile.root_dependencies_by_owner.get(owner_core) {
        for root in roots {
            let parsed = mgc_lockfile::parse_root_pin(root);
            if parsed
                .ecosystem
                .is_some_and(|ecosystem| ecosystem != mgc_lockfile::EcosystemTag::Web)
            {
                continue;
            }
            let Ok(id) = PackageId::parse(parsed.package_id) else {
                return Ok(None);
            };
            if packages_by_id.contains_key(&id) {
                pinned_roots.insert(id);
            }
        }
    }
    if pinned_roots.is_empty() && lockfile.version == "2" {
        for root in &lockfile.root_dependencies {
            if let Ok(id) = PackageId::parse(root)
                && packages_by_id.contains_key(&id)
            {
                pinned_roots.insert(id);
            }
        }
    }

    let mut root_ids = Vec::new();
    for dependency in manifest.all_dependencies() {
        let candidates: Vec<PackageId> = if pinned_roots.is_empty() {
            ids_by_name
                .get(dependency.name.as_str())
                .into_iter()
                .flatten()
                .filter(|id| dependency.range.matches(id.version()))
                .cloned()
                .collect()
        } else {
            pinned_roots
                .iter()
                .filter(|id| {
                    id.name_str() == dependency.name.as_str()
                        && dependency.range.matches(id.version())
                })
                .cloned()
                .collect()
        };
        if candidates.len() != 1 {
            // Missing or ambiguous root pins fall back to a fresh resolution;
            // never silently choose a different version.
            // (Thiếu hoặc trùng root pin thì resolve mới; không tự chọn version khác.)
            return Ok(None);
        }
        root_ids.push(candidates[0].clone());
    }

    let mut pending = std::collections::VecDeque::from(root_ids.clone());
    let mut visited = std::collections::HashSet::new();
    let mut graph_packages = Vec::new();
    while let Some(id) = pending.pop_front() {
        if !visited.insert(id.clone()) {
            continue;
        }
        let Some(package) = packages_by_id.get(&id).copied() else {
            return Ok(None);
        };
        let mut dependencies = Vec::with_capacity(package.dependencies.len());
        for edge in &package.dependencies {
            let Some(target) = locked_edge_target(edge, &packages_by_id, &ids_by_name) else {
                return Ok(None);
            };
            pending.push_back(target.clone());
            dependencies.push(target);
        }
        let mut peer_dependencies = Vec::new();
        for edge in package.peers.as_deref().unwrap_or_default() {
            let Some(target) = locked_edge_target(edge, &packages_by_id, &ids_by_name) else {
                return Ok(None);
            };
            pending.push_back(target.clone());
            peer_dependencies.push(target);
        }
        graph_packages.push(ResolvedPackage {
            id: id.clone(),
            integrity: package.integrity.clone(),
            tarball_url: package.resolved.clone(),
            deps: dependencies,
            peer_deps: peer_dependencies,
            direct: root_ids.contains(&id),
            dev: root_ids.contains(&id)
                && manifest
                    .dev_dependencies
                    .iter()
                    .any(|dependency| dependency.name.as_str() == id.name_str()),
        });
    }
    Ok(Some(ResolvedGraph {
        packages: graph_packages,
    }))
}

/// Resolve a lock edge by exact package identity; old locks with name-only
/// edges are accepted only when that name has one unambiguous version.
/// (Phục hồi cạnh bằng identity chính xác; lock cũ chỉ có tên được nhận khi
/// tên đó có đúng một version, tránh nối nhầm khi có nhiều version.)
fn locked_edge_target(
    edge: &str,
    packages_by_id: &std::collections::HashMap<PackageId, &Package>,
    ids_by_name: &std::collections::HashMap<String, Vec<PackageId>>,
) -> Option<PackageId> {
    if let Ok(id) = PackageId::parse(edge) {
        return packages_by_id.contains_key(&id).then_some(id);
    }
    let candidates = ids_by_name.get(edge)?;
    (candidates.len() == 1).then(|| candidates[0].clone())
}
