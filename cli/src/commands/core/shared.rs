use std::path::{Path, PathBuf};

use anyhow::Result;
use colored::Colorize;
use mg_lockfile::{LockPackage, Lockfile};
use mg_types::adapter::{AddOptions, InstallOptions, PackageAdapter};
use mg_types::{
    adapter::PreparedAdd, DependencySpec, Manifest, PackageId, PackageName, ResolvedGraph,
    ResolvedPackage, Version,
};
use mg_ui::{
    add_multi_bar, create_multi_progress, create_progress_bar, create_spinner, info,
    print_install_summary, style_cmd, success,
};

#[allow(dead_code)]
pub fn find_project_root(cwd: &Path) -> Result<Option<PathBuf>> {
    Ok(mg_config::project::ProjectConfig::find_project_root(cwd))
}

fn install_command_for_adapter(adapter: &dyn PackageAdapter) -> &'static str {
    if adapter.name() == "web" {
        return "mg install";
    }

    "mg install"
}

#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub async fn add(
    adapter: &dyn PackageAdapter,
    root: &Path,
    packages: Vec<String>,
    version: Option<String>,
    dev: bool,
    exact: bool,
    optional: bool,
    peer: bool,
    no_save: bool,
    install: bool,
    global: bool,
) -> Result<()> {
    const MAX_PACKAGES: usize = 50;
    if packages.len() > MAX_PACKAGES {
        anyhow::bail!(
            "Too many packages ({}). Maximum per add command is {}.",
            packages.len(),
            MAX_PACKAGES
        );
    }
    let total = packages.len();
    let group = if peer {
        "peerDependencies"
    } else if optional {
        "optionalDependencies"
    } else if dev {
        "devDependencies"
    } else {
        "dependencies"
    };
    mg_ui::info(&format!("Adding {} package(s) to {}...", total, group));

    let manifest_before_add = if !no_save {
        let started_at = std::time::Instant::now();
        let manifest = adapter.parse_manifest(root).await.ok();
        profile_install_mark("add_parse_manifest_before", started_at);
        manifest
    } else {
        None
    };
    let mut manifest_after_add = manifest_before_add.clone();
    let mut added_ids = Vec::new();
    let mut added_packages = Vec::new();
    let mut changed_any = false;
    for package in packages {
        let spec = mg_types::DependencySpec::parse(&package)?;
        let name = spec.name;
        let range = if let Some(v) = version.as_ref() {
            Some(mg_types::VersionRange::parse(v)?)
        } else if spec.range.is_star() {
            None
        } else {
            Some(spec.range)
        };

        let spinner = create_spinner(&format!("  Resolving {}...", package));
        let opts = AddOptions {
            dev,
            optional,
            peer,
            exact,
            no_save,
            global,
        };
        let add_started_at = std::time::Instant::now();
        let PreparedAdd {
            id: pkg_id,
            range: saved_range,
        } = adapter
            .prepare_add(root, &name, range.as_ref(), opts)
            .await?;
        profile_install_mark("adapter_prepare_add", add_started_at);
        spinner.finish_and_clear();
        let requested_range = range
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "*".to_string());
        let resolved_version = pkg_id.version().to_string();

        if !no_save {
            if let Some(manifest) = manifest_after_add.as_mut() {
                let mut saved_spec = DependencySpec::new(name.clone(), saved_range.clone());
                saved_spec.dev = dev;
                saved_spec.optional = optional;
                saved_spec.peer = peer;
                let changed = manifest.add_dep(saved_spec, dev, optional, peer);
                if changed {
                    changed_any = true;
                    added_ids.push(pkg_id.clone());
                    added_packages.push(AddedPackage {
                        id: pkg_id.clone(),
                        dev,
                        optional,
                        peer,
                    });
                    if resolved_version == "0.0.0" {
                        mg_ui::info(&format!(
                            "  {}@{} saved to {}",
                            pkg_id.name_str(),
                            requested_range,
                            group
                        ));
                    } else {
                        mg_ui::info(&format!(
                            "  {}@{} added to {}",
                            pkg_id.name_str(),
                            resolved_version,
                            group
                        ));
                    }
                    mg_ui::success(&format!("Added {}", package));
                } else {
                    mg_ui::info(&format!(
                        "  {} already present in {}, skipping",
                        pkg_id.name_str(),
                        group
                    ));
                }
            }
        } else {
            mg_ui::info(&format!(
                "  {}@{} checked (--no-save, manifest unchanged)",
                pkg_id.name_str(),
                requested_range
            ));
        }
    }

    if !no_save {
        if changed_any {
            if let Some(manifest) = manifest_after_add.as_ref() {
                let write_started_at = std::time::Instant::now();
                adapter.write_manifest(root, manifest).await?;
                profile_install_mark("add_write_manifest", write_started_at);
            }
        } else {
            info("Manifest unchanged.");
        }
    }

    if !no_save && !changed_any {
        info("Skipping install because dependencies were already present.");
        return Ok(());
    }

    if !no_save && install {
        info("Installing added packages...");
        if !try_install_added_packages_from_lock(
            adapter,
            root,
            manifest_before_add.as_ref(),
            &added_packages,
        )
        .await?
        {
            install_with_adapter(
                adapter,
                root,
                install_command_for_adapter(adapter),
                false,
                InstallOptions {
                    incremental: true,
                    force_install: added_ids,
                    ..Default::default()
                },
            )
            .await?;
        }
    } else if !no_save {
        info(&format!(
            "Run '{}' to update lockfile and node_modules",
            style_cmd(install_command_for_adapter(adapter))
        ));
    }

    Ok(())
}

