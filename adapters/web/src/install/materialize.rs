//! `install/materialize.rs` — Materialization of dependency tree (strict layout, hardlink/reflink, nested).

use std::path::{Path, PathBuf};

use mgc_store::{ContentStore, Layout, PackageCache};
use mgc_types::adapter::{ResolvedGraph, ResolvedPackage};
use mgc_types::{MgError, MgResult, PackageId};
use rayon::prelude::*;

use crate::cache::SharedWebCache;
use crate::install::extract::{
    ensure_extracted_package_root, materialized_package_matches, read_extracted_package_marker,
    write_materialized_package_marker,
};
pub use crate::install::link_tree::{
    backing_link_file, default_hardlink_threads, hardlink_pool, hardlink_thread_count,
    hardlink_tree, hardlink_tree_with_profile, link_package_tree, link_package_tree_with_profile,
    StrictTreeLinkMode,
};
use crate::lockfile::installed_package_matches;
use crate::profile::MaterializationProfile;

pub fn extracted_root_for(
    extracted_roots: &mut std::collections::HashMap<PackageId, PathBuf>,
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    cache: &PackageCache,
    pkg: &ResolvedPackage,
) -> MgResult<PathBuf> {
    if let Some(existing) = extracted_roots.get(&pkg.id) {
        return Ok(existing.clone());
    }

    let tarball_path = cache.tarball_path(&pkg.id);
    let root = ensure_extracted_package_root(layout, store, shared_cache, pkg, &tarball_path)?;
    extracted_roots.insert(pkg.id.clone(), root.clone());
    Ok(root)
}

pub fn select_root_packages(graph: &ResolvedGraph) -> Vec<&ResolvedPackage> {
    let direct_packages = graph
        .packages
        .iter()
        .filter(|pkg| pkg.direct)
        .collect::<Vec<_>>();
    let mut selected: std::collections::HashMap<String, &ResolvedPackage> =
        std::collections::HashMap::new();

    for pkg in &graph.packages {
        selected
            .entry(pkg.id.name_str().to_string())
            .and_modify(|current| {
                let prefer_candidate = (pkg.direct && !current.direct)
                    || (pkg.direct == current.direct && pkg.id.version() > current.id.version());
                if prefer_candidate {
                    *current = pkg;
                }
            })
            .or_insert(pkg);
    }

    if !direct_packages.is_empty() {
        let single_version_names: std::collections::HashSet<String> = graph
            .packages
            .iter()
            .map(|pkg| pkg.id.name_str().to_string())
            .fold(
                std::collections::HashMap::<String, usize>::new(),
                |mut counts, name| {
                    *counts.entry(name).or_insert(0) += 1;
                    counts
                },
            )
            .into_iter()
            .filter(|(_, count)| *count == 1)
            .map(|(name, _)| name)
            .collect();
        return graph
            .packages
            .iter()
            .filter(|pkg| pkg.direct || single_version_names.contains(pkg.id.name_str()))
            .collect();
    }

    graph
        .packages
        .iter()
        .filter(|pkg| {
            selected
                .get(pkg.id.name_str())
                .map(|selected| selected.id == pkg.id)
                .unwrap_or(false)
        })
        .collect()
}

pub fn remove_fs_entry(path: &Path) -> MgResult<()> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(MgError::Other(format!(
                "failed to inspect filesystem entry '{}': {}",
                path.display(),
                err
            )));
        }
    };

    if meta.file_type().is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(path).map_err(|err| {
            MgError::Other(format!(
                "failed to remove directory '{}': {}",
                path.display(),
                err
            ))
        })?;
    } else {
        std::fs::remove_file(path).map_err(|err| {
            MgError::Other(format!(
                "failed to remove file '{}': {}",
                path.display(),
                err
            ))
        })?;
    }

    Ok(())
}

pub fn reset_nested_node_modules(package_dir: &Path) -> MgResult<()> {
    let nested_node_modules = package_dir.join("node_modules");
    if nested_node_modules.exists() {
        remove_fs_entry(&nested_node_modules)?;
    }
    std::fs::create_dir_all(&nested_node_modules).map_err(|err| {
        MgError::Other(format!(
            "failed to recreate nested node_modules '{}': {}",
            nested_node_modules.display(),
            err
        ))
    })?;
    Ok(())
}

