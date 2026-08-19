//! `install/bin.rs` — Node modules .bin link creation and executable handling.

use std::path::{Path, PathBuf};
use mg_types::adapter::ResolvedPackage;
use mg_types::{MgError, MgResult};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PackageBinField {
    Single(String),
    Multiple(std::collections::HashMap<String, String>),
}

#[derive(Debug, Deserialize)]
pub struct InstalledPackageManifest {
    pub name: String,
    pub bin: Option<PackageBinField>,
}

pub fn rebuild_bin_links(
    node_modules: &Path,
    expected_root: &[&ResolvedPackage],
    affected: &[&ResolvedPackage],
    strict: bool,
) -> MgResult<()> {
    let bin_dir = node_modules.join(".bin");
    if !strict {
        if bin_dir.exists() {
            std::fs::remove_dir_all(&bin_dir).map_err(|err| {
                MgError::Other(format!(
                    "failed to remove stale bin dir '{}': {}",
                    bin_dir.display(),
                    err
                ))
            })?;
        }
        std::fs::create_dir_all(&bin_dir).map_err(|err| {
            MgError::Other(format!(
                "failed to create bin dir '{}': {}",
                bin_dir.display(),
                err
            ))
        })?;
    } else {
        std::fs::create_dir_all(&bin_dir).map_err(|err| {
            MgError::Other(format!(
                "failed to create bin dir '{}': {}",
                bin_dir.display(),
                err
            ))
        })?;
        let vstore_keys: std::collections::HashSet<String> = expected_root
            .iter()
            .map(|pkg| {
                format!(
                    "{}@{}",
                    pkg.id.name().as_str().replace('/', "+"),
                    pkg.id.version()
                )
            })
            .collect();
        for entry in std::fs::read_dir(&bin_dir).map_err(|err| {
            MgError::Other(format!(
                "failed to read bin dir '{}': {}",
                bin_dir.display(),
                err
            ))
        })? {
            let entry = entry.map_err(|err| {
                MgError::Other(format!(
                    "failed to iterate bin dir '{}': {}",
                    bin_dir.display(),
                    err
                ))
            })?;
            let link = entry.path();
            let Ok(target) = std::fs::read_link(&link) else {
                continue;
            };
            let resolved = target.canonicalize();
            let Ok(resolved) = resolved else {
                std::fs::remove_file(&link).map_err(|err| {
                    MgError::Other(format!(
                        "failed to remove dangling bin link '{}': {}",
                        link.display(),
                        err
                    ))
                })?;
                continue;
            };
            let resolved_str = resolved.to_string_lossy();
            if !resolved_str.contains(".megagate/") {
                continue;
            }
            let still_expected = vstore_keys
                .iter()
                .any(|key| resolved_str.contains(&format!(".megagate/{key}/node_modules/")));
            if !still_expected {
                std::fs::remove_file(&link).map_err(|err| {
                    MgError::Other(format!(
                        "failed to remove stale bin link '{}': {}",
                        link.display(),
                        err
                    ))
                })?;
            }
        }
    }

    for pkg in affected {
        let package_dir = node_modules.join(pkg.id.name().as_str());
        for (bin_name, relative_target) in package_bin_entries(&package_dir)? {
            if relative_target
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                eprintln!(
                    "WARNING: Skipping bin '{}' from package '{}' - path contains '..'",
                    bin_name,
                    pkg.id.name_str()
                );
                continue;
            }

            let target = package_dir.join(&relative_target);
            if !target.exists() {
                continue;
            }

            let canonical_target = match target.canonicalize() {
                Ok(path) => path,
                Err(_) => {
                    eprintln!(
                        "WARNING: Skipping bin '{}' from package '{}' - cannot resolve path",
                        bin_name,
                        pkg.id.name_str()
                    );
                    continue;
                }
            };

            let canonical_package_dir = match package_dir.canonicalize() {
                Ok(path) => path,
                Err(_) => package_dir.clone(),
            };

            if !canonical_target.starts_with(&canonical_package_dir) {
                eprintln!(
                    "WARNING: Skipping bin '{}' from package '{}' - target escapes package directory",
                    bin_name,
                    pkg.id.name_str()
                );
                continue;
            }

            let link = bin_dir.join(bin_name);
            create_bin_link(&link, &target)?;
        }
    }

    Ok(())
}

pub fn package_bin_entries(package_dir: &Path) -> MgResult<Vec<(String, PathBuf)>> {
    let package_json = package_dir.join("package.json");
    if !package_json.exists() {
        return Ok(vec![]);
    }

    let manifest: InstalledPackageManifest =
        serde_json::from_str(&std::fs::read_to_string(&package_json)?).map_err(|err| {
            MgError::Other(format!(
                "failed to parse package manifest '{}': {}",
                package_json.display(),
                err
            ))
        })?;

    let entries = match manifest.bin {
        Some(PackageBinField::Single(path)) => vec![(
            manifest
                .name
                .rsplit('/')
                .next()
                .unwrap_or(manifest.name.as_str())
                .to_string(),
            PathBuf::from(path),
        )],
        Some(PackageBinField::Multiple(entries)) => entries
            .into_iter()
            .map(|(name, path)| (name, PathBuf::from(path)))
            .collect(),
        None => vec![],
    };

    Ok(entries)
}

#[cfg(unix)]
pub fn create_bin_link(link: &Path, target: &Path) -> MgResult<()> {
    use std::os::unix::fs::symlink;

    if link.exists() {
        std::fs::remove_file(link).map_err(|err| {
            MgError::Other(format!(
                "failed to remove existing bin link '{}': {}",
                link.display(),
                err
            ))
        })?;
    }

    symlink(target, link).map_err(|err| {
        MgError::Other(format!(
            "failed to create bin link '{}' -> '{}': {}",
            link.display(),
            target.display(),
            err
        ))
    })?;
    Ok(())
}

#[cfg(not(unix))]
pub fn create_bin_link(link: &Path, target: &Path) -> MgResult<()> {
    let command = format!("@echo off\r\n\"{}\" %*\r\n", target.display());
    std::fs::write(link.with_extension("cmd"), command).map_err(|err| {
        MgError::Other(format!(
            "failed to create cmd shim for '{}' -> '{}': {}",
            link.display(),
            target.display(),
            err
        ))
    })?;
    Ok(())
}

#[cfg(unix)]
pub fn is_executable(path: &Path) -> MgResult<bool> {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::metadata(path)?.permissions().mode();
    Ok(mode & 0o111 != 0)
}

#[cfg(unix)]
pub fn set_executable(path: &Path, executable: bool) -> MgResult<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = std::fs::metadata(path)?.permissions();
    let mode = if executable { 0o755 } else { 0o644 };
    perms.set_mode(mode);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
pub fn is_executable(_: &Path) -> MgResult<bool> {
    Ok(false)
}

#[cfg(not(unix))]
pub fn set_executable(_: &Path, _: bool) -> MgResult<()> {
    Ok(())
}
