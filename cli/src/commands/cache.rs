use anyhow::{Result, bail};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheTarget {
    All,
    Shared,
    Project,
    Build,
}

impl CacheTarget {
    fn parse(raw: &str) -> Result<Self> {
        match raw {
            "all" => Ok(Self::All),
            "shared" => Ok(Self::Shared),
            "project" => Ok(Self::Project),
            "build" => Ok(Self::Build),
            other => bail!("unknown cache target: {other}"),
        }
    }

    fn includes(self, target: Self) -> bool {
        self == Self::All || self == target
    }
}

#[derive(Debug)]
struct CacheEntry {
    label: &'static str,
    path: PathBuf,
    removable: bool,
}

#[derive(Debug, Deserialize)]
struct WebSharedCacheProjectRef {
    schema_version: u32,
    project_root: String,
    package_roots: Vec<String>,
}

#[derive(Debug, Default)]
struct WebSharedCacheStats {
    pinned_package_bytes: u64,
    unpinned_package_bytes: u64,
    pinned_package_roots: usize,
    unpinned_package_roots: usize,
    project_refs: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct WebProjectCacheStats {
    cas_bytes: u64,
    tarball_bytes: u64,
    resolution_bytes: u64,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct WebProjectPruneStats {
    cas_files: usize,
    tarball_files: usize,
    resolution_files: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct GenericPruneStats {
    files: usize,
}

pub async fn run(
    action: String,
    target: String,
    yes: bool,
    dry_run: bool,
    core: Option<&str>,
) -> Result<()> {
    let action = CacheAction::parse(&action)?;
    let target = CacheTarget::parse(&target)?;
    let entries = cache_entries(target, core, action.includes_build_target(target))?;

    match action {
        CacheAction::Status => print_status(&entries),
        CacheAction::Clean => clean(&entries, yes),
        CacheAction::Prune => prune(&entries, yes, dry_run, core),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheAction {
    Status,
    Clean,
    Prune,
}

impl CacheAction {
    fn parse(raw: &str) -> Result<Self> {
        match raw {
            "status" => Ok(Self::Status),
            "clean" => Ok(Self::Clean),
            "prune" => Ok(Self::Prune),
            other => bail!("unknown cache action: {other}"),
        }
    }

    fn includes_build_target(self, target: CacheTarget) -> bool {
        match self {
            Self::Status => target.includes(CacheTarget::Build),
            Self::Clean | Self::Prune => target == CacheTarget::Build,
        }
    }
}

fn cache_entries(
    target: CacheTarget,
    core: Option<&str>,
    include_build: bool,
) -> Result<Vec<CacheEntry>> {
    let mut entries = Vec::new();

    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if target.includes(CacheTarget::Shared)
        && let Some(root) = dirs::cache_dir()
    {
        let shared = match core {
            Some("web") => root.join("magicore").join("web"),
            Some(core) => root.join("magicore").join(core),
            None => root.join("magicore"),
        };
        entries.push(CacheEntry {
            label: "shared",
            path: shared,
            removable: true,
        });
    }

    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if target.includes(CacheTarget::Project)
        && let Ok(cwd) = std::env::current_dir()
        && let Some(root) = crate::commands::core::shared::find_project_root(&cwd)?
    {
        let project = match core {
            Some("web") => root.join(".magicore").join("cache").join("web"),
            Some(core) => root.join(".magicore").join("cache").join(core),
            None => root.join(".magicore").join("cache"),
        };
        entries.push(CacheEntry {
            label: "project",
            path: project,
            removable: true,
        });
    }

    if include_build {
        let build = workspace_build_cache_path()?;
        entries.push(CacheEntry {
            label: "build",
            path: build,
            removable: true,
        });
    }

    Ok(entries)
}

fn workspace_build_cache_path() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    if let Some(root) = find_cargo_workspace_root(&cwd) {
        return Ok(root.join("target"));
    }
    Ok(cwd.join("target"))
}

fn find_cargo_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    let mut nearest_manifest = None;

    loop {
        let manifest = current.join("Cargo.toml");
        if manifest.exists() {
            nearest_manifest = Some(current.clone());
            if std::fs::read_to_string(&manifest)
                .map(|contents| contents.contains("[workspace]"))
                .unwrap_or(false)
            {
                return Some(current);
            }
        }

        if !current.pop() {
            break;
        }
    }

    nearest_manifest
}

fn print_status(entries: &[CacheEntry]) -> Result<()> {
    if entries.is_empty() {
        println!("No cache paths found.");
        return Ok(());
    }

    for entry in entries {
        let bytes = path_size(&entry.path);
        let exists = entry.path.exists();
        println!(
            "{}\t{}\t{}\t{}",
            entry.label,
            human_bytes(bytes),
            if exists { "exists" } else { "missing" },
            entry.path.display()
        );
        if entry.label == "shared" && entry.path.ends_with("magicore/web") {
            let stats = web_shared_cache_stats(&entry.path);
            println!(
                "shared:web:pinned\t{}\t{} roots\t{} refs",
                human_bytes(stats.pinned_package_bytes),
                stats.pinned_package_roots,
                stats.project_refs
            );
            println!(
                "shared:web:unpinned\t{}\t{} roots",
                human_bytes(stats.unpinned_package_bytes),
                stats.unpinned_package_roots
            );
        } else if entry.label == "project" && entry.path.ends_with(".magicore/cache/web") {
            let stats = web_project_cache_stats(&entry.path);
            println!("project:web:cas\t{}\tcas", human_bytes(stats.cas_bytes));
            println!(
                "project:web:tarballs\t{}\tcache",
                human_bytes(stats.tarball_bytes)
            );
            println!(
                "project:web:resolutions\t{}\tresolutions",
                human_bytes(stats.resolution_bytes)
            );
        }
    }
    Ok(())
}

fn clean(entries: &[CacheEntry], yes: bool) -> Result<()> {
    if !yes {
        println!("Refusing to clean cache without --yes.");
        print_status(entries)?;
        return Ok(());
    }

    for entry in entries {
        if !entry.removable || !entry.path.exists() {
            println!("skip\t{}\t{}", entry.label, entry.path.display());
            continue;
        }
        remove_path(&entry.path)?;
        println!("removed\t{}\t{}", entry.label, entry.path.display());
    }
    Ok(())
}

fn prune(entries: &[CacheEntry], yes: bool, dry_run: bool, core: Option<&str>) -> Result<()> {
    if !yes && !dry_run {
        println!("Refusing to prune cache without --yes.");
        print_status(entries)?;
        return Ok(());
    }

    for entry in entries {
        if entry.label == "shared" && core == Some("web") {
            let count = count_web_shared_unpinned_package_roots(&entry.path)?;
            if !dry_run {
                let removed = prune_web_shared_unpinned_package_roots(&entry.path)?;
                println!(
                    "pruned\t{}\t{} unpinned package roots\t{}",
                    entry.label,
                    removed,
                    entry.path.display()
                );
            } else {
                println!(
                    "would prune\t{}\t{} unpinned package roots\t{}",
                    entry.label,
                    count,
                    entry.path.display()
                );
            }
        } else if entry.label == "project" && core == Some("web") {
            let stats = prune_web_project_cache(&entry.path, dry_run)?;
            println!(
                "{}\t{}\t{} cas files\t{} tarball files\t{} resolution files\t{}",
                if dry_run { "would prune" } else { "pruned" },
                entry.label,
                stats.cas_files,
                stats.tarball_files,
                stats.resolution_files,
                entry.path.display()
            );
        } else if core.is_some() {
            let stats = prune_generic_cache(&entry.path, dry_run)?;
            println!(
                "{}\t{}\t{} files\t{}",
                if dry_run { "would prune" } else { "pruned" },
                entry.label,
                stats.files,
                entry.path.display()
            );
        } else {
            println!(
                "skip\t{}\tprune needs --core for non-web caches",
                entry.label
            );
        }
    }
    Ok(())
}

fn count_web_shared_unpinned_package_roots(root: &Path) -> Result<usize> {
    let pinned = read_web_shared_pinned_package_roots(root);
    let count = web_shared_package_roots(root)
        .into_iter()
        .filter(|path| !pinned.contains(&canonical_or_original(path)))
        .count();
    Ok(count)
}

fn prune_web_shared_unpinned_package_roots(root: &Path) -> Result<usize> {
    let pinned = read_web_shared_pinned_package_roots(root);
    let mut removed = 0usize;
    for package_root in web_shared_package_roots(root) {
        if pinned.contains(&canonical_or_original(&package_root)) {
            continue;
        }
        remove_path(&package_root)?;
        removed += 1;
    }
    cleanup_empty_dirs(&root.join("packages"));
    Ok(removed)
}

fn prune_web_project_cache(root: &Path, dry_run: bool) -> Result<WebProjectPruneStats> {
    Ok(WebProjectPruneStats {
        cas_files: prune_cas_blobs_under(&root.join("cas"), root, dry_run)?,
        tarball_files: prune_files_under(&root.join("cache"), dry_run)?,
        resolution_files: prune_files_under(&root.join("resolutions"), dry_run)?,
    })
}

fn prune_generic_cache(root: &Path, dry_run: bool) -> Result<GenericPruneStats> {
    Ok(GenericPruneStats {
        files: prune_unlinked_files_under(root, dry_run)?,
    })
}

/// Prune CAS blobs using the SQLite refcount (slice 5 of T1): a blob is
/// prunable when it has no live refcount in the DB. FAIL-CLOSED (P0-C,
/// Tech Lead vòng-7 2026-09-13): exports are independent copies since the
/// hardlink removal, so a LIVE blob has nlink == 1 — the old nlink-only
/// fallback (on DB missing/corrupt/unreadable) treated live blobs as
/// unreferenced and DELETED the shared cache projects still warm-claim.
/// When the DB cannot be read, prune REFUSES to run and points at
/// `mgc store doctor` — it never guesses.
/// (Prune blob CAS theo refcount SQLite: xóa blob khi không còn ref sống
/// trong DB. FAIL-CLOSED (P0-C): export là bản sao độc lập từ khi bỏ
/// hardlink nên blob SỐNG có nlink == 1 — fallback nlink-only cũ (khi DB
/// mất/hỏng/không đọc được) coi blob sống là unreferenced và XÓA cache
/// chia sẻ mà project còn đang warm-claim. Khi DB không đọc được, prune
/// TỪ CHỐI chạy và trỏ tới `mgc store doctor` — không bao giờ đoán mò.)
fn prune_cas_blobs_under(cas_root: &Path, store_root: &Path, dry_run: bool) -> Result<usize> {
    if !cas_root.exists() {
        return Ok(0);
    }

    // The DB must ALREADY exist — `Database::open` is open-OR-CREATE: on a
    // missing store.db it would silently build a FRESH empty refset, and
    // pruning against zero claims would delete every live warm-cache blob
    // (exactly the failure mode P0-C exists to prevent). A missing DB is
    // indistinguishable from a lost refset — refuse.
    // (DB phải ĐÃ TỒN TẠI — `Database::open` là open-HOẶC-CREATE: với
    // store.db mất, nó âm thầm dựng refset MỚI RỖNG, và prune theo 0
    // claim sẽ xóa mọi blob warm-cache đang sống (đúng chế độ hỏng mà
    // P0-C tồn tại để chặn). DB vắng không phân biệt được với refset
    // bị mất — từ chối.)
    let db_path = store_root.join("store.db");
    if !db_path.exists() {
        return Err(anyhow::anyhow!(
            "store database missing for prune ({}); refusing to build an empty \
             refset — run `mgc store doctor` first",
            db_path.display()
        ));
    }

    let live: HashSet<String> = match mgc_store::Database::open(&db_path) {
        Ok(db) => match db.list_cas_live_refs() {
            Ok(live_refs) => live_refs.into_iter().collect(),
            // Refcount unreadable on a healthy-looking DB (corrupt table,
            // locked, I/O error) — refuse: nlink is NOT a source of truth
            // after the export-to-independent-copy switch.
            // (Refcount không đọc được trên DB trông khỏe (bảng hỏng, bị
            // khóa, lỗi I/O) — từ chối: nlink KHÔNG còn là nguồn chân lý
            // sau khi export chuyển sang bản sao độc lập.)
            Err(err) => {
                return Err(anyhow::anyhow!(
                    "cannot read CAS refcount for prune (store.db: {err}); \
                     refusing to prune by nlink — run `mgc store doctor`"
                ));
            }
        },
        // DB missing/corrupt/unopenable — refuse to prune. Pruning without
        // refcounts would delete live warm-cache blobs (they are plain
        // files with nlink 1 now).
        // (DB mất/hỏng/không mở được — từ chối prune. Prune không có
        // refcount sẽ xóa blob warm-cache đang sống (giờ chỉ là file
        // thường nlink 1).)
        Err(err) => {
            return Err(anyhow::anyhow!(
                "store database unavailable for prune ({err}); refusing to \
                 prune by nlink — run `mgc store doctor` first"
            ));
        }
    };

    // A blob is prunable when the DB has no live claim for it.
    // (Blob xóa được khi DB không còn claim sống nào cho nó.)
    let mut pruned = 0usize;
    let mut directories = Vec::new();
    for entry in WalkDir::new(cas_root)
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
        let blob_hash = entry
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.split('.').next().unwrap_or(name).to_string())
            .unwrap_or_default();
        if live.contains(&blob_hash) {
            continue;
        }
        pruned += 1;
        if !dry_run {
            remove_path(entry.path())?;
        }
    }
    if !dry_run {
        directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        for dir in directories {
            let _ = std::fs::remove_dir(&dir);
        }
    }
    Ok(pruned)
}

fn prune_files_under(root: &Path, dry_run: bool) -> Result<usize> {
    prune_files_under_with(root, dry_run, |_| true)
}

fn prune_unlinked_files_under(root: &Path, dry_run: bool) -> Result<usize> {
    prune_files_under_with(root, dry_run, file_has_no_external_hardlinks)
}

fn prune_files_under_with<F>(root: &Path, dry_run: bool, should_prune: F) -> Result<usize>
where
    F: Fn(&Path) -> bool,
{
    if !root.exists() {
        return Ok(0);
    }
    let mut pruned = 0usize;
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
        if !entry.file_type().is_file() || !should_prune(entry.path()) {
            continue;
        }
        pruned += 1;
        if !dry_run {
            remove_path(entry.path())?;
        }
    }
    if !dry_run {
        directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        for dir in directories {
            let _ = std::fs::remove_dir(&dir);
        }
    }
    Ok(pruned)
}

