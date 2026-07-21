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

pub async fn run(action: String, target: String, yes: bool, dry_run: bool, core: Option<&str>) -> Result<()> {
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
    if !yes {
        println!("Refusing to prune cache without --yes.");
        print_status(entries)?;
        return Ok(());
    }

    for entry in entries {
        if entry.label == "shared" && core == Some("web") {
            if dry_run {
                let count = count_web_shared_unpinned_package_roots(&entry.path)?;
                println!(
                    "would prune\t{}\t{} unpinned package roots\t{}",
                    entry.label,
                    count,
                    entry.path.display()
                );
            } else {
                let removed = prune_web_shared_unpinned_package_roots(&entry.path)?;
                println!(
                    "pruned\t{}\t{} unpinned package roots\t{}",
                    entry.label,
                    removed,
                    entry.path.display()
                );
            }
        } else {
            println!(
                "skip\t{}\t{}",
                entry.label, "prune is only implemented for --core web shared cache"
            );
        }
    }
    Ok(())
}

fn count_web_shared_unpinned_package_roots(root: &Path) -> Result<usize> {
    let pinned = read_web_shared_pinned_package_roots(root);
    let count = web_shared_package_roots(root)
        .into_iter()
        .filter(|path| !pinned.contains(path))
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
}
