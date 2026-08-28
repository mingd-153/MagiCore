//! `install/link_tree.rs` — Package tree linking for strict web installs.
// Liên kết cây package cho install strict — gom reflink/hardlink/copy một chỗ.

use std::path::Path;
use std::sync::OnceLock;

use mgc_platform::reflink::reflink_clone;
use mgc_types::{MgError, MgResult};
use rayon::prelude::*;
use walkdir::WalkDir;

use crate::install::bin::{is_executable, set_executable};
use crate::install::materialize::remove_fs_entry;
use crate::profile::MaterializationProfile;

pub fn hardlink_thread_count() -> usize {
    std::env::var("MAGICORE_WEB_HARDLINK_THREADS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|&count| count > 0)
        .unwrap_or(default_hardlink_threads())
}

pub fn default_hardlink_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 6))
        .unwrap_or(2)
}

pub fn hardlink_pool() -> MgResult<&'static rayon::ThreadPool> {
    static POOL: OnceLock<Result<rayon::ThreadPool, String>> = OnceLock::new();
    let pool = POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(hardlink_thread_count())
            .thread_name(|index| format!("mgc-web-hardlink-{index}"))
            .build()
            .map_err(|err| err.to_string())
    });
    pool.as_ref()
        .map_err(|err| MgError::Other(format!("failed to initialize hardlink thread pool: {err}")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrictTreeLinkMode {
    Symlink,
    Hardlink,
}

pub fn strict_tree_link_mode() -> StrictTreeLinkMode {
    match std::env::var("MAGICORE_WEB_STRICT_TREE_MODE")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "hardlink" | "copy" | "compat" => StrictTreeLinkMode::Hardlink,
        "symlink" | "link" | "fast" => StrictTreeLinkMode::Symlink,
        _ => StrictTreeLinkMode::Hardlink,
    }
}

pub fn link_package_tree(source_root: &Path, target_root: &Path) -> MgResult<()> {
    link_package_tree_with_profile(source_root, target_root, None)
}

pub fn link_package_tree_with_profile(
    source_root: &Path,
    target_root: &Path,
    profile: Option<&MaterializationProfile>,
) -> MgResult<()> {
    if let Some(parent) = target_root.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            MgError::Other(format!(
                "failed to create package link parent '{}': {}",
                parent.display(),
                err
            ))
        })?;
    }
    remove_fs_entry(target_root)?;
    if let Some(profile) = profile {
        profile.record_package_linked();
    }

    if strict_tree_link_mode() == StrictTreeLinkMode::Symlink {
        return crate::layout::create_symlink(source_root, target_root).map_err(|err| {
            MgError::Other(format!(
                "failed to symlink package '{}' -> '{}': {}",
                source_root.display(),
                target_root.display(),
                err
            ))
        });
    }

    hardlink_tree_with_profile(source_root, target_root, profile).map_err(|err| {
        MgError::Other(format!(
            "failed to link package '{}' -> '{}': {}",
            source_root.display(),
            target_root.display(),
            err
        ))
    })
}

pub fn hardlink_tree(source_root: &Path, target_root: &Path) -> MgResult<()> {
    hardlink_tree_with_profile(source_root, target_root, None)
}

pub fn hardlink_tree_with_profile(
    source_root: &Path,
    target_root: &Path,
    profile: Option<&MaterializationProfile>,
) -> MgResult<()> {
    let reflink_enabled = match std::env::var("MAGICORE_WEB_REFLINK") {
        Ok(value) => value != "0",
        Err(_) => true,
    };

    std::fs::create_dir_all(target_root).map_err(|err| {
        MgError::Other(format!(
            "failed to create target '{}': {}",
            target_root.display(),
            err
        ))
    })?;

    let mut directories = Vec::new();
    let mut files = Vec::new();

    for entry in WalkDir::new(source_root) {
        let entry = entry.map_err(|e| MgError::Other(e.to_string()))?;
        let path = entry.path();
        if path == source_root {
            continue;
        }

        let relative = path
            .strip_prefix(source_root)
            .map_err(|e| MgError::Other(e.to_string()))?;
        let target = target_root.join(relative);

        if entry.file_type().is_dir() {
            if let Some(profile) = profile {
                profile.record_directory();
            }
            directories.push(target);
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }
        if let Some(profile) = profile {
            profile.record_file();
        }
        files.push((path.to_path_buf(), target));
    }

    directories.sort_by_key(|path| path.components().count());
    for target in directories {
        std::fs::create_dir_all(&target).map_err(|err| {
            MgError::Other(format!(
                "failed to create directory '{}' while cloning '{}': {}",
                target.display(),
                source_root.display(),
                err
            ))
        })?;
    }

    hardlink_pool()?.install(|| {
        files
            .into_par_iter()
            .try_for_each(|(path, target)| -> MgResult<()> {
                backing_link_file(&path, &target, profile, reflink_enabled)
            })
    })?;

    Ok(())
}

pub fn backing_link_file(
    source: &Path,
    target: &Path,
    profile: Option<&MaterializationProfile>,
    reflink_enabled: bool,
) -> MgResult<()> {
    if reflink_enabled {
        match reflink_clone(source, target) {
            Ok(()) => {
                if let Some(profile) = profile {
                    profile.record_reflink();
                }
                return Ok(());
            }
            Err(mgc_platform::reflink::ReflinkError::Other(_)) if target.exists() => {
                std::fs::remove_file(target).map_err(|err| {
                    MgError::Other(format!(
                        "failed to remove existing file '{}' before reflink: {}",
                        target.display(),
                        err
                    ))
                })?;
                if let Ok(()) = reflink_clone(source, target) {
                    if let Some(profile) = profile {
                        profile.record_reflink();
                    }
                    return Ok(());
                }
            }
            Err(mgc_platform::reflink::ReflinkError::Other(err)) => {
                return Err(MgError::Other(format!(
                    "reflink failed for '{}' -> '{}': {}",
                    source.display(),
                    target.display(),
                    err
                )));
            }
            Err(mgc_platform::reflink::ReflinkError::NotSupported(_)) => {}
        }
    }

    if let Ok(()) = std::fs::hard_link(source, target) {
        if let Some(profile) = profile {
            profile.record_hardlink();
        }
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            MgError::Other(format!(
                "failed to create parent '{}' for '{}': {}",
                parent.display(),
                target.display(),
                err
            ))
        })?;
    }
    if target.exists() {
        std::fs::remove_file(target).map_err(|err| {
            MgError::Other(format!(
                "failed to remove existing file '{}' before clone: {}",
                target.display(),
                err
            ))
        })?;
    }
    if std::fs::hard_link(source, target).is_ok() {
        if let Some(profile) = profile {
            profile.record_hardlink();
        }
        return Ok(());
    }
    std::fs::copy(source, target).map_err(|err| {
        MgError::Other(format!(
            "failed to materialize '{}' to '{}': {}",
            source.display(),
            target.display(),
            err
        ))
    })?;
    set_executable(target, is_executable(source)?)?;
    if let Some(profile) = profile {
        profile.record_copy();
    }
    Ok(())
}