#[allow(dead_code)]
pub async fn remove(
    adapter: &dyn PackageAdapter,
    root: &Path,
    packages: Vec<String>,
    install: bool,
) -> Result<()> {
    const MAX_PACKAGES: usize = 50;
    if packages.len() > MAX_PACKAGES {
        anyhow::bail!(
            "Too many packages ({}). Maximum per remove command is {}.",
            packages.len(),
            MAX_PACKAGES
        );
    }
    info(&format!("Removing {} package(s)...", packages.len()));
    let parse_started_at = std::time::Instant::now();
    let mut manifest = adapter.parse_manifest(root).await?;
    profile_install_mark("remove_parse_manifest", parse_started_at);
    let mut removed_any = false;
    for package in &packages {
        let _ = PackageName::new(package)?;
        if manifest.remove_dep(package) {
            removed_any = true;
            success(&format!("Removed {}", package));
        } else {
            info(&format!("  {} not found in manifest, skipping", package));
        }
    }
    if !removed_any {
        info("Manifest unchanged.");
        if install {
            info("Skipping reinstall because no dependencies were removed.");
        }
        return Ok(());
    }
    let write_started_at = std::time::Instant::now();
    adapter.write_manifest(root, &manifest).await?;
    profile_install_mark("remove_write_manifest", write_started_at);
    if !install {
        info(&format!(
            "Run '{}' to update lockfile and node_modules",
            style_cmd(install_command_for_adapter(adapter))
        ));
        return Ok(());
    }
    info("Re-installing dependency graph...");
    if let Some(graph) = load_pruned_locked_graph(root, adapter.name(), &manifest)? {
        info("Using mg.lock for remaining dependency graph.");
        let started_at = std::time::Instant::now();
        let spinner = create_spinner("  Linking packages...");
        let mut summary = adapter
            .install(
                &graph,
                root,
                InstallOptions {
                    incremental: true,
                    ..Default::default()
                },
            )
            .await?;
        spinner.finish_and_clear();
        summary.duration_ms = started_at.elapsed().as_millis() as u64;
        print_install_summary(
            summary.added.len(),
            summary.bytes_from_cache as usize,
            summary.duration_ms,
            "0 B",
        );
        mg_ui::blank_line();
        success("All dependencies installed");
        return Ok(());
    }
    install_with_adapter(
        adapter,
        root,
        install_command_for_adapter(adapter),
        false,
        mg_types::adapter::InstallOptions {
            incremental: true,
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

#[derive(Debug, Clone)]
struct AddedPackage {
    id: PackageId,
    dev: bool,
    optional: bool,
    peer: bool,
}

#[allow(dead_code)]
pub async fn list(adapter: &dyn PackageAdapter, root: &Path) -> Result<()> {
    let packages = adapter.list(root).await?;
    if packages.is_empty() {
        info("No packages installed");
        return Ok(());
    }
    for pkg in &packages {
        let dev = if pkg.is_dev { " (dev)" } else { "" };
        info(&format!(
            "  {}@{}{}",
            pkg.id.name_str(),
            pkg.id.version(),
            dev
        ));
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn update(
    adapter: &dyn PackageAdapter,
    root: &Path,
    packages: Vec<String>,
    install: bool,
) -> Result<()> {
    if packages.is_empty() {
        info("Checking for outdated packages...");
        let spinner = create_spinner("  Resolving latest versions...");
        let updated = adapter.update(root, None).await?;
        spinner.finish_and_clear();
        if updated.is_empty() {
            info("All packages are up to date");
        } else {
            for pkg in &updated {
                info(&format!(
                    "  {}: {} → {}",
                    pkg.name, pkg.from_version, pkg.to_version
                ));
            }
            success(&format!("Updated {} package(s)", updated.len()));
            if install {
                info("Installing updated packages...");
                install_with_adapter(
                    adapter,
                    root,
                    install_command_for_adapter(adapter),
                    false,
                    mg_types::adapter::InstallOptions {
                        incremental: true,
                        ..Default::default()
                    },
                )
                .await?;
            } else {
                info(&format!(
                    "Run '{}' to install updates",
                    style_cmd(install_command_for_adapter(adapter))
                ));
            }
        }
    } else {
        for name in &packages {
            let pn = PackageName::new(name)?;
            let spinner = create_spinner(&format!("  Updating {}...", name));
            let updated = adapter.update(root, Some(&pn)).await?;
            spinner.finish_and_clear();
            for pkg in &updated {
                info(&format!(
                    "  {}: {} → {}",
                    pkg.name, pkg.from_version, pkg.to_version
                ));
            }
        }
        success("Update complete");
        if install {
            info("Installing updated packages...");
            install_with_adapter(
                adapter,
                root,
                install_command_for_adapter(adapter),
                false,
                mg_types::adapter::InstallOptions {
                    incremental: true,
                    ..Default::default()
                },
            )
            .await?;
        } else {
            info(&format!(
                "Run '{}' to install updates",
                style_cmd(install_command_for_adapter(adapter))
            ));
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn install_with_adapter(
    adapter: &dyn PackageAdapter,
    root: &Path,
    add_cmd: &str,
    frozen: bool,
    opts: mg_types::adapter::InstallOptions,
) -> Result<()> {
    let command_started_at = std::time::Instant::now();
    let InstallExecution {
        graph,
        summary: _prepared_summary,
        used_lockfile,
    } = prepare_install_execution(adapter, root, frozen, Some(add_cmd)).await?;
    profile_install_mark("prepare_install_execution", command_started_at);
    let started_at = std::time::Instant::now();

    let resolve_bar = create_progress_bar(graph.len() as u64, "Resolving...");
    if used_lockfile {
        resolve_bar.finish_with_message(format!("✅  Loaded {} locked packages", graph.len()));
    } else {
        resolve_bar.finish_with_message(format!("✅  Resolved {} packages", graph.len()));
    }

    let multi = create_multi_progress();
    let mut bars = vec![];
    for pkg in &graph.packages {
        let pb = add_multi_bar(
            &multi,
            100,
            &format!("{}@{}", pkg.id.name_str(), pkg.id.version()),
        );
        bars.push(pb);
    }
    for (i, pb) in bars.iter().enumerate() {
        pb.set_position(100);
        pb.finish_with_message(format!("✅ {}", graph.packages[i].id.name_str()));
    }

    let spinner = create_spinner("  Linking packages...");
    let mut summary = adapter.install(&graph, root, opts).await?;
    spinner.finish_and_clear();
    profile_install_mark("adapter_install", started_at);
    summary.duration_ms = started_at.elapsed().as_millis() as u64;

    print_install_summary(
        summary.added.len(),
        summary.bytes_from_cache as usize,
        summary.duration_ms,
        "0 B",
    );
    mg_ui::blank_line();
    success("All dependencies installed");
    profile_install_mark("install_with_adapter_total", command_started_at);
    Ok(())
}

pub(crate) struct InstallExecution {
    pub graph: ResolvedGraph,
    pub summary: mg_types::adapter::InstallSummary,
    pub used_lockfile: bool,
}

pub(crate) async fn prepare_install_execution(
    adapter: &dyn PackageAdapter,
    root: &Path,
    frozen: bool,
    add_cmd: Option<&str>,
) -> Result<InstallExecution> {
    let started_at = std::time::Instant::now();
    let spinner = create_spinner("  Reading project manifest...");
    let manifest = adapter.parse_manifest(root).await?;
    spinner.finish_and_clear();
    profile_install_mark("parse_manifest", started_at);

    let all_deps: Vec<_> = manifest.all_dependencies().collect();
    if all_deps.is_empty() {
        if let Some(add_cmd) = add_cmd {
            info("No dependencies to install.");
            info(&format!(
                "Use '{} <package>' to add dependencies.",
                style_cmd(add_cmd)
            ));
        }
        return Ok(InstallExecution {
            graph: ResolvedGraph::empty(),
            summary: mg_types::adapter::InstallSummary::default(),
            used_lockfile: false,
        });
    }

    let (graph, used_lockfile) = if let Some(graph) =
        load_locked_graph(root, adapter.name(), &manifest)?
    {
        info("Using mg.lock for install state.");
        profile_install_mark("load_locked_graph", started_at);
        (graph, true)
    } else {
        if frozen {
            let cmd = install_command_for_adapter(adapter);
            anyhow::bail!(
                "--frozen: mg.lock is missing or does not match package.json.\n\
                 Run '{cmd}' to generate an up-to-date lockfile."
            );
        }
        let spinner = create_spinner(&format!("  Resolving {} dependencies...", all_deps.len()));
        let resolve_started_at = std::time::Instant::now();
        let graph = adapter.resolve(&manifest).await?;
        spinner.finish_and_clear();
        profile_install_mark("resolve_graph", resolve_started_at);
        (graph, false)
    };
    profile_install_mark("prepare_install_execution_total", started_at);
    Ok(InstallExecution {
        graph,
        summary: mg_types::adapter::InstallSummary {
            duration_ms: started_at.elapsed().as_millis() as u64,
            ..Default::default()
        },
        used_lockfile,
    })
}

async fn try_install_added_packages_from_lock(
    adapter: &dyn PackageAdapter,
    root: &Path,
    manifest_before_add: Option<&Manifest>,
    added_packages: &[AddedPackage],
) -> Result<bool> {
    if added_packages.is_empty() {
        return Ok(false);
    }
    let Some(previous_manifest) = manifest_before_add else {
        return Ok(false);
    };
    let Some(locked_graph) = load_locked_graph(root, adapter.name(), previous_manifest)? else {
        return Ok(false);
    };

    let delta_manifest = build_delta_manifest(previous_manifest, added_packages)?;

    let resolve_started_at = std::time::Instant::now();
    let spinner = create_spinner(&format!(
        "  Resolving {} new package(s)...",
        added_packages.len()
    ));
    let delta_graph = adapter.resolve(&delta_manifest).await?;
    spinner.finish_and_clear();
    profile_install_mark("resolve_delta_graph", resolve_started_at);

    let graph = merge_graphs(locked_graph, delta_graph);
    let added_ids = added_packages
        .iter()
        .map(|pkg| pkg.id.clone())
        .collect::<Vec<_>>();
    let started_at = std::time::Instant::now();
    let spinner = create_spinner("  Linking changed packages...");
    let mut summary = adapter
        .install(
            &graph,
            root,
            InstallOptions {
                incremental: true,
                force_install: added_ids.to_vec(),
                ..Default::default()
            },
        )
        .await?;
    spinner.finish_and_clear();
    summary.duration_ms = started_at.elapsed().as_millis() as u64;
    print_install_summary(
        summary.added.len(),
        summary.bytes_from_cache as usize,
        summary.duration_ms,
        "0 B",
    );
    mg_ui::blank_line();
    success("All dependencies installed");
    profile_install_mark("install_delta_with_lock_total", started_at);
    Ok(true)
}

fn build_delta_manifest(manifest: &Manifest, added_packages: &[AddedPackage]) -> Result<Manifest> {
    let mut delta = Manifest::new(&manifest.name, manifest.ecosystem.clone());
    for package in added_packages {
        let mut spec = DependencySpec::new(
            package.id.name().clone(),
            mg_types::VersionRange::parse(&format!("={}", package.id.version()))?,
        );
        spec.dev = package.dev;
        spec.optional = package.optional;
        spec.peer = package.peer;
        delta.add_dep(spec, package.dev, package.optional, package.peer);
    }
    Ok(delta)
}

fn merge_graphs(mut base: ResolvedGraph, delta: ResolvedGraph) -> ResolvedGraph {
    let mut positions = std::collections::HashMap::new();
    for (idx, pkg) in base.packages.iter().enumerate() {
        positions.insert(pkg.id.clone(), idx);
    }
    for pkg in delta.packages {
        if let Some(idx) = positions.get(&pkg.id).copied() {
            let existing = &mut base.packages[idx];
            existing.direct |= pkg.direct;
            existing.dev |= pkg.dev;
            if existing.integrity.is_empty() && !pkg.integrity.is_empty() {
                existing.integrity = pkg.integrity;
            }
            if existing.tarball_url.is_empty() && !pkg.tarball_url.is_empty() {
                existing.tarball_url = pkg.tarball_url;
            }
            for dep in pkg.deps {
                if !existing.deps.contains(&dep) {
                    existing.deps.push(dep);
                }
            }
        } else {
            positions.insert(pkg.id.clone(), base.packages.len());
            base.packages.push(pkg);
        }
    }
    base
}

fn profile_install_mark(label: &str, started_at: std::time::Instant) {
    let enabled = std::env::var("MEGAGATE_WEB_PROFILE_INSTALL")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false);
    if enabled {
        eprintln!(
            "[megagate:web:command-profile] {}={}ms",
            label,
            started_at.elapsed().as_millis()
        );
    }
}

#[allow(dead_code)]
fn load_locked_graph(
    project_root: &Path,
    adapter_name: &str,
    manifest: &Manifest,
) -> Result<Option<ResolvedGraph>> {
    let Some(lock) = read_checked_lockfile(project_root)? else {
        return Ok(None);
    };
    let state_ok = matches!(lock.resolution.state.as_str(), "locked" | "installing");
    if lock.core != adapter_name || !state_ok || lock.packages.is_empty() {
        return Ok(None);
    }
    if lock.version != 1 {
        return Ok(None);
    }
    if lock.packages.iter().any(|p| p.name.is_empty()) {
        return Ok(None);
    }
    if !lock_matches_manifest(&lock, manifest) {
        return Ok(None);
    }
    Ok(Some(graph_from_lockfile(&lock)?))
}

fn read_checked_lockfile(project_root: &Path) -> Result<Option<Lockfile>> {
    mg_lockfile::read_lockfile_checked(project_root)
}

#[allow(dead_code)]
fn lock_matches_manifest(lock: &Lockfile, manifest: &Manifest) -> bool {
    let direct_manifest: Vec<_> = manifest.all_dependencies().collect();
    let direct_locked: Vec<_> = lock.packages.iter().filter(|pkg| pkg.direct).collect();
    if direct_manifest.len() != direct_locked.len() {
        return false;
    }
    direct_manifest.iter().all(|dep| {
        direct_locked
            .iter()
            .find(|pkg| pkg.name == dep.name.as_str())
            .and_then(|pkg| Version::parse(&pkg.version).ok())
            .is_some_and(|version| dep.range.matches(&version))
    })
}

fn load_pruned_locked_graph(
    project_root: &Path,
    adapter_name: &str,
    manifest: &Manifest,
) -> Result<Option<ResolvedGraph>> {
    let Some(lock) = read_checked_lockfile(project_root)? else {
        return Ok(None);
    };
    let state_ok = matches!(lock.resolution.state.as_str(), "locked" | "installing");
    if lock.core != adapter_name || !state_ok || lock.version != 1 || lock.packages.is_empty() {
        return Ok(None);
    }

    let direct_manifest: Vec<_> = manifest.all_dependencies().collect();
    let mut direct_ids = Vec::with_capacity(direct_manifest.len());
    for dep in direct_manifest {
        let Some(pkg) = lock
            .packages
            .iter()
            .find(|pkg| pkg.name == dep.name.as_str())
        else {
            return Ok(None);
        };
        let version = Version::parse(&pkg.version)?;
        if !dep.range.matches(&version) {
            return Ok(None);
        }
        direct_ids.push(format!("{}@{}", pkg.name, pkg.version));
    }

    let packages_by_id: std::collections::HashMap<String, &LockPackage> = lock
        .packages
        .iter()
        .map(|pkg| (format!("{}@{}", pkg.name, pkg.version), pkg))
        .collect();
    let mut reachable = std::collections::BTreeSet::new();
    let mut stack = direct_ids;
    while let Some(id) = stack.pop() {
        if !reachable.insert(id.clone()) {
            continue;
        }
        let Some(pkg) = packages_by_id.get(&id) else {
            return Ok(None);
        };
        for dep in &pkg.dependencies {
            stack.push(dep.clone());
        }
    }

    let mut pruned = lock;
    pruned
        .packages
        .retain(|pkg| reachable.contains(&format!("{}@{}", pkg.name, pkg.version)));
    pruned.resolution.package_count = pruned.packages.len();
    Ok(Some(graph_from_lockfile(&pruned)?))
}

#[allow(dead_code)]
fn graph_from_lockfile(lock: &Lockfile) -> Result<ResolvedGraph> {
    let packages = lock
        .packages
        .iter()
        .map(|pkg| {
            let name = PackageName::new(pkg.name.clone())?;
            let version = Version::parse(&pkg.version)?;
            let deps = pkg
                .dependencies
                .iter()
                .map(|dep| {
                    PackageId::parse(dep).map_err(|err| {
                        anyhow::anyhow!(
                            "invalid dependency id '{}' in lockfile package '{}@{}': {}",
                            dep,
                            pkg.name,
                            pkg.version,
                            err
                        )
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(ResolvedPackage {
                id: PackageId::new(name, version),
                integrity: pkg.integrity.clone().unwrap_or_default(),
                tarball_url: String::new(),
                deps,
                direct: pkg.direct,
                dev: pkg.dev,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ResolvedGraph { packages })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mg_lockfile::{serialization, LockPackage, ResolutionMeta};
    use mg_types::{DependencySpec, Ecosystem, VersionRange};

    #[test]
    fn test_lock_matches_manifest_when_versions_satisfy_ranges() {
        let mut manifest = Manifest::new("demo", Ecosystem::Web);
        manifest.add_dep(
            DependencySpec::new(
                PackageName::new("tailwindcss").unwrap(),
                VersionRange::parse("^4.3.0").unwrap(),
            ),
            false,
            false,
            false,
        );
        let mut lock = Lockfile::new("web", "frontend");
        lock.resolution = ResolutionMeta {
            state: "locked".into(),
            store: "megagate".into(),
            package_count: 1,
        };
        lock.packages.push(LockPackage {
            name: "tailwindcss".into(),
            version: "4.3.2".into(),
            integrity: None,
            direct: true,
            dev: false,
            dependencies: vec![],
        });
        assert!(lock_matches_manifest(&lock, &manifest));
    }

    #[test]
    fn test_lock_matches_manifest_rejects_stale_version() {
        let mut manifest = Manifest::new("demo", Ecosystem::Web);
        manifest.add_dep(
            DependencySpec::new(
                PackageName::new("tailwindcss").unwrap(),
                VersionRange::parse("^5.0.0").unwrap(),
            ),
            false,
            false,
            false,
        );
        let mut lock = Lockfile::new("web", "frontend");
        lock.resolution = ResolutionMeta {
            state: "locked".into(),
            store: "megagate".into(),
            package_count: 1,
        };
        lock.packages.push(LockPackage {
            name: "tailwindcss".into(),
            version: "4.3.2".into(),
            integrity: None,
            direct: true,
            dev: false,
            dependencies: vec![],
        });
        assert!(!lock_matches_manifest(&lock, &manifest));
    }

    #[test]
    fn test_read_checked_lockfile_errors_on_checksum_mismatch() {
        let root = tempfile::tempdir().unwrap();
        let lock = Lockfile::new("web", "frontend");
        std::fs::write(
            root.path().join("mg.lock"),
            serialization::to_toml(&lock).unwrap(),
        )
        .unwrap();
        std::fs::write(root.path().join("mg.lock.sha256"), "bad").unwrap();

        let err = read_checked_lockfile(root.path()).unwrap_err();

        assert!(
            err.to_string().contains("lockfile checksum mismatch"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_graph_from_lockfile_rejects_invalid_dependency_id() {
        let mut lock = Lockfile::new("web", "frontend");
        lock.packages.push(LockPackage {
            name: "react".into(),
            version: "18.2.0".into(),
            integrity: None,
            direct: true,
            dev: false,
            dependencies: vec!["not-a-package-id".into()],
        });

        let err = graph_from_lockfile(&lock).unwrap_err();

        assert!(
            err.to_string().contains("invalid dependency id"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_load_pruned_locked_graph_keeps_only_reachable_packages() {
        let root = tempfile::tempdir().unwrap();
        let mut manifest = Manifest::new("demo", Ecosystem::Web);
        manifest.add_dep(
            DependencySpec::new(
                PackageName::new("react").unwrap(),
                VersionRange::parse("^18.0.0").unwrap(),
            ),
            false,
            false,
            false,
        );

        let mut lock = Lockfile::new("web", "frontend");
        lock.resolution = ResolutionMeta {
            state: "locked".into(),
            store: "megagate".into(),
            package_count: 4,
        };
        lock.packages.push(LockPackage {
            name: "react".into(),
            version: "18.3.1".into(),
            integrity: None,
            direct: true,
            dev: false,
            dependencies: vec!["loose-envify@1.4.0".into()],
        });
        lock.packages.push(LockPackage {
            name: "loose-envify".into(),
            version: "1.4.0".into(),
            integrity: None,
            direct: false,
            dev: false,
            dependencies: vec![],
        });
        lock.packages.push(LockPackage {
            name: "zod".into(),
            version: "4.4.3".into(),
            integrity: None,
            direct: true,
            dev: false,
            dependencies: vec![],
        });
        lock.packages.push(LockPackage {
            name: "orphan".into(),
            version: "1.0.0".into(),
            integrity: None,
            direct: false,
            dev: false,
            dependencies: vec![],
        });
        std::fs::write(
            root.path().join("mg.lock"),
            serialization::to_toml(&lock).unwrap(),
        )
        .unwrap();

        let graph = load_pruned_locked_graph(root.path(), "web", &manifest)
            .unwrap()
            .unwrap();
        let names = graph
            .packages
            .iter()
            .map(|pkg| pkg.id.name_str().to_string())
            .collect::<std::collections::BTreeSet<_>>();

        assert!(names.contains("react"));
        assert!(names.contains("loose-envify"));
        assert!(!names.contains("zod"));
        assert!(!names.contains("orphan"));
    }

    #[test]
    fn test_build_delta_manifest_keeps_dependency_group() {
        let manifest = Manifest::new("demo", Ecosystem::Web);
        let added = vec![AddedPackage {
            id: PackageId::parse("vitest@3.2.1").unwrap(),
            dev: true,
            optional: false,
            peer: false,
        }];

        let delta = build_delta_manifest(&manifest, &added).unwrap();

        assert!(delta.dependencies.is_empty());
        assert_eq!(delta.dev_dependencies.len(), 1);
        assert_eq!(delta.dev_dependencies[0].name.as_str(), "vitest");
        assert_eq!(delta.dev_dependencies[0].range.as_str(), "=3.2.1");
    }

    #[test]
    fn test_merge_graphs_promotes_existing_transitive_to_direct() {
        let dep_id = PackageId::parse("zod@4.4.3").unwrap();
        let base = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: dep_id.clone(),
                integrity: String::new(),
                tarball_url: String::new(),
                deps: vec![],
                direct: false,
                dev: false,
            }],
        };
        let delta = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: dep_id,
                integrity: "sha512-test".into(),
                tarball_url: "https://registry.example/zod.tgz".into(),
                deps: vec![],
                direct: true,
                dev: true,
            }],
        };

        let merged = merge_graphs(base, delta);

        assert_eq!(merged.packages.len(), 1);
        assert!(merged.packages[0].direct);
        assert!(merged.packages[0].dev);
        assert_eq!(merged.packages[0].integrity, "sha512-test");
    }
}

fn link_name(package: &str) -> &str {
    if package.contains('/') {
        package.rsplit('/').next().unwrap_or(package)
    } else {
        package
    }
}

pub async fn link(adapter: &dyn PackageAdapter, root: &Path, package: Option<&str>) -> Result<()> {
    if adapter.name() != "web" {
        anyhow::bail!("link only supported for web core");
    }
    let pkg = package.ok_or_else(|| anyhow::anyhow!("Usage: mg link <package>"))?;
    info(&format!("Linking {}...", pkg));

    let node_modules = root.join("node_modules");
    let name = link_name(pkg);
    let link_path = node_modules.join(name);

    if link_path.exists() {
        anyhow::bail!("{} already exists in node_modules", name);
    }

    std::fs::create_dir_all(&node_modules)?;
    let source = find_package_source(root, pkg)?;
    std::os::unix::fs::symlink(&source, &link_path)?;

    success(&format!("Linked {} -> {}", name, source.display()));
    Ok(())
}

pub async fn unlink(
    adapter: &dyn PackageAdapter,
    root: &Path,
    package: Option<&str>,
) -> Result<()> {
    if adapter.name() != "web" {
        anyhow::bail!("unlink only supported for web core");
    }
    let pkg = package.ok_or_else(|| anyhow::anyhow!("Usage: mg unlink <package>"))?;
    info(&format!("Unlinking {}...", pkg));

    let name = link_name(pkg);
    let link_path = root.join("node_modules").join(name);
    if !link_path.exists() {
        anyhow::bail!("{} is not linked in node_modules", name);
    }

    let meta = std::fs::symlink_metadata(&link_path)?;
    if meta.file_type().is_symlink() || meta.is_file() {
        std::fs::remove_file(&link_path)?;
    } else {
        std::fs::remove_dir_all(&link_path)?;
    }

    success(&format!("Unlinked {}", pkg));
    Ok(())
}

pub async fn why(adapter: &dyn PackageAdapter, root: &Path, package: &str) -> Result<()> {
    if adapter.name() != "web" {
        anyhow::bail!("why only supported for web core");
    }

    let lock_path = root.join("mg.lock");
    if !lock_path.exists() {
        anyhow::bail!("mg.lock not found — run 'mg install' first");
    }

    let content = std::fs::read_to_string(&lock_path)?;
    let lock: mg_lockfile::Lockfile = mg_lockfile::serialization::from_toml(&content)?;

    let target = lock.packages.iter().find(|p| p.name == package);
    let target = match target {
        Some(p) => p,
        None => {
            info(&format!("Package '{}' not found in mg.lock", package));
            return Ok(());
        }
    };

    mg_ui::blank_line();
    println!("{} {}@{}", "📦", package.bold().cyan(), target.version.dimmed());
    if target.direct {
        println!("  {} Direct dependency", "├─".green());
    }
    if target.dev {
        println!("  {} Dev dependency", "├─".green());
    }

    let rdeps: Vec<&mg_lockfile::LockPackage> = lock
        .packages
        .iter()
        .filter(|p| p.dependencies.iter().any(|d| d.starts_with(package)))
        .collect();

    if rdeps.is_empty() {
        println!("  {} No reverse dependencies", "└─".yellow());
    } else {
        println!("  {} Required by:", "├─".green());
        for dep in &rdeps {
            println!("  │   {} {}@{}", "◉".blue(), dep.name, dep.version);
        }
    }

    if !target.dependencies.is_empty() {
        println!("  {} Depends on:", "└─".green());
        for dep in &target.dependencies {
            println!("      {} {}", "◉".blue(), dep);
        }
    }

    Ok(())
}

fn find_package_source(root: &Path, package: &str) -> Result<PathBuf> {
    // If it's a local path, use it directly
    if package.starts_with('.') || package.starts_with('/') {
        let path = if package.starts_with('/') {
            PathBuf::from(package)
        } else {
            root.join(package)
        };
        if path.exists() {
            return Ok(path.canonicalize()?);
        }
        anyhow::bail!("local package path not found: {}", path.display());
    }

    // Check local node_modules
    let local = root.join("node_modules").join(package);
    if local.exists() {
        return Ok(local);
    }

    for global in megagate_global_package_roots() {
        let global_pkg = global.join(package);
        if global_pkg.exists() {
            return Ok(global_pkg);
        }
    }

    anyhow::bail!(
        "package '{}' not found in local node_modules or MegaGate global package roots.\n\
         Use 'mg install' to install it first, provide a local path, or configure MEGAGATE_GLOBAL_PACKAGE_ROOT.",
        package
    )
}

fn megagate_global_package_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(path) = std::env::var_os("MEGAGATE_GLOBAL_PACKAGE_ROOT") {
        roots.push(PathBuf::from(path));
    }
    if let Some(cache_dir) = dirs::cache_dir() {
        roots.push(
            cache_dir
                .join("megagate")
                .join("global")
                .join("web")
                .join("node_modules"),
        );
    }
    roots
}
