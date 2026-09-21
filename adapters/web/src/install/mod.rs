//! `install/mod.rs` — WebAdapter install orchestrator and pipeline coordination.

pub mod bin;
pub mod download;
pub mod extract;
pub mod fetch;
pub mod integrity;
pub mod link_tree;
pub mod materialize;
pub mod package_marker;
pub mod script_policy;

use mgc_store::{ContentStore, Database, Layout, PackageCache};
use mgc_types::adapter::{InstallOptions, InstallSummary, ResolvedGraph, ResolvedPackage};
use mgc_types::{MgError, MgResult, PackageId};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::cache::{SharedWebCache, prune_project_local_cache};
use crate::install::bin::rebuild_bin_links;
use crate::install::download::{pipeline_download_and_extract, prefetch_tarballs};
use crate::install::materialize::{
    extracted_root_for, graph_without_packages, hardlink_tree, materialize_nested_dependencies,
    materialize_strict_layout, prune_root_install_dirs, repair_dangling_symlinks,
    reset_nested_node_modules, select_root_packages, strict_vstore_package_dir,
};
pub use crate::install::script_policy::{
    lifecycle_scripts_allowed, load_file_scripts_policy, load_trust_policies,
    should_run_lifecycle_scripts, trust_allows_script,
};
use crate::lifecycle::LifecycleRunner;
use crate::lockfile::installed_package_matches;
use crate::lockfile::{project_cache_dir, write_web_lockfile_with_state};
use crate::native;
use crate::profile::InstallProfile;

/// Pre-install tree backup with crash recovery (P0-2): the live
/// `node_modules` is renamed aside (ONE atomic rename, same filesystem)
/// before any mutation, and the fresh tree materializes in its place.
/// Success deletes the backup (`commit`); ANY failure path — `?` early
/// returns, script errors, even panics — hits `Drop`, which renames the
/// backup home. A SIGKILLed run leaves the backup behind; the next
/// install's `recover_interrupted_install` restores it before doing
/// anything else. This is the whole "transaction": one atomic rename
/// per direction, no partial-tree states.
/// (Backup cây pre-install + phục hồi crash: một rename atomic mỗi
/// chiều, không trạng thái cây dở dang.)
pub(crate) struct TreeBackup {
    live: PathBuf,
    backup: Option<PathBuf>,
}

impl TreeBackup {
    /// No-op backup for skip/no-change installs (nothing will be
    /// rewritten — commit/Drop are harmless no-ops).
    /// (Backup rỗng cho install không đổi gì.)
    pub(crate) fn none(live: &Path) -> Self {
        Self {
            live: live.to_path_buf(),
            backup: None,
        }
    }

    fn backup_path(live: &Path) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        live.parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!(".mgc-prev-{}-{}-{}", std::process::id(), nanos, n))
    }

    /// Move the live tree aside (atomic rename) and recreate an empty
    /// live dir. Absent live tree = nothing to protect. A live path the
    /// owner marked READ-ONLY is refused outright (fail-closed): silently
    /// replacing an intentionally immutable tree would violate the
    /// admin's intent — surface it with the path instead.
    /// (Dời cây live sang bên. Cây read-only thì từ chối rõ ràng.)
    pub(crate) fn take(live: &Path) -> MgResult<Self> {
        // Empty dir carries no state (the fn-top create_dir_all always
        // ensures the path exists) — backing it up would later restore
        // emptiness OVER a freshly materialized tree. Skip it.
        // (Dir trống không có state — backup nó sẽ xóa cây mới materialize.)
        let is_empty = std::fs::read_dir(live)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true);
        if !live.exists() || is_empty {
            return Ok(Self {
                live: live.to_path_buf(),
                backup: None,
            });
        }
        if std::fs::metadata(live)
            .map(|m| m.permissions().readonly())
            .unwrap_or(false)
        {
            return Err(MgError::Other(format!(
                "cannot stage install: existing '{}' is read-only — refusing to replace an immutable tree (adjust permissions to reinstall)",
                live.display()
            )));
        }
        let backup = Self::backup_path(live);
        std::fs::rename(live, &backup).map_err(|e| {
            MgError::Other(format!(
                "cannot stage pre-install backup '{}' -> '{}': {e} (install aborted before mutating the tree)",
                live.display(),
                backup.display()
            ))
        })?;
        std::fs::create_dir_all(live).map_err(|e| {
            MgError::Other(format!(
                "cannot recreate '{}' after backup: {e}",
                live.display()
            ))
        })?;
        Ok(Self {
            live: live.to_path_buf(),
            backup: Some(backup),
        })
    }

    /// Restore the backup NOW, returning every failure as one string.
    /// On success the guard disarms itself (Drop becomes a no-op).
    /// (Khôi phục backup ngay, lỗi gộp thành chuỗi.)
    pub(crate) fn rollback(&mut self) -> Result<(), String> {
        let Some(backup) = self.backup.take() else {
            return Ok(());
        };
        let mut failures: Vec<String> = Vec::new();
        if self.live.exists()
            && let Err(e) = std::fs::remove_dir_all(&self.live)
        {
            failures.push(format!(
                "rollback clear '{}' failed: {e}",
                self.live.display()
            ));
        }
        if failures.is_empty()
            && let Err(e) = std::fs::rename(&backup, &self.live)
        {
            failures.push(format!(
                "rollback rename '{}' -> '{}' failed: {e}",
                backup.display(),
                self.live.display()
            ));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            // Leave backup in place for Drop to retry loudly.
            // (Giữ backup để Drop thử lại ồn ào.)
            self.backup = Some(backup);
            Err(failures.join("; "))
        }
    }

    /// Success path: delete the backup (litter on failure is swept by
    /// the next run's recovery).
    /// (Thành công: xóa backup.)
    pub(crate) fn commit(mut self) {
        if let Some(backup) = self.backup.take()
            && let Err(e) = std::fs::remove_dir_all(&backup)
        {
            eprintln!(
                "[magicore] backup cleanup '{}' failed: {e} (swept on next install)",
                backup.display()
            );
        }
    }
}

