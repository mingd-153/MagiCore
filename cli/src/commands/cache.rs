use anyhow::{bail, Result};
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

pub async fn run(action: String, target: String, yes: bool, core: Option<&str>) -> Result<()> {
    let action = CacheAction::parse(&action)?;
    let target = CacheTarget::parse(&target)?;
    let entries = cache_entries(target, core, action.includes_build_target(target))?;

    match action {
        CacheAction::Status => print_status(&entries),
        CacheAction::Clean => clean(&entries, yes),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheAction {
    Status,
    Clean,
}

impl CacheAction {
    fn parse(raw: &str) -> Result<Self> {
        match raw {
            "status" => Ok(Self::Status),
            "clean" => Ok(Self::Clean),
            other => bail!("unknown cache action: {other}"),
        }
    }

    fn includes_build_target(self, target: CacheTarget) -> bool {
        match self {
            Self::Status => target.includes(CacheTarget::Build),
            Self::Clean => target == CacheTarget::Build,
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
}
