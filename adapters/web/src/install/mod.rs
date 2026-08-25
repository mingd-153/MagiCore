//! `install/mod.rs` — WebAdapter install orchestrator and pipeline coordination.

pub mod bin;
pub mod download;
pub mod extract;
pub mod integrity;
pub mod link_tree;
pub mod materialize;
pub mod package_marker;
pub mod script_policy;

use mgc_store::{ContentStore, Database, Layout, PackageCache};
use mgc_types::adapter::{InstallOptions, InstallSummary, ResolvedGraph, ResolvedPackage};
use mgc_types::{MgError, MgResult, PackageId};
use rayon::prelude::*;
use std::path::{Path, PathBuf};

use crate::cache::{prune_project_local_cache, SharedWebCache};
use crate::install::bin::rebuild_bin_links;
use crate::install::download::{pipeline_download_and_extract, prefetch_tarballs};
use crate::install::materialize::{
    extracted_root_for, graph_without_packages, hardlink_tree, materialize_nested_dependencies,
    materialize_strict_layout, prune_root_install_dirs, repair_dangling_symlinks,
    reset_nested_node_modules, select_root_packages, strict_vstore_package_dir,
};
pub use crate::install::script_policy::{
    lifecycle_scripts_allowed, load_trust_policies, should_run_lifecycle_scripts,
    trust_allows_script,
};
use crate::lifecycle::LifecycleRunner;
use crate::lockfile::installed_package_matches;
use crate::lockfile::{project_cache_dir, write_web_lockfile_with_state};
use crate::native;
use crate::profile::InstallProfile;