impl Drop for TreeBackup {
    fn drop(&mut self) {
        // Every `?` early return, every script arm, every panic lands
        // here when uncommitted: best-effort restore, always loud, never
        // masking (callers add their own error on top).
        // (Mọi đường lỗi chưa commit đều qua đây: cố khôi phục, luôn ồn.)
        if self.backup.is_some()
            && let Err(e) = self.rollback()
        {
            eprintln!("[magicore] backup auto-restore failed: {e}");
        }
    }
}

/// Outcome of crash recovery (unit-assertable).
/// (Kết quả phục hồi crash — assert được.)
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CrashRecovery {
    pub restored_backup: bool,
    pub removed_staging: usize,
    pub removed_snaps: usize,
}

/// Recover from SIGKILLed installs (P0-2): restore the newest backup
/// (last-known-consistent tree), delete stale staging dirs and legacy
/// snapshot litter. Runs under the project install lock, before any
/// mutation. Deterministic: newest backup wins by mtime; anything else
/// with our prefixes is garbage.
/// (Phục hồi sau SIGKILL: dựng backup mới nhất, xóa staging/snapshot
/// cũ — chạy dưới khóa install, trước mọi mutation.)
pub(crate) fn recover_interrupted_install(
    project_root: &Path,
    node_modules: &Path,
    staging_tmp: &Path,
) -> CrashRecovery {
    let mut out = CrashRecovery {
        restored_backup: false,
        removed_staging: 0,
        removed_snaps: sweep_stale_snapshots(project_root),
    };
    // Stale staging dirs (legacy + new) are never live: delete outright.
    // (Staging cũ không bao giờ live: xóa thẳng.)
    for dir in [project_root, staging_tmp] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.file_type().is_dir() {
                continue;
            }
            let name = entry.file_name();
            let s = name.to_string_lossy();
            if (s.starts_with(".mgc-stage-") || s.starts_with("install-stage-"))
                && std::fs::remove_dir_all(&path).is_ok()
            {
                out.removed_staging += 1;
            }
        }
    }
    // Backups: newest by mtime wins; the rest is litter.
    // (Backup: mới nhất theo mtime thắng.)
    let mut backups: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(project_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !entry
                .file_name()
                .to_string_lossy()
                .starts_with(".mgc-prev-")
            {
                continue;
            }
            // Refuse symlinks: only a real dir restores (P1 sweep rule).
            // (Từ chối symlink: chỉ dir thật mới restore.)
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.file_type().is_dir() {
                continue;
            }
            let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            backups.push((mtime, path));
        }
    }
    backups.sort_by_key(|(t, _)| *t);
    let mut backups_iter = backups.into_iter();
    // All but the newest are litter.
    // (Mọi backup trừ mới nhất là rác.)
    let mut newest: Option<PathBuf> = None;
    for (_, path) in backups_iter.by_ref() {
        if let Some(prev) = newest.replace(path) {
            let _ = std::fs::remove_dir_all(prev);
        }
    }
    if let Some(backup) = newest {
        if node_modules.exists() && std::fs::remove_dir_all(node_modules).is_err() {
            return out;
        }
        if std::fs::rename(&backup, node_modules).is_ok() {
            out.restored_backup = true;
        }
    }
    out
}

/// Restore the pre-install lockfile (P0-1): `Some(bytes)` writes them
/// back, `None` (no lock existed) removes the file the failed install
/// wrote. Callers combine the io error with the original failure —
/// never a silent drop, never a fake-safe state.
/// (Khôi phục lock cũ — lỗi gộp với lỗi gốc.)
pub(crate) fn restore_prior_lock_result(
    project_root: &Path,
    prior: &Option<Vec<u8>>,
) -> std::io::Result<()> {
    let path = project_root.join("mgc.lock");
    match prior {
        Some(bytes) => std::fs::write(&path, bytes).map(|_| ()),
        None => std::fs::remove_file(&path).or_else(|e| {
            use std::io::ErrorKind;
            if e.kind() == ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e)
            }
        }),
    }
}

/// Remove stale lifecycle-snapshot dirs (`.mgc-snap-*`) left by
/// SIGKILLed installs. Returns the count removed. ONLY the exact prefix
/// plus a REAL directory is touched: `symlink_metadata` (never following
/// links) gates both the prefix match and the dir check, so a hostile
/// `.mgc-snap-*` symlink can neither cause deletion outside the project
/// nor be "cleaned" into anything. Real trees and files are never
/// candidates. Safe under the project install lock (no concurrent
/// install mutates this dir).
/// (Xóa snapshot cũ do SIGKILL — chỉ đúng prefix + thư mục thật, không
/// follow symlink.)
pub(crate) fn sweep_stale_snapshots(project_root: &Path) -> usize {
    let mut swept = 0usize;
    let Ok(entries) = std::fs::read_dir(project_root) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.file_type().is_dir() {
            continue;
        }
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(".mgc-snap-")
        {
            continue;
        }
        if std::fs::remove_dir_all(&path).is_ok() {
            swept += 1;
        }
    }
    swept
}

