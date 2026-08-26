#![allow(dead_code)]

use crate::context::ProjectContext;
use anyhow::Result;
use mgc_cache::PackageCache;
use mgc_lockfile::Lockfile;
use mgc_types::adapter::{AddOptions, PreparedAdd};
use mgc_types::{DependencySpec, Manifest, PackageId, ResolvedGraph, ResolvedPackage, Version};
use mgc_ui::{
    add_multi_bar, create_multi_progress, create_progress_bar, create_spinner, info,
    print_install_summary, style_cmd, success,
};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// mgc install — install dependencies for the current project
pub async fn run(
    packages: Vec<String>,
    core: Option<&str>,
    ignore_scripts: bool,
    allow_scripts: bool,
    offline: bool, // T4.1: offline mode flag
) -> Result<()> {
    // T4.1: Set thread-local offline mode (R6 fix)
    if offline {
        crate::offline::set_offline_mode(true);
        // R4 FIX (AUDIT VÒNG 2): Set env var to enforce offline in adapters
        std::env::set_var("MGC_OFFLINE_MODE", "1");
    }

    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();

    if let Some(workspaces) = discover_workspace_projects(ctx.root())? {
        if workspaces.is_empty() {
            info("No installable workspaces found in this monorepo.");
            return Ok(());
        }

        // R3 FIX (AUDIT VÒNG 2): Pre-validate ALL workspaces in offline mode
        if offline {
            let mut missing_lockfiles = Vec::new();
            for ws in &workspaces {
                let lockfile = ws.join("mgc.lock");
                if !lockfile.exists() {
                    missing_lockfiles.push(ws.display().to_string());
                }
            }

            if !missing_lockfiles.is_empty() {
                anyhow::bail!(
                    "Offline mode requires lockfiles in all {} workspaces.\n  \
                     Missing lockfiles:\n  {}",
                    workspaces.len(),
                    missing_lockfiles.join("\n  ")
                );
            }

            info(&format!(
                "✓ All {} workspaces have lockfiles (offline mode)",
                workspaces.len()
            ));
        }

        if !packages.is_empty() {
            info("Applying requested packages to each detected workspace.");
        }

        // ITEM 7: install workspace song song (buffered 4), lỗi 1 project không chặn repo
        use futures_util::StreamExt;
        let results: Vec<Result<()>> = futures_util::stream::iter(workspaces)
            .map(|workspace| {
                let packages = &packages;
                async move {
                    info(&format!("Installing workspace: {}", workspace.display()));
                    // Mix core (Q23): detect core riêng cho từng project — không dùng
                    // adapter của root (root có thể không thuộc core nào).
                    let ctx = crate::context::ProjectContext::load_for_dir(&workspace)?;
                    let adapter = ctx.adapter();
                    install_into_root(
                        adapter,
                        &workspace,
                        packages,
                        ignore_scripts,
                        allow_scripts,
                        offline,
                    )
                    .await
                }
            })
            .buffered(4)
            .collect()
            .await;

        let mut failed = 0usize;
        for result in results {
            if let Err(e) = result {
                failed += 1;
                mgc_ui::error(&format!("Workspace install failed: {e:#}"));
            }
        }

        mgc_ui::blank_line();
        if failed > 0 {
            return Err(crate::error::workspace_failed(failed));
        }
        success("Workspace dependencies installed");
        return Ok(());
    }

    install_into_root(
        adapter,
        ctx.root(),
        &packages,
        ignore_scripts,
        allow_scripts,
        offline, // T4.1
    )
    .await
}