#[allow(clippy::too_many_arguments)]
pub async fn run_install(
    registry_url: &str,
    provider_auth_token: Option<&str>,
    store_override: Option<&ContentStore>,
    shared_cache: Option<SharedWebCache>,
    prefetch_handle: Option<tokio::task::JoinHandle<MgResult<u64>>>,
    graph: &ResolvedGraph,
    project_root: &Path,
    opts: InstallOptions,
) -> MgResult<InstallSummary> {
    let start = std::time::Instant::now();
    let mut profile = InstallProfile::from_env();
    let registry = native::npm_registry::NpmRegistry::new_with_token(
        registry_url,
        provider_auth_token.map(str::to_string),
    );
    let store_root = project_cache_dir(project_root);
    let layout = Layout::new(store_root);
    std::fs::create_dir_all(layout.root())?;
    std::fs::create_dir_all(layout.temp_dir())?;

    let cache = PackageCache::new(layout.cache_dir()).map_err(|e| MgError::Store(e.to_string()))?;
    let database =
        Some(Database::open(&layout.db_path()).map_err(|e| MgError::Store(e.to_string()))?);
    let default_store =
        ContentStore::new(layout.cas_dir()).map_err(|e| MgError::Store(e.to_string()))?;
    let store = store_override.unwrap_or(&default_store);
    let node_modules = project_root.join("node_modules");
    std::fs::create_dir_all(&node_modules)?;
    let mut summary = InstallSummary::default();

    if let Some(database) = database.as_ref() {
        database
            .clear_all_cas_refs(&layout.root().to_string_lossy())
            .map_err(|e| MgError::Store(e.to_string()))?;
    }

    let thread_id_hash = {
        let tid = std::thread::current().id();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&tid, &mut hasher);
        std::hash::Hasher::finish(&hasher)
    };

    let staging_root = if opts.legacy_flat {
        let root = layout.temp_dir().join(format!(
            "install-stage-{}-{}-{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            thread_id_hash
        ));
        std::fs::create_dir_all(root.join("node_modules"))?;
        Some(root)
    } else {
        None
    };
    let root_packages = select_root_packages(graph);
    let all_root_matched = !root_packages.is_empty()
        && root_packages.par_iter().all(|pkg| {
            installed_package_matches(&node_modules.join(pkg.id.name().as_str()), &pkg.id)
        });

    if all_root_matched && opts.force_install.is_empty() && !graph.packages.is_empty() {
        let all_vstore_matched = graph.packages.par_iter().all(|pkg| {
            installed_package_matches(&strict_vstore_package_dir(&node_modules, &pkg.id), &pkg.id)
        });
        if all_vstore_matched {
            summary.duration_ms = start.elapsed().as_millis() as u64;
            return Ok(summary);
        }
    }

    let root_package_versions: std::collections::HashMap<String, PackageId> = root_packages
        .iter()
        .map(|pkg| (pkg.id.name_str().to_string(), pkg.id.clone()))
        .collect();
    let package_map: std::collections::HashMap<PackageId, &ResolvedPackage> = graph
        .packages
        .iter()
        .map(|pkg| (pkg.id.clone(), pkg))
        .collect();
    let mut packages_with_scripts: Vec<PathBuf> = Vec::new();

    let already_materialized: std::collections::HashSet<PackageId> = if opts.incremental {
        root_packages
            .par_iter()
            .filter(|pkg| {
                !opts.force_install.contains(&pkg.id)
                    && installed_package_matches(
                        &node_modules.join(pkg.id.name().as_str()),
                        &pkg.id,
                    )
            })
            .map(|pkg| pkg.id.clone())
            .collect()
    } else {
        root_packages
            .par_iter()
            .filter(|pkg| {
                installed_package_matches(&node_modules.join(pkg.id.name().as_str()), &pkg.id)
            })
            .map(|pkg| pkg.id.clone())
            .collect()
    };
    let already_in_virtual_store: std::collections::HashSet<PackageId> = if opts.incremental {
        graph
            .packages
            .par_iter()
            .filter(|pkg| {
                !opts.force_install.contains(&pkg.id)
                    && installed_package_matches(
                        &strict_vstore_package_dir(&node_modules, &pkg.id),
                        &pkg.id,
                    )
            })
            .map(|pkg| pkg.id.clone())
            .collect()
    } else {
        std::collections::HashSet::new()
    };
    let shared_package_cache_for_install = shared_cache
        .as_ref()
        .and_then(|shared| shared.package_cache().ok());
    let local_has_seeded_tarballs = graph
        .packages
        .iter()
        .any(|pkg| cache.contains_tarball(&pkg.id));
    let use_shared_primary =
        !local_has_seeded_tarballs && shared_package_cache_for_install.is_some();
    let active_package_cache = if use_shared_primary {
        shared_package_cache_for_install
            .as_ref()
            .expect("shared package cache checked above")
    } else {
        &cache
    };
    let secondary_shared_cache = if use_shared_primary {
        None
    } else {
        shared_cache.as_ref()
    };

    let fetch_graph = if opts.incremental && !already_in_virtual_store.is_empty() {
        if std::env::var("MAGICORE_WEB_PROFILE_INSTALL").is_ok() {
            eprintln!(
                "[magicore:web:materialize-profile] fetch_graph={} already_in_vstore={} graph_total={}",
                graph_without_packages(graph, &already_in_virtual_store)
                    .packages
                    .len(),
                already_in_virtual_store.len(),
                graph.packages.len()
            );
        }
        graph_without_packages(graph, &already_in_virtual_store)
    } else {
        graph.clone()
    };
    if !fetch_graph.is_empty() {
        write_web_lockfile_with_state(project_root, graph, "installing").inspect_err(|_e| {
            if let Some(root) = &staging_root {
                let _ = std::fs::remove_dir_all(root);
            }
        })?;
    }
    if opts.legacy_flat {
        if let Some(handle) = prefetch_handle {
            match handle.await {
                Ok(Ok(bytes)) => {
                    summary.bytes_from_cache += bytes;
                }
                Ok(Err(e)) => {
                    if let Some(root) = &staging_root {
                        let _ = std::fs::remove_dir_all(root);
                    }
                    return Err(e);
                }
                Err(e) => {
                    if let Some(root) = &staging_root {
                        let _ = std::fs::remove_dir_all(root);
                    }
                    return Err(MgError::Other(format!("prefetch panicked: {e}")));
                }
            }
        }
    } else if let Some(handle) = prefetch_handle {
        handle.abort();
    }
    if opts.legacy_flat && !fetch_graph.is_empty() {
        summary.bytes_from_cache += prefetch_tarballs(
            &fetch_graph,
            &already_materialized,
            active_package_cache,
            secondary_shared_cache,
            &registry,
        )
        .await
        .inspect_err(|_e| {
            if let Some(root) = &staging_root {
                let _ = std::fs::remove_dir_all(root);
            }
        })?;
    }
    profile.mark("prefetch_tarballs", start);

    if opts.legacy_flat {
        let mut extracted_roots = std::collections::HashMap::new();
        profile.mark("prepare_extracted_roots", start);

        for pkg in &root_packages {
            let final_dir = node_modules.join(pkg.id.name().as_str());
            if installed_package_matches(&final_dir, &pkg.id) {
                if let Some(database) = database.as_ref() {
                    database
                        .insert_package(
                            &pkg.id,
                            if pkg.integrity.is_empty() {
                                None
                            } else {
                                Some(pkg.integrity.as_str())
                            },
                        )
                        .map_err(|e| MgError::Store(e.to_string()))?;
                }
                if !opts.incremental || !already_materialized.contains(&pkg.id) {
                    summary.added.push(pkg.id.clone());
                }
                continue;
            }

            let package_root = match extracted_root_for(
                &mut extracted_roots,
                &layout,
                store,
                shared_cache.as_ref(),
                active_package_cache,
                pkg,
            ) {
                Ok(root) => root,
                Err(err) => {
                    if let Some(staging_root) = staging_root.as_ref() {
                        if staging_root.exists() {
                            let _ = std::fs::remove_dir_all(staging_root);
                        }
                    }
                    return Err(err);
                }
            };
            let materialized_dir = staging_root
                .as_ref()
                .expect("legacy-flat installs always create staging_root")
                .join("node_modules")
                .join(pkg.id.name().as_str());
            if materialized_dir.exists() {
                std::fs::remove_dir_all(&materialized_dir)?;
            }
            if let Err(err) = hardlink_tree(package_root.as_path(), &materialized_dir) {
                if let Some(staging_root) = staging_root.as_ref() {
                    if staging_root.exists() {
                        let _ = std::fs::remove_dir_all(staging_root);
                    }
                }
                return Err(err);
            }
            if let Some(database) = database.as_ref() {
                if let Err(err) = database
                    .insert_package(
                        &pkg.id,
                        if pkg.integrity.is_empty() {
                            None
                        } else {
                            Some(pkg.integrity.as_str())
                        },
                    )
                    .map_err(|e| MgError::Store(e.to_string()))
                {
                    if let Some(staging_root) = staging_root.as_ref() {
                        if staging_root.exists() {
                            let _ = std::fs::remove_dir_all(staging_root);
                        }
                    }
                    return Err(err);
                }
            }
            if !opts.incremental || !already_materialized.contains(&pkg.id) {
                summary.added.push(pkg.id.clone());
            }
        }

        for pkg in &root_packages {
            let staged_dir = staging_root
                .as_ref()
                .expect("legacy-flat installs always create staging_root")
                .join("node_modules")
                .join(pkg.id.name().as_str());
            let final_dir = node_modules.join(pkg.id.name().as_str());
            if !staged_dir.exists() {
                continue;
            }
            if let Some(parent) = final_dir.parent() {
                std::fs::create_dir_all(parent).map_err(|err| {
                    MgError::Other(format!(
                        "failed to create parent '{}' for '{}': {}",
                        parent.display(),
                        pkg.id.name_str(),
                        err
                    ))
                })?;
            }
            if final_dir.exists() {
                std::fs::remove_dir_all(&final_dir).map_err(|err| {
                    MgError::Other(format!(
                        "failed to remove existing install dir '{}' for '{}': {}",
                        final_dir.display(),
                        pkg.id.name_str(),
                        err
                    ))
                })?;
            }
            std::fs::rename(&staged_dir, &final_dir).map_err(|err| {
                MgError::Other(format!(
                    "failed to promote staged package '{}' from '{}' to '{}': {}",
                    pkg.id.name_str(),
                    staged_dir.display(),
                    final_dir.display(),
                    err
                ))
            })?;
        }
    } else {
        for pkg in &root_packages {
            if let Some(database) = database.as_ref() {
                database
                    .insert_package(
                        &pkg.id,
                        if pkg.integrity.is_empty() {
                            None
                        } else {
                            Some(pkg.integrity.as_str())
                        },
                    )
                    .map_err(|e| MgError::Store(e.to_string()))?;
            }
            if !opts.incremental || !already_materialized.contains(&pkg.id) {
                summary.added.push(pkg.id.clone());
            }
        }
    }
    profile.mark("materialize_root_packages", start);
    let mut affected_root_bin_links: Vec<&ResolvedPackage> = Vec::new();
    if opts.legacy_flat {
        affected_root_bin_links = root_packages.to_vec();
        for pkg in &root_packages {
            let package_dir = node_modules.join(pkg.id.name().as_str());
            reset_nested_node_modules(&package_dir)?;
            packages_with_scripts.push(package_dir.clone());
            let mut visiting = std::collections::HashSet::new();
            let mut extracted_roots = std::collections::HashMap::new();
            materialize_nested_dependencies(
                &package_dir,
                pkg,
                &package_map,
                &root_package_versions,
                &layout,
                store,
                shared_cache.as_ref(),
                active_package_cache,
                &mut extracted_roots,
                &mut visiting,
                0,
                &mut packages_with_scripts,
            )?;
        }
    } else if fetch_graph.is_empty() {
        profile.mark("prepare_extracted_roots", start);
    } else {
        let pipeline_step_started_at = std::time::Instant::now();
        let (pipeline_bytes, extracted_roots, persist_handles) = pipeline_download_and_extract(
            &fetch_graph,
            &already_materialized,
            active_package_cache,
            shared_cache.as_ref(),
            Some(&registry),
            &layout,
            store,
        )
        .await?;
        summary.bytes_from_cache += pipeline_bytes;
        profile.mark_step(
            "pipeline_download_and_extract_step",
            pipeline_step_started_at,
        );
        profile.mark("prepare_extracted_roots", start);
        let fetch_ids = fetch_graph
            .packages
            .iter()
            .map(|pkg| pkg.id.clone())
            .collect::<std::collections::HashSet<_>>();
        let root_packages_to_link = if opts.incremental {
            root_packages
                .iter()
                .copied()
                .filter(|pkg| {
                    fetch_ids.contains(&pkg.id)
                        || !installed_package_matches(
                            &node_modules.join(pkg.id.name().as_str()),
                            &pkg.id,
                        )
                })
                .collect::<Vec<_>>()
        } else {
            root_packages.to_vec()
        };
        affected_root_bin_links = root_packages_to_link.clone();

        let strict_materialize_step_started_at = std::time::Instant::now();
        materialize_strict_layout(
            &node_modules,
            graph,
            &fetch_graph,
            &package_map,
            &root_packages_to_link,
            &layout,
            store,
            shared_cache.as_ref(),
            active_package_cache,
            &mut packages_with_scripts,
            &extracted_roots,
        )?;
        profile.mark_step(
            "materialize_strict_layout_step",
            strict_materialize_step_started_at,
        );
        let persist_step_started_at = std::time::Instant::now();
        for handle in persist_handles {
            handle
                .await
                .map_err(|e| MgError::Other(format!("shared cache persist task panicked: {e}")))?;
        }
        profile.mark_step("persist_shared_cache_step", persist_step_started_at);
    }
    profile.mark("materialize_dependency_graph", start);
    if let Some(shared_cache) = shared_cache.as_ref() {
        let _ = shared_cache.write_project_ref(
            project_root,
            graph
                .packages
                .iter()
                .map(|pkg| shared_cache.extracted_package_root(pkg)),
        );
    }
    prune_root_install_dirs(&node_modules, &root_package_versions)?;
    profile.mark("prune_root_install_dirs", start);
    if let Some(staging_root) = staging_root.as_ref() {
        if staging_root.exists() {
            std::fs::remove_dir_all(staging_root).map_err(|err| {
                MgError::Other(format!(
                    "failed to clean staging root '{}': {}",
                    staging_root.display(),
                    err
                ))
            })?;
        }
    }
    if !node_modules.join(".bin").exists() && affected_root_bin_links.is_empty() {
        affected_root_bin_links = root_packages.to_vec();
    }
    rebuild_bin_links(
        &node_modules,
        &root_packages,
        &affected_root_bin_links,
        !opts.legacy_flat,
    )?;
    profile.mark("rebuild_bin_links", start);

    write_web_lockfile_with_state(project_root, graph, "locked")?;
    profile.mark("write_lockfile", start);
    prune_project_local_cache(&layout);
    profile.mark("prune_project_local_cache", start);

    if !opts.ignore_scripts {
        use std::sync::Arc;
        use tokio::sync::Semaphore;
        use tokio::task::JoinSet;

        let trust_map = load_trust_policies(&layout);
        let blanket_scripts = opts.allow_scripts || lifecycle_scripts_allowed();

        let mut scripted_packages = Vec::new();
        for pkg_dir in &packages_with_scripts {
            let package_json = pkg_dir.join("package.json");
            if package_json.exists() {
                if let Ok(contents) = std::fs::read_to_string(&package_json) {
                    if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&contents) {
                        let has_scripts = manifest
                            .get("scripts")
                            .and_then(|s| s.as_object())
                            .map(|scripts| {
                                scripts.contains_key("preinstall")
                                    || scripts.contains_key("install")
                                    || scripts.contains_key("postinstall")
                            })
                            .unwrap_or(false);
                        if has_scripts {
                            let name = manifest
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string();
                            let version = manifest
                                .get("version")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default();
                            let policy = trust_map
                                .get(&format!("{name}@{version}"))
                                .or_else(|| trust_map.get(&name))
                                .map(String::as_str);
                            if !trust_allows_script(policy, blanket_scripts) {
                                if policy == Some("denied") {
                                    eprintln!(
                                        "[magicore] DENIED lifecycle scripts for {name}@{version} (mgc trust deny)"
                                    );
                                } else {
                                    eprintln!(
                                        "[magicore] skipped lifecycle scripts for {name}@{version} — not approved. Approve with: mgc trust approve {name}"
                                    );
                                }
                                continue;
                            }
                            scripted_packages.push(pkg_dir.clone());
                        }
                    }
                }
            }
        }

        let semaphore = Arc::new(Semaphore::new(8));
        let mut join_set = JoinSet::new();

        for pkg_dir in scripted_packages {
            let project_root = project_root.to_path_buf();
            let Ok(permit) = semaphore.clone().acquire_owned().await else {
                eprintln!("[magicore] warning: lifecycle semaphore closed");
                continue;
            };
            join_set.spawn(async move {
                let _permit = permit;
                LifecycleRunner::run_scripts(&pkg_dir, &project_root)
            });
        }

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("[magicore] warning: lifecycle script error: {}", e),
                Err(e) => eprintln!("[magicore] warning: lifecycle script task panicked: {}", e),
            }
        }
    }
    profile.mark("lifecycle_scripts", start);

    let project_root_str = project_root.to_string_lossy().to_string();
    if let Some(database) = database.as_ref() {
        database
            .clear_all_refs(&project_root_str)
            .map_err(|e| MgError::Store(e.to_string()))?;
        for pkg in &graph.packages {
            database
                .set_ref(&project_root_str, &pkg.id)
                .map_err(|e| MgError::Store(e.to_string()))?;
        }
    }

    if opts.repair {
        repair_dangling_symlinks(&node_modules)?;
    }

    summary.duration_ms = start.elapsed().as_millis() as u64;
    profile.flush(summary.duration_ms);
    Ok(summary)
}