fn file_has_no_external_hardlinks(path: &Path) -> bool {
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

fn remove_path(path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };

    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(path)?;
    } else {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn path_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    if path.is_file() {
        return std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    }
    WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .sum()
}

fn web_shared_cache_stats(root: &Path) -> WebSharedCacheStats {
    let pinned = read_web_shared_pinned_package_roots(root);
    let mut stats = WebSharedCacheStats {
        project_refs: count_web_shared_live_project_refs(root),
        ..Default::default()
    };

    for package_root in web_shared_package_roots(root) {
        let size = path_size(&package_root);
        if pinned.contains(&canonical_or_original(&package_root)) {
            stats.pinned_package_roots += 1;
            stats.pinned_package_bytes = stats.pinned_package_bytes.saturating_add(size);
        } else {
            stats.unpinned_package_roots += 1;
            stats.unpinned_package_bytes = stats.unpinned_package_bytes.saturating_add(size);
        }
    }
    stats
}

fn web_project_cache_stats(root: &Path) -> WebProjectCacheStats {
    WebProjectCacheStats {
        cas_bytes: path_size(&root.join("cas")),
        tarball_bytes: path_size(&root.join("cache")),
        resolution_bytes: path_size(&root.join("resolutions")),
    }
}

fn web_shared_package_roots(root: &Path) -> Vec<PathBuf> {
    let packages = root.join("packages");
    if !packages.exists() {
        return Vec::new();
    }

    WalkDir::new(packages)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir())
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| path.join(".magicore-package-root.json").exists())
        .collect()
}