pub fn prune_root_install_dirs(
    node_modules: &Path,
    expected_root_packages: &std::collections::HashMap<String, PackageId>,
) -> MgResult<()> {
    if !node_modules.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(node_modules).map_err(|err| {
        MgError::Other(format!(
            "failed to read node_modules '{}': {}",
            node_modules.display(),
            err
        ))
    })? {
        let entry = entry.map_err(|err| {
            MgError::Other(format!(
                "failed to iterate node_modules '{}': {}",
                node_modules.display(),
                err
            ))
        })?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if name == ".bin" || name == ".magicore" {
            continue;
        }

        if name.starts_with('@') && path.is_dir() {
            for scoped_entry in std::fs::read_dir(&path).map_err(|err| {
                MgError::Other(format!(
                    "failed to read scoped directory '{}': {}",
                    path.display(),
                    err
                ))
            })? {
                let scoped_entry = scoped_entry.map_err(|err| {
                    MgError::Other(format!(
                        "failed to iterate scoped directory '{}': {}",
                        path.display(),
                        err
                    ))
                })?;
                let scoped_name = scoped_entry.file_name().to_string_lossy().to_string();
                let package_name = format!("{}/{}", name, scoped_name);
                if !expected_root_packages.contains_key(&package_name) {
                    remove_fs_entry(&scoped_entry.path())?;
                }
            }

            let mut remaining = std::fs::read_dir(&path).map_err(|err| {
                MgError::Other(format!(
                    "failed to re-read scoped directory '{}': {}",
                    path.display(),
                    err
                ))
            })?;
            if remaining.next().is_none() {
                remove_fs_entry(&path)?;
            }
            continue;
        }

        if !expected_root_packages.contains_key(&name) {
            remove_fs_entry(&path)?;
        }
    }

    Ok(())
}

pub fn strict_vstore_package_dir(node_modules: &Path, package_id: &PackageId) -> PathBuf {
    strict_vstore_node_modules_dir(node_modules, package_id).join(package_id.name().as_str())
}

pub fn repair_dangling_symlinks(node_modules: &Path) -> MgResult<()> {
    let vstore_root = node_modules.join(".magicore");
    let mut fixed = 0usize;
    let mut stack = vec![node_modules.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_symlink = match entry.file_type() {
                Ok(ft) => ft.is_symlink(),
                Err(_) => false,
            };
            if is_symlink {
                if path.exists() {
                    continue;
                }
                let parent = match path.parent() {
                    Some(parent) => parent,
                    None => continue,
                };
                let package_dir = parent.join(path.file_name().unwrap_or_default());
                let name_in_vstore = package_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                let vstore_pkg = vstore_root.join(name_in_vstore);
                if vstore_pkg.exists() {
                    let _ = std::fs::remove_file(&path);
                    link_package_tree(&vstore_pkg, &path)?;
                    fixed += 1;
                }
            } else if path.is_dir() && path != vstore_root {
                stack.push(path);
            }
        }
    }
    if fixed > 0 {
        eprintln!("[magicore] repair: re-linked {} dangling symlink(s)", fixed);
    }
    Ok(())
}

pub fn strict_vstore_node_modules_dir(node_modules: &Path, package_id: &PackageId) -> PathBuf {
    let vstore_pkg_name = format!(
        "{}@{}",
        package_id.name().as_str().replace('/', "+"),
        package_id.version()
    );
    node_modules
        .join(".magicore")
        .join(vstore_pkg_name)
        .join("node_modules")
}

