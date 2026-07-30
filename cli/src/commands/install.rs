#![allow(dead_code)]

use crate::context::ProjectContext;
use anyhow::Result;
use mg_lockfile::Lockfile;
use mg_types::adapter::{AddOptions, PreparedAdd};
use mg_types::{
    DependencySpec, Manifest, PackageId, PackageName, ResolvedGraph, ResolvedPackage, Version,
};
use mg_ui::{
    add_multi_bar, create_multi_progress, create_progress_bar, create_spinner, info,
    print_install_summary, style_cmd, success,
};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};



/// mg install — install dependencies for the current project
pub async fn run(
    packages: Vec<String>,
    core: Option<&str>,
    ignore_scripts: bool,
    allow_scripts: bool,
) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();

    if let Some(workspaces) = discover_workspace_projects(ctx.root())? {
        if workspaces.is_empty() {
            info("No installable workspaces found in this monorepo.");
            return Ok(());
        }

        if !packages.is_empty() {
            info("Applying requested packages to each detected workspace.");
        }

        for workspace in workspaces {
            info(&format!("Installing workspace: {}", workspace.display()));
            install_into_root(
                adapter,
                &workspace,
                &packages,
                ignore_scripts,
                allow_scripts,
            )
            .await?;
        }

        mg_ui::blank_line();
        success("Workspace dependencies installed");
        return Ok(());
    }

    install_into_root(
        adapter,
        ctx.root(),
        &packages,
        ignore_scripts,
        allow_scripts,
    )
    .await
}

async fn install_into_root(
    adapter: &dyn mg_types::adapter::PackageAdapter,
    project_root: &Path,
    packages: &[String],
    ignore_scripts: bool,
    allow_scripts: bool,
) -> Result<()> {
    const MAX_PACKAGES: usize = 50;
    if packages.len() > MAX_PACKAGES {
        anyhow::bail!(
            "Too many packages ({}). Maximum per install command is {}.",
            packages.len(),
            MAX_PACKAGES
        );
    }

    let started_at = std::time::Instant::now();
    let add_cmd = match adapter.name() {
        "web" => "mg add".to_string(),
        other => format!("mg add-{other}"),
    };

    let spinner = create_spinner("  Reading project manifest...");
    let mut manifest = adapter.parse_manifest(project_root).await?;
    spinner.finish_and_clear();

    if !packages.is_empty() {
        for package in packages {
            let spec = DependencySpec::parse(package)?;
            let name = spec.name;
            let range = if spec.range.is_star() {
                None
            } else {
                Some(spec.range)
            };

            let spinner = create_spinner(&format!("  Adding {}...", package));
            let PreparedAdd {
                id: _pkg_id,
                range: saved_range,
            } = adapter
                .prepare_add(project_root, &name, range.as_ref(), AddOptions::default())
                .await?;
            spinner.finish_and_clear();

            let saved_spec = DependencySpec::new(name, saved_range);
            manifest.add_dep(saved_spec, false, false, false);
        }

        adapter.write_manifest(project_root, &manifest).await?;
    }

    let all_deps: Vec<_> = manifest.all_dependencies().collect();
    if all_deps.is_empty() {
        info("No dependencies to install.");
        info(&format!(
            "Use '{} <package>' to add dependencies.",
            style_cmd(&add_cmd)
        ));
        return Ok(());
    }

    let (graph, used_lockfile) = if let Some(graph) =
        load_locked_graph(project_root, adapter.name(), &manifest)?
    {
        info("Using mg.lock for install state.");
        (graph, true)
    } else {
        let spinner = create_spinner(&format!("  Resolving {} dependencies...", all_deps.len()));
        let graph = adapter.resolve(&manifest).await?;
        spinner.finish_and_clear();
        (graph, false)
    };

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

    let opts = mg_types::adapter::InstallOptions {
        ignore_scripts,
        allow_scripts,
        legacy_flat: crate::commands::core::shared::should_use_legacy_flat_layout(adapter.name()),
        frozen: false,
        ..Default::default()
    };
    let mut summary = adapter.install(&graph, project_root, opts).await?;
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
    Ok(())
}

