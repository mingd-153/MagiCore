//! `list.rs` — List installed web dependencies in node_modules / lockfile.

use std::path::Path;
use mg_types::adapter::InstalledPackage;
use mg_types::{MgResult, PackageId, Version};

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

            let version = installed_package_version(&path)
                .or_else(|| {
                    lockfile.as_ref().and_then(|lock| {
                        lock.packages
                            .iter()
                            .find(|pkg| pkg.name == dep.name.as_str())
                            .and_then(|pkg| Version::parse(&pkg.version).ok())
                    })
                })
                .unwrap_or_else(|| Version::new(0, 0, 0));

            let integrity = lockfile.as_ref().and_then(|lock| {
                lock.packages
                    .iter()
                    .find(|pkg| pkg.name == dep.name.as_str())
                    .and_then(|pkg| pkg.integrity.clone())
            });
            let is_direct = lockfile
                .as_ref()
                .map(|lock| {
                    lock.packages
                        .iter()
                        .find(|pkg| pkg.name == dep.name.as_str())
                        .map(|pkg| pkg.direct)
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
