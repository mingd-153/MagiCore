//! `update.rs` — Update dependencies to latest matching or newer versions for WebAdapter.

use mgc_types::adapter::UpdatedPackage;
use mgc_types::{MgError, MgResult, PackageName, Version, VersionRange};
use std::path::Path;

use crate::lockfile::read_web_lockfile_checked;
use crate::manifest::{parse_manifest, write_manifest};
use crate::native;
use crate::provider::NpmDependencyProvider;

pub fn preferred_registry_version(
    metadata: &native::npm_registry::PackageMetadata,
) -> Option<String> {
    let stable_max = metadata
        .versions
        .keys()
        .filter_map(|v| Version::parse(v).ok())
        .filter(|v| v.pre.is_none())
        .max()
        .map(|v| v.to_string());

    if let Some(latest) = metadata.dist_tags.get("latest") {
        if let Ok(version) = Version::parse(latest) {
            if version.pre.is_none() {
                return Some(version.to_string());
            }
        }
    }

    stable_max
        .or_else(|| metadata.dist_tags.get("latest").cloned())
        .or_else(|| {
            metadata
                .versions
                .keys()
                .filter_map(|v| Version::parse(v).ok())
                .max()
                .map(|v| v.to_string())
        })
}

pub fn preferred_saved_range(current: &VersionRange, latest: &str) -> MgResult<VersionRange> {
    let raw = current.as_str();
    let next = if raw.starts_with('^') {
        format!("^{latest}")
    } else if raw.starts_with('~') {
        format!("~{latest}")
    } else if raw == "*"
        || raw.is_empty()
        || raw.starts_with(">=")
        || raw.starts_with('>')
        || raw.starts_with("<=")
    {
        format!("^{latest}")
    } else {
        latest.to_string()
    };
    VersionRange::parse(&next)
}

pub async fn run_update(
    project_root: &Path,
    name: Option<&PackageName>,
    registry_url: &str,
    provider: &NpmDependencyProvider,
) -> MgResult<Vec<UpdatedPackage>> {
    let mut manifest = parse_manifest(project_root)?;
    let _registry = native::npm_registry::NpmRegistry::new(registry_url);
    let lockfile = read_web_lockfile_checked(project_root)?;
    let mut updated = Vec::new();

    for deps in [
        &mut manifest.dependencies,
        &mut manifest.dev_dependencies,
        &mut manifest.peer_dependencies,
        &mut manifest.optional_dependencies,
    ] {
        for dep in deps.iter_mut() {
            if let Some(selected) = name {
                if dep.name != *selected {
                    continue;
                }
            }

            let metadata = provider
                .metadata(&dep.name)
                .await
                .map_err(|err| MgError::Network(err.to_string()))?;

            let latest = preferred_registry_version(&metadata).ok_or_else(|| {
                MgError::Other(format!(
                    "unable to infer latest version for '{}'",
                    dep.name.as_str()
                ))
            })?;

            let latest_version = Version::parse(&latest)?;
            if dep.range.matches(&latest_version) {
                continue;
            }

            let from_version = lockfile
                .as_ref()
                .and_then(|lock| {
                    lock.packages
                        .iter()
                        .find(|pkg| pkg.name == dep.name.as_str())
                        .map(|pkg| pkg.version.clone())
                })
                .unwrap_or_else(|| dep.range.to_string());

            dep.range = preferred_saved_range(&dep.range, &latest)?;
            updated.push(UpdatedPackage {
                name: dep.name.as_str().to_string(),
                from_version,
                to_version: latest,
            });
        }
    }

    if !updated.is_empty() {
        write_manifest(project_root, &manifest)?;
    }

    Ok(updated)
}