#[allow(clippy::too_many_arguments)]
pub async fn run_install(
    registry_url: &str,
    provider_auth_token: Option<&str>,
    store_override: Option<&ContentStore>,
    shared_cache: Option<SharedWebCache>,
    prefetch_handle: Option<tokio::task::JoinHandle<MgResult<u64>>>,
    graph: &ResolvedGraph,
    project_root: &Path,
    opts: InstallOptions,
) -> MgResult<InstallSummary> {
    let start = std::time::Instant::now();
    let mut profile = InstallProfile::from_env();
    let registry = native::npm_registry::NpmRegistry::new_with_token(
        registry_url,
        provider_auth_token.map(str::to_string),
    );
    let store_root = project_cache_dir(project_root);
    let layout = Layout::new(store_root);
    // Context-wrapped dir creation — bare io errors left CI failures
    // undiagnosable ("os error 5" with no path).
    // Bọc context — io error trần khiến CI fail không đoán được đường dẫn.
    std::fs::create_dir_all(layout.root()).map_err(|e| {
        MgError::Other(format!(
            "failed to create layout root '{}': {e}",
            layout.root().display()
        ))
    })?;
    std::fs::create_dir_all(layout.temp_dir()).map_err(|e| {
        MgError::Other(format!(
            "failed to create temp dir '{}': {e}",
            layout.temp_dir().display()
        ))
    })?;

    let cache = PackageCache::new(layout.cache_dir()).map_err(|e| MgError::Store(e.to_string()))?;
    eprintln!("[magicore:debug] install:cache_open=ok");
    let database =
        Some(Database::open(&layout.db_path()).map_err(|e| MgError::Store(e.to_string()))?);
    eprintln!("[magicore:debug] install:db_open=ok");
    let default_store =
        ContentStore::new(layout.cas_dir()).map_err(|e| MgError::Store(e.to_string()))?;
    eprintln!("[magicore:debug] install:cas_open=ok");
    let store = store_override.unwrap_or(&default_store);
    let node_modules = project_root.join("node_modules");
    std::fs::create_dir_all(&node_modules).map_err(|e| {
        MgError::Other(format!(
            "failed to create '{}': {e}",
            node_modules.display()
        ))
    })?;
    let mut summary = InstallSummary::default();

    // Generation-token CAS claims (P0-A, adversarial review vòng-9
    // 2026-09-14): the install OPENS a staging generation and carries its
    // TOKEN end-to-end — every claim files into THIS token, and the final
    // promote names THIS token. The v1 scheme bound claims to a global
    // MAX(generation) (a concurrent install hijacked another's claims) and
    // keyed rows by (project, hash) alone (a re-claimed hash silently kept
    // its OLD generation row, then promote deleted the claim of a blob the
    // new install still needed — plain sequential reinstall). Crash safety
    // unchanged: an unfinished staging generation leaves old ∪ new claims
    // live (over-retention, never a pruned live blob).
    // (Claim CAS theo token generation (P0-A): install MỞ một staging
    // generation và mang TOKEN của nó suốt luồng — mọi claim ghi vào đúng
    // token này, promote cuối gọi đúng token này. Scheme v1 gắn claim vào
    // MAX(generation) toàn cục (install song song cướp claim của nhau) và
    // khóa row chỉ theo (project, hash) (hash claim lại âm thầm giữ row
    // generation CŨ, rồi promote xóa claim của blob mà install mới vẫn cần
    // — reinstall tuần tự thuần). An toàn crash không đổi: staging gen chưa
    // xong để lại claim cũ ∪ mới (giữ thừa, không bao giờ prune blob sống).)
    // Project-level install lock (Gate 11-A item 7, vòng-11): one install
    // per project at a time. Generation tokens keep CAS claims safe, but
    // two installs mutating the same node_modules/lockfile in parallel is
    // a correctness hazard (whoever renames last wins the tree) that no
    // refcount protocol can fix — production package managers serialize
    // project mutation the same way. The lock is an OS file lock: a
    // crashed process releases it automatically. Escape hatch: a second
    // concurrent install fails FAST with a clear error instead of racing.
    // (Khóa install mức project: một install mỗi project tại một thời
    // điểm. Token generation giữ claim CAS an toàn, nhưng 2 install song
    // song mutate cùng node_modules/lockfile là mối nguy correctness (ai
    // rename sau cùng thắng cây) mà không giao thức refcount nào sửa được
    // — package manager production tuần tự hóa mutation project y hệt.
    // Lock là file lock OS: tiến trình đứt tự nhả. Install song song thứ
    // hai fail NGAY với lỗi rõ ràng thay vì chạy đua.)
    let project_key_lock = layout.root().to_string_lossy().to_string();
    // P0 hermetic: the install lock lives under THIS project layout —
    // never the user-global store — so locked-HOME machines and hermetic
    // tests exclude per project without touching ~/.magicore.
    // (Lock install theo project, không chạm store user.)
    let _install_lock =
        mgc_store::ProjectInstallLock::acquire_at(&layout.locks_dir(), &project_key_lock)
            .map_err(|e| MgError::Store(e.to_string()))?;

    // P0-2: recover interrupted installs FIRST (under the lock, before
    // any mutation): stale backups restore, staging/snapshot litter
    // goes. A crash between ANY two steps below resumes here.
    // (Phục hồi install đứt trước mọi mutation.)
    let recovered = recover_interrupted_install(project_root, &node_modules, &layout.temp_dir());
    if recovered.restored_backup || recovered.removed_staging > 0 {
        eprintln!(
            "[magicore] recovered interrupted install (backup restored: {}, staging removed: {}, snaps swept: {})",
            recovered.restored_backup, recovered.removed_staging, recovered.removed_snaps
        );
    }

    // RAII generation guard (Gate 11-A item 6, vòng-11): begin → claims →
    // promote. EVERY early return/panic below drops the guard and
    // auto-ABORTS the still-staging token — a failed install can never
    // leak a staging generation whose claims over-protect blobs. Only a
    // successful promote disarms it (the v2 protocol leaked staging
    // markers on every error path: 12+ `return Err` sites, zero aborts).
    // The i64 token copy threads through the pipeline (a borrow-holding
    // guard cannot cross those call boundaries); the guard itself stays
    // here and owns the lifecycle.
    // (Guard generation RAII: begin → claim → promote. MỌI return sớm/
    // panic bên dưới drop guard và TỰ HỦY token staging — install fail
    // không bao giờ rò staging generation mà claim giữ blob vô hạn. Chỉ
    // promote thành công mới disarm (protocol v2 rò marker staging trên
    // mọi đường lỗi: 12+ chỗ `return Err`, 0 abort). Bản i64 của token
    // xuyên qua pipeline (guard giữ borrow không qua được ranh giới gọi
    // đó); guard ở lại đây và sở hữu vòng đời.)
    let (cas_generation, cas_generation_guard) = match database.as_ref() {
        Some(db) => {
            let project_key = layout.root().to_string_lossy().to_string();
            let guard = db
                .begin_cas_generation_guarded(&layout.db_path(), &project_key)
                .map_err(|e| MgError::Store(e.to_string()))?;
            let token = guard.generation();
            (token, Some(guard))
        }
        None => (0, None),
    };

    // Test-only failpoint (Gate 11-B.2): park here AFTER the generation
    // token is committed to the DB — no SQLite transaction is open, so a
    // SIGKILL here leaves a clean, claim-less staging marker the doctor
    // classifies STALE (leaked begin).
    // (Failpoint chỉ-cho-test: đỗ ở đây SAU khi token generation đã commit
    // vào DB — không có transaction SQLite đang mở, nên SIGKILL ở đây để
    // lại marker staging sạch, không claim, doctor phân loại STALE (begin rò).)
    mgc_store::failpoint::hit("after-generation-begin");

    let thread_id_hash = {
        let tid = std::thread::current().id();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&tid, &mut hasher);
        std::hash::Hasher::finish(&hasher)
    };

    let staging_root = if opts.legacy_flat {
        let root = layout.temp_dir().join(format!(
            "install-stage-{}-{}-{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            thread_id_hash
        ));
        std::fs::create_dir_all(root.join("node_modules")).map_err(|e| {
            MgError::Other(format!(
                "failed to create staging node_modules '{}': {e}",
                root.join("node_modules").display()
            ))
        })?;
        Some(root)
    } else {
        None
    };
    let root_packages = select_root_packages(graph);
    let all_root_matched = !root_packages.is_empty()
        && root_packages.par_iter().all(|pkg| {
            installed_package_matches(&node_modules.join(pkg.id.name().as_str()), &pkg.id)
        });

    if all_root_matched && opts.force_install.is_empty() && !graph.packages.is_empty() {
        let all_vstore_matched = graph.packages.par_iter().all(|pkg| {
            installed_package_matches(&strict_vstore_package_dir(&node_modules, &pkg.id), &pkg.id)
        });
        if all_vstore_matched {
            summary.duration_ms = start.elapsed().as_millis() as u64;
            return Ok(summary);
        }
    }

    // P0-2: the live tree stays put until a mutation is certain.
    // (Cây live giữ nguyên tới khi chắc chắn mutate.)
    let root_package_versions: std::collections::HashMap<String, PackageId> = root_packages
        .iter()
        .map(|pkg| (pkg.id.name_str().to_string(), pkg.id.clone()))
        .collect();
    let package_map: std::collections::HashMap<PackageId, &ResolvedPackage> = graph
        .packages
        .iter()
        .map(|pkg| (pkg.id.clone(), pkg))
        .collect();
    let mut packages_with_scripts: Vec<PathBuf> = Vec::new();

    let already_materialized: std::collections::HashSet<PackageId> = if opts.incremental {
        root_packages
            .par_iter()
            .filter(|pkg| {
                !opts.force_install.contains(&pkg.id)
                    && installed_package_matches(
                        &node_modules.join(pkg.id.name().as_str()),
                        &pkg.id,
                    )
            })
            .map(|pkg| pkg.id.clone())
            .collect()
    } else {
        root_packages
            .par_iter()
            .filter(|pkg| {
                installed_package_matches(&node_modules.join(pkg.id.name().as_str()), &pkg.id)
            })
            .map(|pkg| pkg.id.clone())
            .collect()
    };
    let already_in_virtual_store: std::collections::HashSet<PackageId> = if opts.incremental {
        graph
            .packages
            .par_iter()
            .filter(|pkg| {
                !opts.force_install.contains(&pkg.id)
                    && installed_package_matches(
                        &strict_vstore_package_dir(&node_modules, &pkg.id),
                        &pkg.id,
                    )
            })
            .map(|pkg| pkg.id.clone())
            .collect()
    } else {
        std::collections::HashSet::new()
    };
    let shared_package_cache_for_install = shared_cache
        .as_ref()
        .and_then(|shared| shared.package_cache().ok());
    let local_has_seeded_tarballs = graph
        .packages
        .iter()
        .any(|pkg| cache.contains_tarball(&pkg.id));
    let use_shared_primary =
        !local_has_seeded_tarballs && shared_package_cache_for_install.is_some();
    let active_package_cache = if use_shared_primary {
        shared_package_cache_for_install
            .as_ref()
            .expect("shared package cache checked above")
    } else {
        &cache
    };
    let secondary_shared_cache = if use_shared_primary {
        None
    } else {
        shared_cache.as_ref()
    };

    let fetch_graph = if opts.incremental && !already_in_virtual_store.is_empty() {
        if std::env::var("MAGICORE_WEB_PROFILE_INSTALL").is_ok() {
            eprintln!(
                "[magicore:web:materialize-profile] fetch_graph={} already_in_vstore={} graph_total={}",
                graph_without_packages(graph, &already_in_virtual_store)
                    .packages
                    .len(),
                already_in_virtual_store.len(),
                graph.packages.len()
            );
        }
        graph_without_packages(graph, &already_in_virtual_store)
    } else {
        graph.clone()
    };
    // Offline gate: every fetchable package must already exist in local or shared
    // cache BEFORE any download path — fail closed instead of hitting the network.
    // Cổng offline: mọi package cần fetch phải có sẵn trong cache trước khi chạm
    // network — thiếu là lỗi rõ ràng, tuyệt đối không âm thầm truy cập registry.
    if opts.offline && !fetch_graph.is_empty() {
        let missing: Vec<String> = fetch_graph
            .packages
            .iter()
            .filter(|pkg| {
                !cache.contains_tarball(&pkg.id)
                    && shared_package_cache_for_install
                        .as_ref()
                        .map(|shared| !shared.contains_tarball(&pkg.id))
                        .unwrap_or(true)
            })
            .map(|pkg| pkg.id.to_string())
            .collect();
        if !missing.is_empty() {
            return Err(MgError::Other(format!(
                "offline install: {} package(s) not in local cache and cannot be fetched \
                 without network:\n  {}",
                missing.len(),
                missing.join("\n  ")
            )));
        }
    }
    // Lockfile intent write (P0-1/restore): the lock records the
    // RESOLVED graph (versions + integrity) — including when a later
    // fetch fails (environmental, not a policy rejection). It does NOT
    // certify scripts: if lifecycle scripts fail below, the pre-install
    // bytes snapshotted here are restored (or the file removed when
    // none existed), so no lock ever certifies a script-failed state.
    // (Ghi lock ý định resolve; script fail thì khôi phục bytes cũ.)
    let prior_lock = std::fs::read(project_root.join("mgc.lock")).ok();
    if !fetch_graph.is_empty() {
        write_web_lockfile_with_state(project_root, graph, registry_url).inspect_err(|_e| {
            if let Some(root) = &staging_root {
                let _ = std::fs::remove_dir_all(root);
            }
        })?;
    }
    eprintln!("[magicore:debug] install:prune_cache_start");
    prune_project_local_cache(&layout);
    profile.mark("prune_project_local_cache", start);
    if opts.legacy_flat
        && let Some(handle) = prefetch_handle
    {
        match handle.await {
            Ok(Ok(bytes)) => {
                summary.bytes_from_cache += bytes;
            }
            Ok(Err(e)) => {
                if let Some(root) = &staging_root {
                    let _ = std::fs::remove_dir_all(root);
                }
                return Err(e);
            }
            Err(e) => {
                if let Some(root) = &staging_root {
                    let _ = std::fs::remove_dir_all(root);
                }
                return Err(MgError::Other(format!("prefetch panicked: {e}")));
            }
        }
    } else if let Some(handle) = prefetch_handle {
        handle.abort();
    }
    if opts.legacy_flat && !fetch_graph.is_empty() {
        summary.bytes_from_cache += prefetch_tarballs(
            &fetch_graph,
            &already_materialized,
            active_package_cache,
            secondary_shared_cache,
            &registry,
        )
        .await
        .inspect_err(|_e| {
            if let Some(root) = &staging_root {
                let _ = std::fs::remove_dir_all(root);
            }
        })?;
    }
    profile.mark("prefetch_tarballs", start);

    // P0-2: move the live tree aside (ONE atomic rename) — but ONLY when
    // package content will actually be rewritten (a root package is new
    // or forced). Skip/no-change installs keep the live tree in place:
    // stray files survive, incremental matching keeps working, and
    // prune/bin-link rewrites converge on re-run. Fresh materialization
    // lands in the recreated live dir; success deletes the backup
    // (`commit`); every failure path restores it via Drop.
    // (Chỉ dời cây khi sẽ viết lại content — install skip giữ nguyên cây.)
    let mut tree_backup = if root_packages
        .iter()
        .any(|p| !already_materialized.contains(&p.id))
        || !opts.force_install.is_empty()
    {
        TreeBackup::take(&node_modules)?
    } else {
        TreeBackup::none(&node_modules)
    };

    if opts.legacy_flat {
        let mut extracted_roots = std::collections::HashMap::new();
        profile.mark("prepare_extracted_roots", start);

        for pkg in &root_packages {
            let final_dir = node_modules.join(pkg.id.name().as_str());
            if installed_package_matches(&final_dir, &pkg.id) {
                if let Some(database) = database.as_ref() {
                    database
                        .insert_package(
                            &pkg.id,
                            if pkg.integrity.is_empty() {
                                None
                            } else {
                                Some(pkg.integrity.as_str())
                            },
                        )
                        .map_err(|e| MgError::Store(e.to_string()))?;
                }
                if !opts.incremental || !already_materialized.contains(&pkg.id) {
                    summary.added.push(pkg.id.clone());
                }
                continue;
            }

            let package_root = match extracted_root_for(
                &mut extracted_roots,
                &layout,
                store,
                shared_cache.as_ref(),
                active_package_cache,
                pkg,
                cas_generation,
            ) {
                Ok(root) => root,
                Err(err) => {
                    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
                    if let Some(staging_root) = staging_root.as_ref()
                        && staging_root.exists()
                    {
                        let _ = std::fs::remove_dir_all(staging_root);
                    }
                    return Err(err);
                }
            };
            let materialized_dir = staging_root
                .as_ref()
                .expect("legacy-flat installs always create staging_root")
                .join("node_modules")
                .join(pkg.id.name().as_str());
            if materialized_dir.exists() {
                std::fs::remove_dir_all(&materialized_dir).map_err(|e| {
                    MgError::Other(format!(
                        "failed to remove existing materialized dir '{}': {e}",
                        materialized_dir.display()
                    ))
                })?;
            }
            // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
            if let Err(err) = hardlink_tree(package_root.as_path(), &materialized_dir) {
                if let Some(staging_root) = staging_root.as_ref()
                    && staging_root.exists()
                {
                    let _ = std::fs::remove_dir_all(staging_root);
                }
                return Err(err);
            }
            // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
            if let Some(database) = database.as_ref()
                && let Err(err) = database
                    .insert_package(
                        &pkg.id,
                        if pkg.integrity.is_empty() {
                            None
                        } else {
                            Some(pkg.integrity.as_str())
                        },
                    )
                    .map_err(|e| MgError::Store(e.to_string()))
            {
                if let Some(staging_root) = staging_root.as_ref()
                    && staging_root.exists()
                {
                    let _ = std::fs::remove_dir_all(staging_root);
                }
                return Err(err);
            }
            if !opts.incremental || !already_materialized.contains(&pkg.id) {
                summary.added.push(pkg.id.clone());
            }
        }

        for pkg in &root_packages {
            let staged_dir = staging_root
                .as_ref()
                .expect("legacy-flat installs always create staging_root")
                .join("node_modules")
                .join(pkg.id.name().as_str());
            let final_dir = node_modules.join(pkg.id.name().as_str());
            if !staged_dir.exists() {
                continue;
            }
            if let Some(parent) = final_dir.parent() {
                std::fs::create_dir_all(parent).map_err(|err| {
                    MgError::Other(format!(
                        "failed to create parent '{}' for '{}': {}",
                        parent.display(),
                        pkg.id.name_str(),
                        err
                    ))
                })?;
            }
            if final_dir.exists() {
                std::fs::remove_dir_all(&final_dir).map_err(|err| {
                    MgError::Other(format!(
                        "failed to remove existing install dir '{}' for '{}': {}",
                        final_dir.display(),
                        pkg.id.name_str(),
                        err
                    ))
                })?;
            }
            std::fs::rename(&staged_dir, &final_dir).map_err(|err| {
                MgError::Other(format!(
                    "failed to promote staged package '{}' from '{}' to '{}': {}",
                    pkg.id.name_str(),
                    staged_dir.display(),
                    final_dir.display(),
                    err
                ))
            })?;
        }
    } else {
        for pkg in &root_packages {
            if let Some(database) = database.as_ref() {
                database
                    .insert_package(
                        &pkg.id,
                        if pkg.integrity.is_empty() {
                            None
                        } else {
                            Some(pkg.integrity.as_str())
                        },
                    )
                    .map_err(|e| MgError::Store(e.to_string()))?;
            }
            if !opts.incremental || !already_materialized.contains(&pkg.id) {
                summary.added.push(pkg.id.clone());
            }
        }
    }
    profile.mark("materialize_root_packages", start);
    let mut affected_root_bin_links: Vec<&ResolvedPackage> = Vec::new();
    if opts.legacy_flat {
        affected_root_bin_links = root_packages.to_vec();
        for pkg in &root_packages {
            let package_dir = node_modules.join(pkg.id.name().as_str());
            reset_nested_node_modules(&package_dir)?;
            packages_with_scripts.push(package_dir.clone());
            let mut visiting = std::collections::HashSet::new();
            let mut extracted_roots = std::collections::HashMap::new();
            materialize_nested_dependencies(
                &package_dir,
                pkg,
                &package_map,
                &root_package_versions,
                &layout,
                store,
                shared_cache.as_ref(),
                active_package_cache,
                &mut extracted_roots,
                &mut visiting,
                0,
                &mut packages_with_scripts,
                cas_generation,
            )?;
        }
    } else if fetch_graph.is_empty() {
        profile.mark("prepare_extracted_roots", start);
    } else {
        let pipeline_step_started_at = std::time::Instant::now();
        eprintln!(
            "[magicore:debug] install:pipeline_start pkgs={} materialized={}",
            fetch_graph.packages.len(),
            already_materialized.len()
        );
        let (pipeline_bytes, extracted_roots, persist_handles) = pipeline_download_and_extract(
            &fetch_graph,
            &already_materialized,
            active_package_cache,
            shared_cache.as_ref(),
            Some(&registry),
            &layout,
            store,
            cas_generation,
        )
        .await?;
        eprintln!(
            "[magicore:debug] install:pipeline_done extracted={}",
            extracted_roots.len()
        );
        summary.bytes_from_cache += pipeline_bytes;
        profile.mark_step(
            "pipeline_download_and_extract_step",
            pipeline_step_started_at,
        );
        profile.mark("prepare_extracted_roots", start);
        let fetch_ids = fetch_graph
            .packages
            .iter()
            .map(|pkg| pkg.id.clone())
            .collect::<std::collections::HashSet<_>>();
        let root_packages_to_link = if opts.incremental {
            root_packages
                .iter()
                .copied()
                .filter(|pkg| {
                    fetch_ids.contains(&pkg.id)
                        || !installed_package_matches(
                            &node_modules.join(pkg.id.name().as_str()),
                            &pkg.id,
                        )
                })
                .collect::<Vec<_>>()
        } else {
            root_packages.to_vec()
        };
        affected_root_bin_links = root_packages_to_link.clone();

        let strict_materialize_step_started_at = std::time::Instant::now();
        eprintln!("[magicore:debug] install:strict_materialize_start");
        // Test-only failpoint: park just before the node_modules tree is
        // materialized (claims + CAS blobs are already committed upstream).
        // (Failpoint chỉ-cho-test: đỗ ngay trước khi cây node_modules được
        // materialize (claim + blob CAS đã commit ở upstream).)
        mgc_store::failpoint::hit("before-materialize");
        materialize_strict_layout(
            &node_modules,
            graph,
            &fetch_graph,
            &package_map,
            &root_packages_to_link,
            &layout,
            store,
            shared_cache.as_ref(),
            active_package_cache,
            &mut packages_with_scripts,
            &extracted_roots,
            cas_generation,
        )?;
        // Test-only failpoint: park after materialization committed the
        // hardlink tree (still before promote/refs).
        // (Failpoint chỉ-cho-test: đỗ sau khi materialization đã commit cây
        // hardlink (vẫn trước promote/refs).)
        mgc_store::failpoint::hit("after-materialize");
        eprintln!("[magicore:debug] install:strict_materialize_done");
        profile.mark_step(
            "materialize_strict_layout_step",
            strict_materialize_step_started_at,
        );
        let persist_step_started_at = std::time::Instant::now();
        for handle in persist_handles {
            handle
                .await
                .map_err(|e| MgError::Other(format!("shared cache persist task panicked: {e}")))?;
        }
        profile.mark_step("persist_shared_cache_step", persist_step_started_at);
    }
    profile.mark("materialize_dependency_graph", start);
    if let Some(shared_cache) = shared_cache.as_ref() {
        let _ = shared_cache.write_project_ref(
            project_root,
            graph
                .packages
                .iter()
                .map(|pkg| shared_cache.extracted_package_root(pkg)),
        );
    }
    prune_root_install_dirs(&node_modules, &root_package_versions)?;
    profile.mark("prune_root_install_dirs", start);
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Some(staging_root) = staging_root.as_ref()
        && staging_root.exists()
    {
        std::fs::remove_dir_all(staging_root).map_err(|err| {
            MgError::Other(format!(
                "failed to clean staging root '{}': {}",
                staging_root.display(),
                err
            ))
        })?;
    }
    if !node_modules.join(".bin").exists() && affected_root_bin_links.is_empty() {
        affected_root_bin_links = root_packages.to_vec();
    }
    eprintln!("[magicore:debug] install:rebuild_bin_links_start");
    rebuild_bin_links(
        &node_modules,
        &root_packages,
        &affected_root_bin_links,
        !opts.legacy_flat,
    )?;
    profile.mark("rebuild_bin_links", start);

    // Parse script policy BEFORE lifecycle scripts — a broken [scripts]
    // table must fail the install, not silently drop denials.
    // Parse script policy TRƯỚC lifecycle — bảng [scripts] hỏng phải
    // chặn install, không âm thầm bỏ deny.
    let file_policy = if !opts.ignore_scripts {
        Some(crate::install::script_policy::load_file_scripts_policy(project_root)
            .map_err(|e| MgError::Other(format!(
                "[scripts] policy parse failed — install aborted (use --ignore-scripts to skip): {e}"
            )))?)
    } else {
        None
    };

    if !opts.ignore_scripts {
        // P0-3: trust DB errors abort (fail-closed) — an unreadable DB
        // must never degrade to "no denies".
        let trust_map = load_trust_policies(&layout)?;
        let blanket_scripts = opts.allow_scripts || lifecycle_scripts_allowed();
        let mut scripted_packages = Vec::new();
        for pkg_dir in &packages_with_scripts {
            let package_json = pkg_dir.join("package.json");
            // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
            if package_json.exists()
                && let Ok(contents) = std::fs::read_to_string(&package_json)
                && let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&contents)
            {
                let has_scripts = manifest
                    .get("scripts")
                    .and_then(|s| s.as_object())
                    .map(|scripts| {
                        scripts.contains_key("preinstall")
                            || scripts.contains_key("install")
                            || scripts.contains_key("postinstall")
                    })
                    .unwrap_or(false);
                if has_scripts {
                    let name = manifest
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let version = manifest
                        .get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    let policy = trust_map
                        .get(&format!("{name}@{version}"))
                        .or_else(|| trust_map.get(&name))
                        .map(String::as_str);
                    // ONE merged decision (mgc-config::decide_scripts):
                    // deny wins from either source, then allow, then the
                    // blanket default. No second implementation to drift.
                    // (Một quyết định gộp duy nhất — deny luôn thắng.)
                    match mgc_config::project::decide_scripts(
                        &name,
                        version,
                        file_policy.as_ref().and_then(|o| o.as_ref()),
                        policy,
                        blanket_scripts,
                    ) {
                        mgc_config::project::ScriptVerdict::Deny(reason) => {
                            eprintln!(
                                "[magicore] DENIED lifecycle scripts for {name}@{version} ({reason})"
                            );
                            continue;
                        }
                        mgc_config::project::ScriptVerdict::Allow(_) => {
                            scripted_packages.push(pkg_dir.clone());
                        }
                        mgc_config::project::ScriptVerdict::Undecided => {
                            // No opinion anywhere: fall back to the
                            // blanket default (historical behavior
                            // preserved exactly).
                            // (Không ý kiến: theo blanket default như cũ.)
                            if blanket_scripts {
                                scripted_packages.push(pkg_dir.clone());
                            } else {
                                eprintln!(
                                    "[magicore] skipped lifecycle scripts for {name}@{version} — not approved. Approve with: mgc trust approve {name}"
                                );
                                continue;
                            }
                        }
                    }
                }
            }
        }
        let semaphore = Arc::new(Semaphore::new(8));
        let mut join_set = JoinSet::new();

        for pkg_dir in scripted_packages {
            let project_root = project_root.to_path_buf();
            let Ok(permit) = semaphore.clone().acquire_owned().await else {
                eprintln!("[magicore] warning: lifecycle semaphore closed");
                continue;
            };
            join_set.spawn(async move {
                let _permit = permit;
                LifecycleRunner::run_scripts(&pkg_dir, &project_root)
            });
        }

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(())) => {}
                // A failed lifecycle script fails the install (npm parity:
                // a red postinstall is never a green install). The escape
                // hatch is --ignore-scripts, never silence by default.
                // (Script lỗi thì install fail — escape là --ignore-scripts.)
                Ok(Err(e)) => {
                    // Lifecycle failure: restore the pre-install lock
                    // (no lock certifies this state) AND roll back the
                    // tree backup. Either rollback failing is reported
                    // TOGETHER with the script error — never fake-safe.
                    // (Script fail: khôi phục lock + cây, lỗi nào cũng báo.)
                    //
                    // Test-only park for SIGKILL-during-rollback E2E
                    // (MGC_FAILPOINT=before-rollback-restore): no-op
                    // unless armed.
                    mgc_store::failpoint::hit("before-rollback-restore");
                    let script_err = format!("lifecycle script failed — install aborted: {e}");
                    let mut rb = Vec::new();
                    if let Err(x) = restore_prior_lock_result(project_root, &prior_lock) {
                        rb.push(format!("lock restore failed: {x}"));
                    }
                    if let Err(x) = tree_backup.rollback() {
                        rb.push(format!("tree rollback failed: {x}"));
                    }
                    if rb.is_empty() {
                        return Err(MgError::Other(format!(
                            "{script_err} (lockfile rolled back to pre-install state; re-run with --ignore-scripts to skip lifecycle scripts)"
                        )));
                    }
                    return Err(MgError::Other(format!(
                        "{script_err} (ROLLBACK ALSO FAILED — project may be inconsistent, delete node_modules + mgc.lock and re-run install: {})",
                        rb.join("; ")
                    )));
                }
                Err(e) => {
                    let panic_err =
                        format!("lifecycle script task panicked — install aborted: {e}");
                    let mut rb = Vec::new();
                    if let Err(x) = restore_prior_lock_result(project_root, &prior_lock) {
                        rb.push(format!("lock restore failed: {x}"));
                    }
                    if let Err(x) = tree_backup.rollback() {
                        rb.push(format!("tree rollback failed: {x}"));
                    }
                    if rb.is_empty() {
                        return Err(MgError::Other(panic_err));
                    }
                    return Err(MgError::Other(format!(
                        "{panic_err} (ROLLBACK ALSO FAILED — project may be inconsistent, delete node_modules + mgc.lock and re-run install: {})",
                        rb.join("; ")
                    )));
                }
            }
        }
    }
    // Scripts skipped or succeeded: the new tree stands — delete the
    // backup. Later failures (refs/promote) keep the new tree AND the
    // new lock (a mutually consistent pair).
    // (Script xong/bỏ qua: xóa backup — fail sau đó giữ cặp mới nhất quán.)
    tree_backup.commit();
    profile.mark("lifecycle_scripts", start);

    let project_root_str = project_root.to_string_lossy().to_string();
    if let Some(database) = database.as_ref() {
        // Test-only failpoint: park before the final commit block (refs +
        // promote) begins — the staging generation still has its claims.
        // (Failpoint chỉ-cho-test: đỗ trước khi block commit cuối (refs +
        // promote) bắt đầu — staging generation vẫn còn giữ claim.)
        mgc_store::failpoint::hit("before-commit");
        database
            .clear_all_refs(&project_root_str)
            .map_err(|e| MgError::Store(e.to_string()))?;
        for pkg in &graph.packages {
            database
                .set_ref(&project_root_str, &pkg.id)
                .map_err(|e| MgError::Store(e.to_string()))?;
        }
        // Promote THIS install's generation token (P0-A): only NOW — after
        // materialization, lockfile and scripts — does the refset retire,
        // and it retires by TOKEN, not by a global MAX(generation). Until
        // here, prune sees old ∪ new claims (safe over-retention); after
        // here, exactly the claims of this install's graph survive.
        // (Thăng cấp theo TOKEN generation của install này (P0-A): chỉ BÂY
        // GIỜ — sau materialization, lockfile và scripts — refset mới nghỉ
        // hưu, và nghỉ theo TOKEN, không theo MAX(generation) toàn cục.
        // Trước điểm này prune thấy claim cũ ∪ mới (giữ thừa an toàn); sau
        // điểm này, đúng claim của graph install này sống sót.)
        //
        // KEY FIX (vòng-11 Gate 11-A): promote MUST use the SAME project
        // key the token was registered under — `layout.root()` (the
        // project's per-store identity). The WIP run promoted under the
        // BARE `project_root` string while begin/claims registered under
        // `layout.root()`, so promote hit `UnknownToken` ("token 1 not
        // registered") and the whole install failed; the armed guard's
        // Drop then aborted the still-staging token — which made the
        // marker VANISH "mid-flight" and masquerade as a marker deletion.
        // All token-protocol mutations (begin/claim/promote/abort) must
        // share ONE key: `layout.root()`.
        // (Sửa khóa: promote PHẢI dùng đúng khóa project mà token đã đăng
        // ký — `layout.root()` (định danh per-store của project). Bản WIP
        // promote theo chuỗi `project_root` trần trong khi begin/claim đăng
        // ký theo `layout.root()`, nên promote dính `UnknownToken` ("token
        // 1 not registered") và cả install fail; guard còn armed bị Drop
        // abort token staging — khiến marker "biến mất giữa chừng" và ngụy
        // trang thành deletion. Mọi mutation của giao thức token phải dùng
        // MỘT khóa: `layout.root()`.)
        let cas_project_key = layout.root().to_string_lossy().to_string();
        // Test-only failpoint: park right before promote flips the token's
        // generation to 'promoted' (refs already committed above).
        // (Failpoint chỉ-cho-test: đỗ ngay trước khi promote lật generation
        // của token sang 'promoted' (refs đã commit ở trên).)
        mgc_store::failpoint::hit("before-promote");
        database
            .promote_cas_generation(&cas_project_key, cas_generation)
            .map_err(|e| MgError::Store(e.to_string()))?;
        // Test-only failpoint: park just after the generation flipped to
        // 'promoted' — the store is committed but the guard is still armed.
        // (Failpoint chỉ-cho-test: đỗ ngay sau khi generation lật sang
        // 'promoted' — store đã commit nhưng guard vẫn còn armed.)
        mgc_store::failpoint::hit("after-generation-flip");
        // Promote succeeded — disarm the RAII guard so its Drop does NOT
        // abort the now-promoted token (Gate 11-A item 6).
        // (Promote thành công — disarm guard RAII để Drop không hủy token
        // vừa promoted (Gate 11-A mục 6).)
        if let Some(guard) = cas_generation_guard {
            guard.disarm();
        }
        // Test-only failpoint: park after the final commit block completed
        // (refs + promote + disarm) — the install is fully committed.
        // (Failpoint chỉ-cho-test: đỗ sau khi block commit cuối hoàn tất
        // (refs + promote + disarm) — install đã commit trọn vẹn.)
        mgc_store::failpoint::hit("after-commit");
    }
    if opts.repair {
        repair_dangling_symlinks(&node_modules)?;
    }

    summary.duration_ms = start.elapsed().as_millis() as u64;
    profile.flush(summary.duration_ms);
    Ok(summary)
}
