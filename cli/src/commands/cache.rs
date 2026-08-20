use anyhow::{bail, Result};
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

    if target.includes(CacheTarget::Shared) {
        if let Some(root) = dirs::cache_dir() {
            let shared = match core {
                Some("web") => root.join("megagate").join("web"),
                Some(core) => root.join("megagate").join(core),
                None => root.join("megagate"),
            };
            entries.push(CacheEntry {
                label: "shared",
                path: shared,
                removable: true,
            });
        }
    }

    if target.includes(CacheTarget::Project) {
        if let Ok(cwd) = std::env::current_dir() {
            if let Some(root) = crate::commands::core::shared::find_project_root(&cwd)? {
                let project = match core {
                    Some("web") => root.join(".megagate").join("cache").join("web"),
                    Some(core) => root.join(".megagate").join("cache").join(core),
                    None => root.join(".megagate").join("cache"),
                };
                entries.push(CacheEntry {
                    label: "project",
                    path: project,
                    removable: true,
                });
            }
        }
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
        if entry.label == "shared" && entry.path.ends_with("megagate/web") {
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
        } else if entry.label == "project" && entry.path.ends_with(".megagate/cache/web") {
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
        } else {
            println!(
                "skip\t{}\tprune is only implemented for --core web cache",
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

/// Prune CAS blobs using the SQLite refcount (slice 5 of T1): a blob is
/// prunable when it has no live refcount in the DB (or the DB has no row for
/// it). The nlink heuristic stays as the outer safety net — never prune a
/// blob still hardlinked into a live tree. DB missing/corrupt → nlink only.
/// (Prune blob CAS theo refcount SQLite: xóa blob không còn ref nào trong DB
///  (hoặc chưa từng claim). Heuristic nlink vẫn là lưới an toàn ngoài — không
///  bao giờ xóa blob còn hardlink. DB mất/hỏng → chỉ dùng nlink.)
fn prune_cas_blobs_under(cas_root: &Path, store_root: &Path, dry_run: bool) -> Result<usize> {
    if !cas_root.exists() {
        return Ok(0);
    }

    let live: HashSet<String> = match mg_store::Database::open(&store_root.join("store.db")) {
        Ok(db) => {
            let mut set = HashSet::new();
            match db.list_cas_live_refs() {
                Ok(live_refs) => {
                    for hash in live_refs {
                        set.insert(hash);
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        "cas refcount read failed, falling back to nlink-only prune: {err}"
                    );
                    return prune_unlinked_files_under(cas_root, dry_run);
                }
            }
            set
        }
        Err(_) => return prune_unlinked_files_under(cas_root, dry_run),
    };

    // A blob is prunable when the DB has no live claim for it AND no live
    // tree still hardlinks it (nlink outer safety net).
    // (Blob xóa được khi DB không còn claim sống VÀ không cây nào còn
    //  hardlink tới — nlink là lưới an toàn ngoài.)
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
        if !file_has_no_external_hardlinks(entry.path()) {
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
        .filter(|path| path.join(".megagate-package-root.json").exists())
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
mod tests {
    use super::*;

    #[test]
    fn clean_all_does_not_include_build_cache() {
        let entries = cache_entries(
            CacheTarget::All,
            None,
            CacheAction::Clean.includes_build_target(CacheTarget::All),
        )
        .unwrap();
        assert!(
            entries.iter().all(|entry| entry.label != "build"),
            "clean --target all must not delete Rust build artifacts implicitly"
        );
    }

    #[test]
    fn clean_build_includes_build_cache_explicitly() {
        let entries = cache_entries(
            CacheTarget::Build,
            None,
            CacheAction::Clean.includes_build_target(CacheTarget::Build),
        )
        .unwrap();
        assert!(
            entries.iter().any(|entry| entry.label == "build"),
            "clean --target build should include build cache explicitly"
        );
    }

    #[test]
    fn finds_workspace_target_from_nested_crate() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"cli\"]\n",
        )
        .unwrap();
        let nested = root.path().join("cli").join("src");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            root.path().join("cli").join("Cargo.toml"),
            "[package]\nname = \"cli\"\n",
        )
        .unwrap();

        assert_eq!(
            find_cargo_workspace_root(&nested).unwrap(),
            root.path().to_path_buf()
        );
    }

    #[test]
    fn web_shared_prune_keeps_pinned_and_removes_unpinned_package_roots() {
        let cache = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let pinned = cache
            .path()
            .join("packages")
            .join("react")
            .join("18.2.0-demo")
            .join("package");
        let unpinned = cache
            .path()
            .join("packages")
            .join("zod")
            .join("3.22.4-demo")
            .join("package");
        std::fs::create_dir_all(&pinned).unwrap();
        std::fs::create_dir_all(&unpinned).unwrap();
        std::fs::write(pinned.join(".megagate-package-root.json"), "{}").unwrap();
        std::fs::write(pinned.join("index.js"), "react").unwrap();
        std::fs::write(unpinned.join(".megagate-package-root.json"), "{}").unwrap();
        std::fs::write(unpinned.join("index.js"), "zod").unwrap();

        let refs = cache.path().join("refs").join("projects");
        std::fs::create_dir_all(&refs).unwrap();
        std::fs::write(
            refs.join("demo.json"),
            serde_json::json!({
                "schema_version": 1,
                "project_root": project.path().canonicalize().unwrap().to_string_lossy(),
                "updated_at": 1,
                "package_roots": [
                    pinned.canonicalize().unwrap().to_string_lossy()
                ]
            })
            .to_string(),
        )
        .unwrap();

        let removed = prune_web_shared_unpinned_package_roots(cache.path()).unwrap();

        assert_eq!(removed, 1);
        assert!(pinned.join("index.js").exists());
        assert!(!unpinned.exists());
        let stats = web_shared_cache_stats(cache.path());
        assert_eq!(stats.pinned_package_roots, 1);
        assert_eq!(stats.unpinned_package_roots, 0);
    }

    #[test]
    fn web_project_cache_stats_reports_cache_breakdown() {
        let cache = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(cache.path().join("cas")).unwrap();
        std::fs::create_dir_all(cache.path().join("cache")).unwrap();
        std::fs::create_dir_all(cache.path().join("resolutions")).unwrap();
        std::fs::write(cache.path().join("cas").join("blob"), [0u8; 4]).unwrap();
        std::fs::write(cache.path().join("cache").join("tarball.tgz"), [0u8; 3]).unwrap();
        std::fs::write(
            cache.path().join("resolutions").join("graph.json"),
            [0u8; 2],
        )
        .unwrap();

        let stats = web_project_cache_stats(cache.path());

        assert_eq!(
            stats,
            WebProjectCacheStats {
                cas_bytes: 4,
                tarball_bytes: 3,
                resolution_bytes: 2,
            }
        );
    }

    #[test]
    fn web_project_prune_removes_only_safe_cache_files() {
        let root = tempfile::tempdir().unwrap();
        let web = root.path().join(".megagate").join("cache").join("web");
        let cas_blob = web.join("cas").join("ab").join("live");
        let cas_orphan = web.join("cas").join("cd").join("orphan");
        let tarball = web.join("cache").join("pkg").join("1.0.0.tgz");
        let resolution = web.join("resolutions").join("graph.json");
        let live_link = root.path().join("node_modules").join("live");
        std::fs::create_dir_all(cas_blob.parent().unwrap()).unwrap();
        std::fs::create_dir_all(cas_orphan.parent().unwrap()).unwrap();
        std::fs::create_dir_all(tarball.parent().unwrap()).unwrap();
        std::fs::create_dir_all(resolution.parent().unwrap()).unwrap();
        std::fs::create_dir_all(live_link.parent().unwrap()).unwrap();
        std::fs::write(&cas_blob, b"live").unwrap();
        std::fs::write(&cas_orphan, b"orphan").unwrap();
        std::fs::write(&tarball, b"tarball").unwrap();
        std::fs::write(&resolution, b"resolution").unwrap();
        std::fs::hard_link(&cas_blob, &live_link).unwrap();

        let dry_run = prune_web_project_cache(&web, true).unwrap();

        assert_eq!(
            dry_run,
            WebProjectPruneStats {
                cas_files: 1,
                tarball_files: 1,
                resolution_files: 1,
            }
        );
        assert!(cas_orphan.exists());
        assert!(tarball.exists());
        assert!(resolution.exists());

        let pruned = prune_web_project_cache(&web, false).unwrap();

        assert_eq!(pruned, dry_run);
        assert!(cas_blob.exists());
        assert!(live_link.exists());
        assert!(!cas_orphan.exists());
        assert!(!tarball.exists());
        assert!(!resolution.exists());
    }

    #[test]
    fn web_project_prune_keeps_refcount_claimed_cas_blobs() {
        let root = tempfile::tempdir().unwrap();
        let web = root.path().join(".megagate").join("cache").join("web");
        let claimed_blob = web.join("cas").join("ab").join("claimed-hash");
        let orphan_blob = web.join("cas").join("cd").join("orphan-hash");
        std::fs::create_dir_all(claimed_blob.parent().unwrap()).unwrap();
        std::fs::create_dir_all(orphan_blob.parent().unwrap()).unwrap();
        std::fs::write(&claimed_blob, b"claimed").unwrap();
        std::fs::write(&orphan_blob, b"orphan").unwrap();

        let db = mg_store::Database::open(&web.join("store.db")).unwrap();
        db.cas_claim("/proj/demo", "claimed-hash").unwrap();

        let pruned = prune_web_project_cache(&web, false).unwrap();

        assert_eq!(pruned.cas_files, 1);
        assert!(claimed_blob.exists());
        assert!(!orphan_blob.exists());
    }

    #[test]
    fn web_project_prune_corrupt_db_falls_back_to_nlink() {
        let root = tempfile::tempdir().unwrap();
        let web = root.path().join(".megagate").join("cache").join("web");
        let blob = web.join("cas").join("ab").join("blob-hash");
        let live_link = root.path().join("node_modules").join("live");
        std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
        std::fs::create_dir_all(live_link.parent().unwrap()).unwrap();
        std::fs::write(&blob, b"blob").unwrap();
        std::fs::hard_link(&blob, &live_link).unwrap();
        std::fs::write(web.join("store.db"), b"not a sqlite db").unwrap();

        let pruned = prune_web_project_cache(&web, false).unwrap();

        assert_eq!(pruned.cas_files, 0);
        assert!(blob.exists());
        assert!(live_link.exists());
    }
}