fn read_web_shared_pinned_package_roots(root: &Path) -> HashSet<PathBuf> {
    let refs_root = root.join("refs").join("projects");
    let mut pinned = HashSet::new();
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
        let Ok(reference) = serde_json::from_str::<WebSharedCacheProjectRef>(&contents) else {
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
            pinned.insert(canonical_or_original(Path::new(&package_root)));
        }
    }
    pinned
}

fn count_web_shared_live_project_refs(root: &Path) -> usize {
    let refs_root = root.join("refs").join("projects");
    let Ok(entries) = std::fs::read_dir(refs_root) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                return false;
            }
            let Ok(contents) = std::fs::read_to_string(path) else {
                return false;
            };
            let Ok(reference) = serde_json::from_str::<WebSharedCacheProjectRef>(&contents) else {
                return false;
            };
            reference.schema_version == 1 && PathBuf::from(reference.project_root).exists()
        })
        .count()
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn cleanup_empty_dirs(root: &Path) {
    if !root.exists() {
        return;
    }
    let mut dirs = WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir())
        .map(|entry| entry.path().to_path_buf())
        .collect::<Vec<_>>();
    dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for dir in dirs {
        let _ = std::fs::remove_dir(&dir);
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
#[path = "../test/cache_test.rs"]
mod tests;
