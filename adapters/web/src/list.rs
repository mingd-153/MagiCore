//! `list.rs` — List installed web dependencies from materialized package manifests.

use mgc_types::adapter::InstalledPackage;
use mgc_types::{MgError, MgResult, PackageId, Version};
use std::path::Path;

use crate::lockfile::{installed_package_version, read_web_lockfile_checked};
use crate::manifest::parse_manifest;

pub async fn run_list(project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
    let manifest = parse_manifest(project_root)?;
    let lockfile = read_web_lockfile_checked(project_root)?;
    let node_modules = project_root.join("node_modules");
    let mut packages = Vec::new();

    for (label, deps) in manifest.dep_groups() {
        let is_dev = label == "devDependencies";
        for dep in deps {
            let path = node_modules.join(dep.name.as_str());
            if !path.exists() {
                continue;
            }

            let version = installed_package_version(&path).ok_or_else(|| MgError::Unsupported {
                core: "web",
                capability: "list installed package version",
                guidance: format!(
                    "cannot verify the installed version of '{}': its materialized package.json has no valid version; a lock pin alone is not proof of installed state",
                    dep.name.as_str()
                ),
            })?;

            let integrity = lockfile.as_ref().and_then(|lock| {
                lock.packages
                    .iter()
                    .find(|pkg| {
                        pkg.name == dep.name.as_str()
                            && Version::parse(&pkg.version).ok().as_ref() == Some(&version)
                    })
                    .map(|pkg| pkg.integrity.clone())
            });
            let is_direct = lockfile
                .as_ref()
                .map(|lock| {
                    lock.packages
                        .iter()
                        .find(|pkg| pkg.name == dep.name.as_str())
                        // direct field not in v2 schema - check if in manifest dependencies
                        .map(|pkg| {
                            manifest
                                .all_dependencies()
                                .any(|dep| dep.name.as_str() == pkg.name)
                        })
                        .unwrap_or(true)
                })
                .unwrap_or(true);

            packages.push(InstalledPackage {
                id: PackageId::new(dep.name.clone(), version),
                path,
                integrity,
                is_direct,
                is_dev,
            });
        }
    }

    Ok(packages)
}