#[derive(Debug, Deserialize)]
struct WorkspaceConfig {
    mode: Option<String>,
    layout: Option<WorkspaceLayout>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceLayout {
    apps_dir: Option<String>,
    packages_dir: Option<String>,
}

fn discover_workspace_projects(project_root: &Path) -> Result<Option<Vec<PathBuf>>> {
    let workspace_path = project_root.join("megagate.workspace.toml");
    if !workspace_path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&workspace_path)?;
    let config: WorkspaceConfig = toml::from_str(&contents)?;
    if config.mode.as_deref() != Some("monorepo") {
        return Ok(None);
    }

    let apps_dir = config
        .layout
        .as_ref()
        .and_then(|layout| layout.apps_dir.as_deref())
        .unwrap_or("apps");
    let packages_dir = config
        .layout
        .as_ref()
        .and_then(|layout| layout.packages_dir.as_deref())
        .unwrap_or("packages");

    let mut workspaces = vec![];
    collect_installable_projects(project_root.join(apps_dir), &mut workspaces)?;
    collect_installable_projects(project_root.join(packages_dir), &mut workspaces)?;

    workspaces.sort();
    workspaces.dedup();
    Ok(Some(workspaces))
}

fn collect_installable_projects(root: PathBuf, out: &mut Vec<PathBuf>) -> Result<()> {
    if !root.exists() || !root.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if path.join("package.json").exists() {
            out.push(path);
            continue;
        }

        collect_installable_projects(path, out)?;
    }

    Ok(())
}

fn load_locked_graph(
    project_root: &std::path::Path,
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

    if lock.packages.iter().any(|pkg| pkg.name.is_empty()) {
        return Ok(None);
    }

    if !lock_matches_manifest(&lock, manifest) {
        return Ok(None);
    }

    Ok(Some(graph_from_lockfile(&lock)?))
}

fn read_checked_lockfile(project_root: &std::path::Path) -> Result<Option<Lockfile>> {
    mg_lockfile::read_lockfile_checked(project_root)
}

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
                peer_deps: vec![],
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
    use tempfile::tempdir;

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
    fn test_load_locked_graph_rejects_unsupported_lock_version() {
        let dir = tempdir().unwrap();
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
        lock.version = 0;
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
        std::fs::write(
            dir.path().join("mg.lock"),
            serialization::to_toml(&lock).unwrap(),
        )
        .unwrap();

        assert!(load_locked_graph(dir.path(), "web", &manifest)
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_load_locked_graph_errors_on_checksum_mismatch() {
        let dir = tempdir().unwrap();
        let manifest = Manifest::new("demo", Ecosystem::Web);
        let lock = Lockfile::new("web", "frontend");
        std::fs::write(
            dir.path().join("mg.lock"),
            serialization::to_toml(&lock).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.path().join("mg.lock.sha256"), "bad").unwrap();

        let err = load_locked_graph(dir.path(), "web", &manifest).unwrap_err();

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
    fn test_discover_workspace_projects_for_monorepo_root() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("megagate.workspace.toml"),
            r#"
version = 1
mode = "monorepo"

[layout]
apps_dir = "apps"
packages_dir = "packages"
"#,
        )
        .unwrap();

        let frontend = dir.path().join("apps").join("frontend");
        let backend = dir.path().join("apps").join("backend");
        let contracts = dir.path().join("packages").join("contracts");
        fs::create_dir_all(&frontend).unwrap();
        fs::create_dir_all(&backend).unwrap();
        fs::create_dir_all(&contracts).unwrap();
        fs::write(frontend.join("package.json"), "{}").unwrap();
        fs::write(contracts.join("package.json"), "{}").unwrap();

        let workspaces = discover_workspace_projects(dir.path())
            .unwrap()
            .expect("should detect monorepo");

        assert_eq!(workspaces, vec![frontend, contracts]);
        assert!(!workspaces.contains(&backend));
    }

    #[test]
    fn test_discover_workspace_projects_ignores_non_monorepo_file() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("megagate.workspace.toml"),
            r#"
version = 1
mode = "single"
"#,
        )
        .unwrap();

        assert!(discover_workspace_projects(dir.path()).unwrap().is_none());
    }
}
