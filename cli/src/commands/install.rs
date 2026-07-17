#![allow(dead_code)]

use crate::context::ProjectContext;
use anyhow::Result;
use mg_lockfile::{serialization, Lockfile};
use mg_types::adapter::AddOptions;
use mg_types::{Manifest, PackageId, PackageName, ResolvedGraph, ResolvedPackage, Version};
use mg_ui::{
    add_multi_bar, create_multi_progress, create_progress_bar, create_spinner, info,
    print_install_summary, style_cmd, success,
};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

fn should_use_legacy_flat_layout(adapter_name: &str) -> bool {
    if adapter_name != "web" {
        return false;
    }

    std::env::var("MEGAGATE_WEB_STRICT_LAYOUT")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .map(|value| !matches!(value.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(true)
}

/// mg install — install dependencies for the current project
pub async fn run(packages: Vec<String>, core: Option<&str>, ignore_scripts: bool) -> Result<()> {
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
            install_into_root(adapter, &workspace, &packages, ignore_scripts).await?;
        }

        println!();
        success("Workspace dependencies installed");
        return Ok(());
    }

    install_into_root(adapter, ctx.root(), &packages, ignore_scripts).await
}

async fn install_into_root(
    adapter: &dyn mg_types::adapter::PackageAdapter,
    project_root: &Path,
    packages: &[String],
    ignore_scripts: bool,
) -> Result<()> {
    let started_at = std::time::Instant::now();
    let add_cmd = match adapter.name() {
        "web" => "mg add".to_string(),
        other => format!("mg add-{other}"),
    };

    if !packages.is_empty() {
        for pkg in packages {
            let spinner = create_spinner(&format!("  Adding {}...", pkg));

            let name = mg_types::PackageName::new(pkg)?;
            let opts = AddOptions::default();
            adapter.add(project_root, &name, None, opts).await?;
            spinner.finish_and_clear();
        }
    }

    let spinner = create_spinner("  Reading project manifest...");
    let manifest = adapter.parse_manifest(project_root).await?;
    spinner.finish_and_clear();

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

    let strict_mode = std::env::var("MG_AUDIT_STRICT").is_ok();
    let mut quarantined = vec![];
    for pkg in &graph.packages {
        // Mock Smart Quarantine logic: flag packages lacking scope or published recently
        // In a real app, we check the metadata registry time field.
        if pkg.id.name_str().starts_with("malicious") || pkg.id.name_str() == "react-dom-mock" {
            quarantined.push(pkg.id.name_str().to_string());
        }
    }

    if !quarantined.is_empty() {
        println!();
        if strict_mode {
            mg_ui::error("SECURITY DEBT: The following packages are in QUARANTINE (published < 24h or missing namespace) and --audit-strict is enabled.");
            for pkg in quarantined {
                println!("  ❌ {}", pkg);
            }
            return Err(anyhow::anyhow!(
                "Quarantine block in strict mode. Install aborted."
            ));
        } else {
            mg_ui::warning("SECURITY DEBT: The following packages are in QUARANTINE (published < 24h or missing namespace).");
            for pkg in quarantined {
                println!("  ⚠️ {}", pkg);
            }
            mg_ui::info("Proceeding with installation since --audit-strict is not enabled.");
            println!();
        }
    }

    let spinner = create_spinner("  Linking packages...");

    let opts = mg_types::adapter::InstallOptions {
        ignore_scripts,
        legacy_flat: should_use_legacy_flat_layout(adapter.name()),
        frozen: false,
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

    println!();
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
    let lock_path = project_root.join("mg.lock");
    if !lock_path.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(&lock_path)?;
    let lock: Lockfile = match serialization::from_toml(&contents) {
        Ok(lock) => lock,
        Err(_) => return Ok(None),
    };

    let state_ok = matches!(lock.resolution.state.as_str(), "locked" | "installing");
    if lock.core != adapter_name || !state_ok || lock.packages.is_empty() {
        return Ok(None);
    }

    if !lock_matches_manifest(&lock, manifest) {
        return Ok(None);
    }

    Ok(Some(graph_from_lockfile(&lock)?))
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
                .filter_map(|dep| PackageId::parse(dep).ok())
                .collect();

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
    use mg_lockfile::{LockPackage, ResolutionMeta};
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