pub fn graph_without_packages(
    graph: &ResolvedGraph,
    excluded: &std::collections::HashSet<PackageId>,
) -> ResolvedGraph {
    ResolvedGraph {
        packages: graph
            .packages
            .iter()
            .filter(|pkg| !excluded.contains(&pkg.id))
            .cloned()
            .collect(),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn materialize_strict_layout(
    node_modules: &Path,
    graph: &ResolvedGraph,
    materialization_graph: &ResolvedGraph,
    package_map: &std::collections::HashMap<PackageId, &ResolvedPackage>,
    root_packages: &[&ResolvedPackage],
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    cache: &PackageCache,
    packages_with_scripts: &mut Vec<std::path::PathBuf>,
    extracted_roots: &std::collections::HashMap<PackageId, PathBuf>,
) -> MgResult<()> {
    let virtual_store = node_modules.join(".magicore");
    if let Err(e) = std::fs::create_dir_all(&virtual_store) {
        return Err(MgError::Other(format!(
            "failed to create virtual store: {}",
            e
        )));
    }
    let materialization_profile = MaterializationProfile::from_env();

    let vstore_dirs: Vec<_> = materialization_graph
        .packages
        .iter()
        .map(|pkg| {
            let pkg_id = &pkg.id;
            let vstore_pkg_dir = strict_vstore_package_dir(node_modules, pkg_id);
            (pkg_id.clone(), vstore_pkg_dir, pkg.clone())
        })
        .collect();
    let vstore_dir_map: std::collections::HashMap<PackageId, PathBuf> = graph
        .packages
        .iter()
        .map(|pkg| {
            (
                pkg.id.clone(),
                strict_vstore_package_dir(node_modules, &pkg.id),
            )
        })
        .collect();

    let materialize_results: Vec<_> = vstore_dirs
        .into_par_iter()
        .map(|(pkg_id, vstore_pkg_dir, pkg)| {
            let package_root = materialization_package_root(
                layout,
                store,
                shared_cache,
                cache,
                extracted_roots,
                &pkg,
            )?;
            let source_marker = read_extracted_package_marker(&package_root)?;
            if !materialized_package_matches(&vstore_pkg_dir, &pkg_id, source_marker.as_ref())? {
                remove_fs_entry(&vstore_pkg_dir)?;
                if let Some(parent) = vstore_pkg_dir.parent() {
                    std::fs::create_dir_all(parent).map_err(|err| {
                        MgError::Other(format!(
                            "failed to create vstore parent '{}': {}",
                            parent.display(),
                            err
                        ))
                    })?;
                }
                link_package_tree_with_profile(
                    &package_root,
                    &vstore_pkg_dir,
                    Some(&materialization_profile),
                )?;
                write_materialized_package_marker(&vstore_pkg_dir, source_marker.as_ref())?;
            }
            Ok::<_, MgError>(vstore_pkg_dir)
        })
        .collect();

    for result in materialize_results {
        packages_with_scripts.push(result?);
    }

    let link_results: Vec<_> = materialization_graph
        .packages
        .par_iter()
        .map(|pkg| {
            let pkg_id = &pkg.id;
            if !vstore_dir_map.contains_key(pkg_id) {
                return Err(MgError::Other(format!(
                    "missing virtual store path for '{}'",
                    pkg_id
                )));
            }
            let vstore_node_modules = strict_vstore_node_modules_dir(node_modules, pkg_id);
            let pkg_local_node_modules = vstore_node_modules
                .join(pkg_id.name().as_str())
                .join("node_modules");

            if !pkg.deps.is_empty() {
                std::fs::create_dir_all(&pkg_local_node_modules).map_err(|err| {
                    MgError::Other(format!(
                        "failed to create strict nested node_modules '{}' for '{}': {}",
                        pkg_local_node_modules.display(),
                        pkg_id.name_str(),
                        err
                    ))
                })?;
            }

            for dep_id in &pkg.deps {
                if let Some(_dep_pkg) = package_map.get(dep_id) {
                    let Some(dep_vstore_pkg_dir) = vstore_dir_map.get(dep_id) else {
                        return Err(MgError::Other(format!(
                            "missing dependency virtual store path for '{}'",
                            dep_id
                        )));
                    };

                    let symlink_path = vstore_node_modules.join(dep_id.name().as_str());
                    crate::layout::create_symlink(dep_vstore_pkg_dir, &symlink_path)?;
                    materialization_profile.record_symlink();

                    let local_symlink_path = pkg_local_node_modules.join(dep_id.name().as_str());
                    crate::layout::create_symlink(dep_vstore_pkg_dir, &local_symlink_path)?;
                    materialization_profile.record_symlink();
                }
            }

            if !pkg.peer_deps.is_empty() {
                std::fs::create_dir_all(&pkg_local_node_modules).map_err(|err| {
                    MgError::Other(format!(
                        "failed to create strict nested node_modules '{}' for peer deps of '{}': {}",
                        pkg_local_node_modules.display(),
                        pkg_id.name_str(),
                        err
                    ))
                })?;
            }
            for peer_id in &pkg.peer_deps {
                if let Some(dep_vstore_pkg_dir) = vstore_dir_map.get(peer_id) {
                    let symlink_path = vstore_node_modules.join(peer_id.name().as_str());
                    crate::layout::create_symlink(dep_vstore_pkg_dir, &symlink_path)?;
                    materialization_profile.record_symlink();
                    let local_symlink_path = pkg_local_node_modules.join(peer_id.name().as_str());
                    crate::layout::create_symlink(dep_vstore_pkg_dir, &local_symlink_path)?;
                    materialization_profile.record_symlink();
                }
            }

            Ok::<_, MgError>(())
        })
        .collect();

    for result in link_results {
        result?;
    }

    for pkg in root_packages {
        let root_link = node_modules.join(pkg.id.name().as_str());
        let Some(vstore_pkg_dir) = vstore_dir_map.get(&pkg.id) else {
            return Err(MgError::Other(format!(
                "missing root virtual store path for '{}'",
                pkg.id
            )));
        };
        crate::layout::create_symlink(vstore_pkg_dir, &root_link)?;
        materialization_profile.record_symlink();
    }

    materialization_profile.flush();
    Ok(())
}

pub fn materialization_package_root(
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    cache: &PackageCache,
    extracted_roots: &std::collections::HashMap<PackageId, PathBuf>,
    pkg: &ResolvedPackage,
) -> MgResult<PathBuf> {
    if let Some(root) = extracted_roots.get(&pkg.id) {
        if root.join("package.json").exists() {
            return Ok(root.clone());
        }
    }

    let local_tarball = cache.tarball_path(&pkg.id);
    if local_tarball.exists() {
        return ensure_extracted_package_root(layout, store, shared_cache, pkg, &local_tarball);
    }

    if let Some(shared_cache) = shared_cache {
        let shared_package_cache = shared_cache
            .package_cache()
            .map_err(|err| MgError::Store(err.to_string()))?;
        let shared_tarball = shared_package_cache.tarball_path(&pkg.id);
        if shared_tarball.exists() {
            return ensure_extracted_package_root(
                layout,
                store,
                Some(shared_cache),
                pkg,
                &shared_tarball,
            );
        }
    }

    Err(MgError::Other(format!(
        "missing extracted root and cached tarball for '{}'",
        pkg.id
    )))
}

#[allow(clippy::too_many_arguments)]
pub fn materialize_nested_dependencies(
    package_dir: &Path,
    pkg: &ResolvedPackage,
    package_map: &std::collections::HashMap<PackageId, &ResolvedPackage>,
    root_package_versions: &std::collections::HashMap<String, PackageId>,
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    cache: &PackageCache,
    extracted_roots: &mut std::collections::HashMap<PackageId, PathBuf>,
    visiting: &mut std::collections::HashSet<String>,
    depth: usize,
    packages_with_scripts: &mut Vec<std::path::PathBuf>,
) -> MgResult<()> {
    const MAX_DEPTH: usize = 50;
    if depth > MAX_DEPTH {
        return Err(MgError::Other(format!(
            "dependency graph too deep (>{}) for '{}'",
            MAX_DEPTH,
            pkg.id.name_str()
        )));
    }
    if !visiting.insert(pkg.id.name_str().to_string()) {
        return Ok(());
    }

    let nested_node_modules = package_dir.join("node_modules");
    std::fs::create_dir_all(&nested_node_modules).map_err(|err| {
        MgError::Other(format!(
            "failed to create nested node_modules '{}' for '{}': {}",
            nested_node_modules.display(),
            pkg.id.name_str(),
            err
        ))
    })?;

    for dep_id in &pkg.deps {
        let Some(dep_pkg) = package_map.get(dep_id) else {
            continue;
        };
        let is_hoisted_match = root_package_versions
            .get(dep_id.name_str())
            .map(|root_id| root_id == dep_id)
            .unwrap_or(false);
        if is_hoisted_match {
            continue;
        }

        let nested_dir = nested_node_modules.join(dep_id.name().as_str());
        if !installed_package_matches(&nested_dir, dep_id) {
            if nested_dir.exists() {
                std::fs::remove_dir_all(&nested_dir).map_err(|err| {
                    MgError::Other(format!(
                        "failed to remove stale nested dependency '{}' for '{}': {}",
                        nested_dir.display(),
                        dep_id,
                        err
                    ))
                })?;
            }

            let package_root =
                extracted_root_for(extracted_roots, layout, store, shared_cache, cache, dep_pkg)?;
            hardlink_tree(package_root.as_path(), &nested_dir)?;
            packages_with_scripts.push(nested_dir.clone());
        }

        materialize_nested_dependencies(
            &nested_dir,
            dep_pkg,
            package_map,
            root_package_versions,
            layout,
            store,
            shared_cache,
            cache,
            extracted_roots,
            visiting,
            depth + 1,
            packages_with_scripts,
        )?;
    }

    visiting.remove(pkg.id.name_str());
    Ok(())
}
