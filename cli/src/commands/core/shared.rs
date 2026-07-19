use std::path::{Path, PathBuf};

use anyhow::Result;
use colored::Colorize;
use mg_lockfile::{LockPackage, Lockfile};
use mg_types::adapter::{AddOptions, PackageAdapter};
use mg_types::{Manifest, PackageId, PackageName, ResolvedGraph, ResolvedPackage, Version};
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
        let pkg_id = adapter.add(root, &name, range.as_ref(), opts).await?;
        spinner.finish_and_clear();
        let requested_range = range
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "*".to_string());
        let resolved_version = pkg_id.version().to_string();

        if !no_save {
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
                "  {}@{} checked (--no-save, manifest unchanged)",
                pkg_id.name_str(),
                requested_range
            ));
        }
    }

    if !no_save && install {
        info("Installing added packages...");
        install_with_adapter(adapter, root, install_command_for_adapter(adapter), false).await?;
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
    package: &str,
    install: bool,
) -> Result<()> {
    let name = PackageName::new(package)?;
    info(&format!("Removing {}...", package));
    adapter.remove(root, &name).await?;
    success(&format!("Removed {}", package));
    if !install {
        info(&format!(
            "Run '{}' to update lockfile and node_modules",
            style_cmd(install_command_for_adapter(adapter))
        ));
        return Ok(());
    }
    info("Re-installing dependency graph...");
    let manifest = adapter.parse_manifest(root).await?;
    if let Some(graph) = load_pruned_locked_graph(root, adapter.name(), &manifest)? {
        info("Using mg.lock for remaining dependency graph.");
        let started_at = std::time::Instant::now();
        let spinner = create_spinner("  Linking packages...");
        let mut summary = adapter
            .install(&graph, root, mg_types::adapter::InstallOptions::default())
            .await?;
        spinner.finish_and_clear();
        summary.duration_ms = started_at.elapsed().as_millis() as u64;
        print_install_summary(
            summary.added.len(),
            summary.bytes_from_cache as usize,
            summary.duration_ms,
            "0 B",
        );
        println!();
        success("All dependencies installed");
        return Ok(());
    }
    install_with_adapter(adapter, root, install_command_for_adapter(adapter), false).await?;
    Ok(())
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
                install_with_adapter(adapter, root, install_command_for_adapter(adapter), false)
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
            install_with_adapter(adapter, root, install_command_for_adapter(adapter), false)
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
    let mut summary = adapter
        .install(&graph, root, mg_types::adapter::InstallOptions::default())
        .await?;
    spinner.finish_and_clear();
    profile_install_mark("adapter_install", started_at);
    summary.duration_ms = started_at.elapsed().as_millis() as u64;

    print_install_summary(
        summary.added.len(),
        summary.bytes_from_cache as usize,
        summary.duration_ms,
        "0 B",
    );
    println!();
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

    println!(
        "\n{} {}@{}",
        "📦",
        package.bold().cyan(),
        target.version.dimmed()
    );
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