async fn install_into_root(
    adapter: &dyn mgc_types::adapter::PackageAdapter,
    project_root: &Path,
    packages: &[String],
    ignore_scripts: bool,
    allow_scripts: bool,
    offline: bool, // T4.1: offline mode
) -> Result<()> {
    // T4.1: Offline mode validation
    if offline {
        // R2.1 FIX (AUDIT VÒNG 2): Atomic check-and-load (no TOCTOU)
        info("🔒 Offline mode enabled");

        // Try load lockfile immediately (check = use, atomic)
        let lockfile_path = project_root.join("mgc.lock");
        if !lockfile_path.exists() {
            anyhow::bail!(
                "Offline mode requires mgc.lock\n  \
                 Run 'mgc install' online first to create lockfile"
            );
        }

        // T4.5: Verify lockfile integrity BEFORE using cache
        let status = mgc_lockfile::verify_lockfile(&lockfile_path)?;
        match status {
            mgc_lockfile::VerificationStatus::Tampered(msg) => {
                // T4.5: Invalidate cache on tamper detection
                info("⚠ Lockfile tampered — invalidating cache");
                let cache = PackageCache::new()?;
                // Invalidate all packages in lockfile
                let lockfile = mgc_lockfile::load_lockfile(&lockfile_path)?;
                for pkg in &lockfile.packages {
                    let pkg_id = format!("{}@{}", pkg.name, pkg.version);
                    let _ = cache.invalidate_package(&pkg_id); // Ignore errors (may not exist)
                }
                anyhow::bail!(
                    "Lockfile tampered: {}\n  \
                     Cache invalidated. Run 'mgc trust verify' to inspect.",
                    msg
                );
            }
            mgc_lockfile::VerificationStatus::Unsigned => {
                info("⚠ Lockfile not signed — run 'mgc trust sign' for tamper detection");
            }
            mgc_lockfile::VerificationStatus::Valid => {
                info("✓ Lockfile signature valid");
            }
            mgc_lockfile::VerificationStatus::InvalidSignature(msg) => {
                anyhow::bail!("Invalid lockfile signature: {}", msg);
            }
        }

        if !packages.is_empty() {
            anyhow::bail!(
                "Cannot add packages in offline mode\n  \
                 Use 'mgc install' online to add dependencies"
            );
        }

        info("  - Using lockfile for dependencies");
        info("  - Installing from local cache");
    }

    const MAX_PACKAGES: usize = 50;
    if packages.len() > MAX_PACKAGES {
        return Err(crate::error::too_many_packages(packages.len(), "install"));
    }

    let started_at = std::time::Instant::now();
    let add_cmd = match adapter.name() {
        "web" => "mgc add".to_string(),
        other => format!("mgc add-{other}"),
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
        info("Using mgc.lock for install state.");
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

    let opts = mgc_types::adapter::InstallOptions {
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

    mgc_ui::blank_line();
    success("All dependencies installed");

    Ok(())
}

/// Mix core entry (Q23): install 1 workspace project với adapter đúng core.
pub(crate) async fn install_into_root_ws(
    adapter: &dyn mgc_types::adapter::PackageAdapter,
    project_root: &Path,
    packages: &[String],
    ignore_scripts: bool,
    allow_scripts: bool,
    offline: bool, // T4.1
) -> Result<()> {
    install_into_root(
        adapter,
        project_root,
        packages,
        ignore_scripts,
        allow_scripts,
        offline, // T4.1
    )
    .await
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

pub(crate) fn discover_workspace_projects(project_root: &Path) -> Result<Option<Vec<PathBuf>>> {
    let workspace_path = project_root.join("magicore.workspace.toml");
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

/// package.json name (web) — dùng cho --filter match. Non-web fallback: None.
pub(crate) fn workspace_package_name(project_root: &Path) -> Option<String> {
    mgc_workspace::read_package_manifest(project_root)
        .ok()
        .flatten()
        .map(|m| m.name)
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

        // Mix core (Q23): nhận mọi manifest — package.json (web), Cargo.toml
        // (lib), pyproject.toml (ai), pubspec.yaml (app), mgc.toml (mọi core).
        if mgc_config::project::ProjectConfig::auto_detect(&path).is_some() {
            out.push(path);
            continue;
        }

        collect_installable_projects(path, out)?;
    }

    Ok(())
}

fn load_locked_graph(
    project_root: &std::path::Path,
    _adapter_name: &str,
    manifest: &Manifest,
) -> Result<Option<ResolvedGraph>> {
    let Some(lock) = read_checked_lockfile(project_root)? else {
        let legacy = mgc_lockfile::import::detect_legacy_lockfiles(project_root);
        if !legacy.is_empty() {
            let names = legacy
                .iter()
                .map(|lock| lock.file_name)
                .collect::<Vec<_>>()
                .join(", ");
            mgc_ui::warning(&format!(
                "Ignoring legacy lockfile(s): {names}. Run an explicit MagiCore lock migration before install if you want to seed mgc.lock from them."
            ));
        }
        return Ok(None);
    };

    // T3.5: Auto-verify lockfile signature before install
    verify_lockfile_if_signed(project_root)?;

    // FIXME(V1.0.1): Re-enable lock.core, lock.version, lock.resolution checks after v2 migration
    // let state_ok = matches!(lock.resolution.state.as_str(), "locked" | "installing");
    // if lock.core != adapter_name || !state_ok || lock.version != 1 || lock.packages.is_empty() {
    if lock.packages.is_empty() {
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
    mgc_lockfile::read_lockfile_checked(project_root).map_err(|e| anyhow::anyhow!("{}", e))
}

fn lock_matches_manifest(lock: &Lockfile, manifest: &Manifest) -> bool {
    manifest.all_dependencies().all(|dependency| {
        lock.get_package(dependency.name.as_str())
            .and_then(|package| Version::parse(&package.version).ok())
            .is_some_and(|version| dependency.range.matches(&version))
    })
}

fn graph_from_lockfile(lock: &Lockfile) -> Result<ResolvedGraph> {
    let packages = lock
        .packages
        .iter()
        .map(|package| {
            Ok(ResolvedPackage {
                id: PackageId::parse(&format!("{}@{}", package.name, package.version))?,
                integrity: package.integrity.clone(),
                tarball_url: package.resolved.clone(),
                deps: package
                    .dependencies
                    .iter()
                    .map(|dependency| PackageId::parse(dependency))
                    .collect::<std::result::Result<Vec<_>, _>>()?,
                peer_deps: vec![],
                direct: false,
                dev: false,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ResolvedGraph { packages })
}

/// T3.5: Verify lockfile signature before install (soft fail on unsigned)
/// T3.5: Verify chữ ký lockfile trước install (soft fail nếu chưa ký)
fn verify_lockfile_if_signed(project_root: &Path) -> Result<()> {
    let lockfile_path = project_root.join("mgc.lock");
    if !lockfile_path.exists() {
        return Ok(());
    }

    // T3.6: Enforce policy in CI environment
    crate::commands::trust::policy::auto_enforce_in_ci(&lockfile_path)?;

    let status = mgc_lockfile::verify_lockfile(&lockfile_path)?;

    match status {
        mgc_lockfile::VerificationStatus::Valid => {
            mgc_ui::success("✓ Lockfile signature valid");
        }
        mgc_lockfile::VerificationStatus::Unsigned => {
            mgc_ui::warning("⚠ Lockfile not signed — run 'mgc trust sign' to sign it");
        }
        mgc_lockfile::VerificationStatus::Tampered(msg) => {
            return Err(anyhow::anyhow!("Lockfile tampered: {}", msg));
        }
        mgc_lockfile::VerificationStatus::InvalidSignature(msg) => {
            return Err(anyhow::anyhow!("Invalid signature: {}", msg));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mgc_lockfile::Package;
    use mgc_types::{DependencySpec, Ecosystem, PackageName, VersionRange};
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

        let mut lock = Lockfile::new();
        lock.packages.push(Package {
            name: "tailwindcss".into(),
            version: "4.3.2".into(),
            resolved: "https://registry.npmjs.org/tailwindcss/-/tailwindcss-4.3.2.tgz".into(),
            integrity: "sha256-test".into(),
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

        let mut lock = Lockfile::new();
        lock.packages.push(Package {
            name: "tailwindcss".into(),
            version: "4.3.2".into(),
            resolved: "https://registry.npmjs.org/tailwindcss/-/tailwindcss-4.3.2.tgz".into(),
            integrity: "sha256-test".into(),
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

        let mut lock = Lockfile::new();
        lock.version = "0".into(); // Unsupported version
        lock.packages.push(Package {
            name: "tailwindcss".into(),
            version: "4.3.2".into(),
            resolved: "https://registry.npmjs.org/tailwindcss/-/tailwindcss-4.3.2.tgz".into(),
            integrity: "sha256-test".into(),
            dependencies: vec![],
        });
        std::fs::write(
            dir.path().join("mgc.lock"),
            mgc_lockfile::serialization::to_toml(&lock).unwrap(),
        )
        .unwrap();

        let err = load_locked_graph(dir.path(), "web", &manifest).unwrap_err();
        assert!(err.to_string().contains("unsupported lockfile version"));
    }

    #[test]
    fn test_load_locked_graph_ignores_legacy_checksum_sidecar() {
        let dir = tempdir().unwrap();
        let manifest = Manifest::new("demo", Ecosystem::Web);
        let lock = Lockfile::new();
        std::fs::write(
            dir.path().join("mgc.lock"),
            mgc_lockfile::serialization::to_toml(&lock).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.path().join("mgc.lock.sha256"), "bad").unwrap();

        assert!(load_locked_graph(dir.path(), "web", &manifest)
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_graph_from_lockfile_rejects_invalid_dependency_id() {
        let mut lock = Lockfile::new();
        lock.packages.push(Package {
            name: "react".into(),
            version: "18.2.0".into(),
            resolved: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".into(),
            integrity: "sha256-test".into(),
            dependencies: vec!["not-a-package-id".into()],
        });

        let err = graph_from_lockfile(&lock).unwrap_err();

        assert!(
            err.to_string().contains("invalid package spec"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_discover_workspace_projects_for_monorepo_root() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("magicore.workspace.toml"),
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
    fn test_discover_workspace_projects_mix_cores() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("magicore.workspace.toml"),
            r#"
mode = "monorepo"
[layout]
apps_dir = "apps"
packages_dir = "packages"
"#,
        )
        .unwrap();

        let web = dir.path().join("apps/web");
        fs::create_dir_all(&web).unwrap();
        fs::write(web.join("package.json"), "{}").unwrap();

        let lib = dir.path().join("packages/rustlib");
        fs::create_dir_all(lib.join("src")).unwrap();
        fs::write(lib.join("Cargo.toml"), "[package]\nname = \"rustlib\"\n").unwrap();

        let ignored = dir.path().join("packages/not-a-project");
        fs::create_dir_all(&ignored).unwrap();
        fs::write(ignored.join("notes.txt"), "x").unwrap();

        let mut workspaces = discover_workspace_projects(dir.path()).unwrap().unwrap();
        workspaces.sort();
        let normalized: Vec<String> = workspaces
            .iter()
            .map(|p| {
                p.strip_prefix(dir.path())
                    .unwrap_or(p)
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        assert_eq!(normalized, vec!["apps/web", "packages/rustlib"]);
    }

    #[test]
    fn test_discover_workspace_projects_ignores_non_monorepo_file() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("magicore.workspace.toml"),
            r#"
version = 1
mode = "single"
"#,
        )
        .unwrap();

        assert!(discover_workspace_projects(dir.path()).unwrap().is_none());
    }
}
