#![allow(dead_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
// use mgc_lockfile::{
//     serialization, LockPackage, Lockfile, LockfileSigner, ResolutionMeta, WorkspaceLock,
// };
use mgc_types::adapter::PackageAdapter;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;

use crate::commands::core::scaffold_flags::ScaffoldFlags;
use crate::commands::core::shared;
use mgc_types::Ecosystem;
use mgc_ui::info;

const DEFAULT_NPM_REGISTRY: &str = "https://registry.npmjs.org";
const SCAFFOLD_VERSION_OVERRIDES_ENV: &str = "MAGICORE_WEB_SCAFFOLD_VERSION_OVERRIDES";
/// Version baseline — trước đây ở templates/web/versions/scaffold-baseline.toml;
/// registry-first (templates/ xóa khỏi repo) → giữ thẳng trong code.
const SCAFFOLD_BASELINE_VERSIONS_TOML: &str = r#"[versions]
vue = "^3.5.39"
react = "^19.2.7"
react-dom = "^19.2.7"
vite = "^8.1.4"
"@vitejs/plugin-react" = "^6.0.3"
"@vitejs/plugin-vue" = "^6.0.7"
solid-js = "^1.9.14"
vite-plugin-solid = "^2.11.12"
typescript = "^5.9.2"
"@types/react" = "^19.2.17"
"@types/react-dom" = "^19.2.3"
"@types/node" = "^26.1.1"
tailwindcss = "^4.3.2"
"@sveltejs/kit" = "^2.69.2"
"@sveltejs/vite-plugin-svelte" = "^7.2.0"
"@sveltejs/adapter-auto" = "^7.0.1"
svelte = "^5.56.4"
next = "^16.2.10"
nuxt = "^4.4.8"
"@angular/core" = "^22.0.6"
"@angular/platform-browser" = "^22.0.6"
"@angular/platform-browser-dynamic" = "^22.0.6"
"@angular/router" = "^22.0.6"
"@angular/compiler" = "^22.0.6"
"@angular/common" = "^22.0.6"
rxjs = "^7.8.2"
"zone.js" = "^0.16.2"
tslib = "^2.8.1"
"@angular/cli" = "^22.0.6"
"@angular/compiler-cli" = "^22.0.6"
"@angular-devkit/build-angular" = "^22.0.6"
"@builder.io/qwik" = "^1.20.0"
"@builder.io/qwik-city" = "^1.20.0"
astro = "^7.0.7"
express = "^5.2.1"
"@types/express" = "^5.0.6"
hono = "^4.12.30"
"@hono/node-server" = "^1.19.6"
"@nestjs/core" = "^11.1.28"
"@nestjs/common" = "^11.1.28"
"@nestjs/platform-express" = "^11.1.28"
reflect-metadata = "^0.2.2"
zod = "^4.4.3"
"@trpc/server" = "^11.18.0"
fastify = "^5.10.0"
tsx = "^4.23.0"
"@prisma/client" = "^6.6.0"
prisma = "^6.6.0"
vitest = "^3.2.0"
eslint = "^9.28.0"
eslint-config-next = "^16.2.10"
prettier = "^3.6.0"
"@tailwindcss/postcss" = "^4.3.2"
pg = "^8.15.0"
zustand = "^5.0.3"
"@tanstack/react-query" = "^5.62.0"
next-auth = "^5.0.0"
"@playwright/test" = "^1.52.0"
husky = "^9.2.0"
lint-staged = "^15.5.0"
"@biomejs/biome" = "^1.9.0"
clsx = "^2.1.0"
tailwind-merge = "^3.2.0"
class-variance-authority = "^0.7.0"
sass = "^1.83.0"
unocss = "^65.5.0"
daisyui = "^4.12.0"
"@reduxjs/toolkit" = "^2.6.0"
react-redux = "^9.2.0"
jest = "^29.7.0"
"@testing-library/react" = "^16.0.0"
"@testing-library/jest-dom" = "^6.6.0"
jest-environment-jsdom = "^29.7.0"
cypress = "^14.2.0"
drizzle-orm = "^0.38.0"
drizzle-kit = "^0.30.0"
"@clerk/nextjs" = "^6.12.0"
styled-components = "^6.1.0"
"@types/styled-components" = "^5.1.0"
"@commitlint/cli" = "^19.5.0"
"@commitlint/config-conventional" = "^19.5.0"
"@apollo/server" = "^4.11.0"
"@as-integrations/next" = "^3.2.0"
"@trpc/client" = "^11.0.0"
"@trpc/next" = "^11.0.0"
"@grpc/grpc-js" = "^1.11.0"
"@grpc/proto-loader" = "^0.7.0"
lucia = "^3.2.0"
"@lucia-auth/adapter-drizzle" = "^1.1.0"
jose = "^5.7.0"
dotenv-cli = "^7.4.0"
next-i18next = "^15.3.0"
next-pwa = "^5.6.0"
"@storybook/nextjs" = "^8.2.0"
"@storybook/react" = "^8.2.0"
"@sentry/nextjs" = "^8.21.0"
"@vercel/analytics" = "^1.3.0"
"@railway/cli" = "^4.2.0"
flyctl = "^0.2.0"
"#;

fn web_command_profile_enabled() -> bool {
    std::env::var_os("MAGICORE_WEB_PROFILE_COMMAND").is_some()
}

fn web_command_profile_mark(label: &str, started_at: std::time::Instant) {
    if web_command_profile_enabled() {
        eprintln!(
            "[magicore:web:web-command-profile] {}={}ms",
            label,
            started_at.elapsed().as_millis()
        );
    }
}

fn install_hint_command() -> &'static str {
    #[cfg(all(
        feature = "web",
        not(any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        ))
    ))]
    {
        return "mgc install";
    }

    #[cfg(all(
        feature = "web",
        any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        )
    ))]
    {
        return "mgc install-web";
    }

    #[allow(unreachable_code)]
    "mgc install"
}

/// Find project root for web commands
fn project_root() -> Result<std::path::PathBuf> {
    let started_at = std::time::Instant::now();
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?
        .ok_or_else(|| crate::error::no_mgc_project_found("web"))?;
    web_command_profile_mark("project_root", started_at);
    Ok(root)
}

fn web_adapter() -> Arc<dyn PackageAdapter> {
    let started_at = std::time::Instant::now();
    let registry_url = std::env::var("MAGICORE_WEB_REGISTRY_URL").ok();
    let token = std::env::var("MAGICORE_WEB_REGISTRY_TOKEN").ok();
    let adapter =
        crate::factory::create_adapter(&Ecosystem::Web, registry_url.as_deref(), token.as_deref())
            .expect("web adapter always available in web core build");
    web_command_profile_mark("web_adapter", started_at);
    adapter
}

/// Add web dependency
#[allow(clippy::too_many_arguments)]
pub async fn add(
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
    let started_at = std::time::Instant::now();
    let root = project_root()?;
    let adapter = web_adapter();
    let result = shared::add(
        &*adapter, &root, packages, version, dev, exact, optional, peer, no_save, install, global,
    )
    .await;
    web_command_profile_mark("web_add_total", started_at);
    result
}

/// Remove web dependencies
pub async fn remove(packages: Vec<String>, install: bool) -> Result<()> {
    let started_at = std::time::Instant::now();
    let root = project_root()?;
    let adapter = web_adapter();
    let result = shared::remove(&*adapter, &root, packages, install).await;
    web_command_profile_mark("web_remove_total", started_at);
    result
}

/// List web packages
pub async fn list() -> Result<()> {
    let started_at = std::time::Instant::now();
    let root = project_root()?;
    let adapter = web_adapter();
    let result = shared::list(&*adapter, &root).await;
    web_command_profile_mark("web_list_total", started_at);
    result
}

/// Update web packages
pub async fn update(packages: Vec<String>, install: bool) -> Result<()> {
    let started_at = std::time::Instant::now();
    let root = project_root()?;
    let adapter = web_adapter();
    let result = shared::update(&*adapter, &root, packages, install).await;
    web_command_profile_mark("web_update_total", started_at);
    result
}

pub async fn install(
    packages: Vec<String>,
    frozen: bool,
    ignore_scripts: bool,
    allow_scripts: bool,
    prefer_dedupe: bool,
    repair: bool,
    _offline: bool, // Issue #3: Implement offline mode (v1.2.0 milestone)
) -> Result<()> {
    let root = project_root()?;
    let adapter: Arc<dyn PackageAdapter> = web_adapter();
    let targets = install_targets(&root)?;

    // Dedupe opt-in (02 §2.1): CLI flag OR mgc.toml [dedupe] prefer = true.
    let mut dedupe_enabled = prefer_dedupe;
    if !dedupe_enabled {
        if let Ok(Some(cfg)) = mgc_config::project::ProjectConfig::load(&root) {
            dedupe_enabled = cfg.dedupe.prefer;
        }
    }
    if dedupe_enabled {
        adapter.set_dedupe_pref(true);
        // Shared lock seeding: root mgc.lock (workspace aggregate) + mỗi web target lock.
        let mut lock_roots: Vec<&std::path::Path> = vec![root.as_path()];
        lock_roots.extend(
            targets
                .iter()
                .filter(|target| target.join("package.json").exists())
                .map(|target| target.as_path()),
        );
        // Issue #4: existing_versions_from disabled - restore after lockfile v2 migration complete
        // if let Ok(existing) = mgc_lockfile::existing_versions_from(&lock_roots) {
        if let Ok(_existing) = Ok::<Vec<String>, ()>(Vec::new()) {
            // Skipping set_existing_versions call until v2 migration complete
            // if !existing.is_empty() {
            //     adapter.set_existing_versions(existing);
            // }
        }
    }

    for pkg in &packages {
        let spinner = mgc_ui::create_spinner(&format!("  Adding {}...", pkg));
        let name = mgc_types::PackageName::new(pkg)?;
        let opts = mgc_types::adapter::AddOptions::default();
        adapter.add(&root, &name, None, opts).await?;
        spinner.finish_and_clear();
    }

    let project_mode = detect_project_mode(&root)?;
    if matches!(project_mode, WebProjectMode::Monorepo) {
        // Mix core (Q23): mỗi workspace project detect core riêng — web targets
        // đi qua web adapter, target khác (lib/app/ai/...) qua adapter của nó.
        let mut web_targets = Vec::new();
        let mut mix_targets = Vec::new();
        for target in &targets {
            if target.join("package.json").exists() {
                web_targets.push(target.clone());
            } else {
                mix_targets.push(target.clone());
            }
        }

        if !web_targets.is_empty() {
            install_monorepo_targets(
                &adapter,
                &web_targets,
                frozen,
                ignore_scripts,
                allow_scripts,
                prefer_dedupe,
                repair,
            )
            .await?;
            link_monorepo_workspace_packages(&root, &web_targets)?;
            write_monorepo_root_lockfile(&root, &web_targets)?;
        }

        for target in &mix_targets {
            let ctx = crate::context::ProjectContext::load_for_dir(target)?;
            mgc_ui::info(&format!("Installing workspace: {}", target.display()));
            crate::commands::install::install_into_root_ws(
                ctx.adapter(),
                target,
                &packages,
                ignore_scripts,
                allow_scripts,
                false, // offline - Issue #3: pass from command args when offline mode implemented
            )
            .await?;
        }
    } else {
        for target in &targets {
            if target.join("package.json").exists() {
                shared::install_with_adapter(
                    adapter.as_ref(),
                    target,
                    "mgc add",
                    frozen,
                    mgc_types::adapter::InstallOptions {
                        allow_scripts,
                        prefer_dedupe: dedupe_enabled,
                        repair,
                        legacy_flat: shared::should_use_legacy_flat_layout("web"),
                        ..Default::default()
                    },
                )
                .await?;
            } else {
                native_install_target(target)?;
            }
        }
    }

    Ok(())
}

pub async fn dev_at_root(
    project_root: &Path,
    host: Option<String>,
    port: Option<u16>,
) -> Result<()> {
    let targets = dev_targets(project_root, host, port)?;
    if targets.len() == 1 {
        return run_single_dev_target(&targets[0]).await;
    }

    run_multi_dev_targets(&targets).await
}

#[derive(Debug)]
struct DevLaunch {
    program: PathBuf,
    args: Vec<OsString>,
    envs: Vec<(OsString, OsString)>,
}

impl DevLaunch {
    fn describe(&self) -> String {
        let args = self
            .args
            .iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        if args.is_empty() {
            self.program.display().to_string()
        } else {
            format!("{} {}", self.program.display(), args)
        }
    }
}

#[derive(Debug, Deserialize)]
struct DevPackageJson {
    scripts: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceConfig {
    mode: Option<String>,
    layout: Option<WorkspaceLayout>,
    workspace: Option<WorkspaceManifest>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceLayout {
    apps_dir: Option<String>,
    packages_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceManifest {
    apps: Option<Vec<String>>,
    packages: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
struct DevTarget {
    dir: PathBuf,
    role: &'static str,
    script_name: &'static str,
    host: Option<String>,
    port: Option<u16>,
}

#[derive(Debug, Clone)]
enum WebProjectMode {
    Standalone,
    FullstackSplit,
    Monorepo,
}

fn detect_dev_target(project_root: &Path) -> Result<PathBuf> {
    let root_script = read_dev_script(project_root)?;
    if root_script
        .as_deref()
        .is_some_and(|script| !script.starts_with("mgc "))
    {
        return Ok(project_root.to_path_buf());
    }

    let frontend = workspace_frontend_dir(project_root)?;
    if frontend.join("package.json").exists() {
        return Ok(frontend);
    }

    if root_script.is_some() {
        return Ok(project_root.to_path_buf());
    }

    if infer_native_dev_launch(project_root, None, None).is_ok() {
        return Ok(project_root.to_path_buf());
    }

    Err(crate::error::web_no_dev_target(
        project_root,
        install_hint_command(),
    ))
}

fn workspace_frontend_dir(project_root: &Path) -> Result<PathBuf> {
    let workspace_path = project_root.join("magicore.workspace.toml");
    if workspace_path.exists() {
        let contents = std::fs::read_to_string(&workspace_path)?;
        let config: WorkspaceConfig = toml::from_str(&contents)?;
        if is_workspace_monorepo(&config) {
            let apps_dir = config
                .layout
                .as_ref()
                .and_then(|layout| layout.apps_dir.as_deref())
                .unwrap_or("apps");
            return Ok(project_root.join(apps_dir).join("frontend"));
        }
    }
    Ok(project_root.join("apps").join("frontend"))
}

fn workspace_backend_dir(project_root: &Path) -> Result<PathBuf> {
    let workspace_path = project_root.join("magicore.workspace.toml");
    if workspace_path.exists() {
        let contents = std::fs::read_to_string(&workspace_path)?;
        let config: WorkspaceConfig = toml::from_str(&contents)?;
        if is_workspace_monorepo(&config) {
            let apps_dir = config
                .layout
                .as_ref()
                .and_then(|layout| layout.apps_dir.as_deref())
                .unwrap_or("apps");
            return Ok(project_root.join(apps_dir).join("backend"));
        }
    }
    Ok(project_root.join("apps").join("backend"))
}

fn detect_project_mode(project_root: &Path) -> Result<WebProjectMode> {
    let workspace_path = project_root.join("magicore.workspace.toml");
    if workspace_path.exists() {
        let contents = std::fs::read_to_string(&workspace_path)?;
        let config: WorkspaceConfig = toml::from_str(&contents)?;
        if is_workspace_monorepo(&config) {
            return Ok(WebProjectMode::Monorepo);
        }
    }

    if has_backend_manifest(&project_root.join("server")) {
        return Ok(WebProjectMode::FullstackSplit);
    }

    Ok(WebProjectMode::Standalone)
}

fn install_targets(project_root: &Path) -> Result<Vec<PathBuf>> {
    match detect_project_mode(project_root)? {
        WebProjectMode::Standalone => Ok(vec![project_root.to_path_buf()]),
        WebProjectMode::FullstackSplit => Ok(vec![
            project_root.to_path_buf(),
            project_root.join("server"),
        ]),
        WebProjectMode::Monorepo => discover_monorepo_install_targets(project_root),
    }
}

fn is_workspace_monorepo(config: &WorkspaceConfig) -> bool {
    if config.mode.as_deref() == Some("monorepo") {
        return true;
    }

    config.workspace.as_ref().is_some_and(|workspace| {
        workspace.apps.as_ref().is_some_and(|apps| !apps.is_empty())
            || workspace
                .packages
                .as_ref()
                .is_some_and(|packages| !packages.is_empty())
    })
}

fn discover_monorepo_install_targets(project_root: &Path) -> Result<Vec<PathBuf>> {
    mgc_workspace::discover_workspace_targets(project_root)
}

#[derive(Debug, Deserialize)]
struct WorkspacePackageJson {
    name: String,
    #[serde(default)]
    dependencies: std::collections::HashMap<String, String>,
    #[serde(default, rename = "devDependencies")]
    dev_dependencies: std::collections::HashMap<String, String>,
    #[serde(default, rename = "peerDependencies")]
    peer_dependencies: std::collections::HashMap<String, String>,
    #[serde(default, rename = "optionalDependencies")]
    optional_dependencies: std::collections::HashMap<String, String>,
}

fn link_monorepo_workspace_packages(project_root: &Path, targets: &[PathBuf]) -> Result<()> {
    let package_targets = targets
        .iter()
        .filter(|path| path.join("package.json").exists())
        .cloned()
        .collect::<Vec<_>>();
    if package_targets.is_empty() {
        return Ok(());
    }

    let mut workspace_index = std::collections::HashMap::<String, PathBuf>::new();
    let mut manifests = Vec::new();

    for target in &package_targets {
        let manifest = read_workspace_package_manifest(target)?;
        workspace_index.insert(manifest.name.clone(), target.clone());
        manifests.push((target.clone(), manifest));
    }

    for (target, manifest) in manifests {
        let node_modules = target.join("node_modules");
        std::fs::create_dir_all(&node_modules)?;

        for (dep_name, spec) in manifest
            .dependencies
            .into_iter()
            .chain(manifest.dev_dependencies)
            .chain(manifest.peer_dependencies)
            .chain(manifest.optional_dependencies)
        {
            if !spec.trim().starts_with("workspace:") {
                continue;
            }
            let Some(source_dir) = workspace_index.get(&dep_name) else {
                return Err(crate::error::web_workspace_dep_missing(
                    &dep_name,
                    &target,
                    project_root,
                ));
            };

            let link_path = node_modules.join(dep_name.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = link_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if link_path.exists() {
                let metadata = std::fs::symlink_metadata(&link_path)?;
                if metadata.file_type().is_symlink() || metadata.is_file() {
                    std::fs::remove_file(&link_path)?;
                } else if metadata.is_dir() {
                    std::fs::remove_dir_all(&link_path)?;
                }
            }
            create_workspace_dir_link(source_dir, &link_path)?;
        }
    }

    Ok(())
}

fn read_workspace_package_manifest(project_root: &Path) -> Result<WorkspacePackageJson> {
    let path = project_root.join("package.json");
    let contents = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "failed to read workspace package manifest '{}'",
            path.display()
        )
    })?;
    serde_json::from_str(&contents).with_context(|| {
        format!(
            "failed to parse workspace package manifest '{}'",
            path.display()
        )
    })
}

async fn install_monorepo_targets(
    adapter: &Arc<dyn PackageAdapter>,
    targets: &[PathBuf],
    frozen: bool,
    ignore_scripts: bool,
    allow_scripts: bool,
    prefer_dedupe: bool,
    repair: bool,
) -> Result<()> {
    let mut native_targets = Vec::new();
    let mut package_targets = Vec::new();

    for target in targets {
        if target.join("package.json").exists() {
            package_targets.push(target.clone());
        } else {
            native_targets.push(target.clone());
        }
    }

    if !package_targets.is_empty() {
        let graph = mgc_workspace::build_workspace_graph(&package_targets)?;
        let levels =
            mgc_workspace::topo_levels(&graph).map_err(|e| crate::error::topo_order_failed(&e))?;

        let concurrency = monorepo_install_concurrency(&package_targets);
        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

        for level in &levels {
            let mut join_set = tokio::task::JoinSet::new();
            for &node_index in level {
                let node = graph.nodes[node_index].clone();
                let adapter = Arc::clone(adapter);
                let semaphore = Arc::clone(&semaphore);
                join_set.spawn(async move {
                    let _permit = semaphore
                        .acquire_owned()
                        .await
                        .map_err(|e| crate::error::install_slot_failed(&e))?;
                    mgc_ui::info(&format!("Installing workspace: {}", node.path.display()));
                    install_web_target_quiet(
                        adapter.as_ref(),
                        &node.path,
                        frozen,
                        ignore_scripts,
                        allow_scripts,
                        prefer_dedupe,
                        repair,
                    )
                    .await?;
                    Ok::<PathBuf, anyhow::Error>(node.path)
                });
            }
            while let Some(result) = join_set.join_next().await {
                result.map_err(|e| crate::error::install_task_failed(&e))??;
            }
        }
    }

    for target in native_targets {
        native_install_target(&target)?;
    }

    Ok(())
}

fn monorepo_install_concurrency(package_targets: &[PathBuf]) -> usize {
    if package_targets.is_empty() {
        return 1;
    }

    if let Some(override_value) = std::env::var("MAGICORE_WEB_MONOREPO_INSTALL_CONCURRENCY")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
    {
        return override_value;
    }

    let default = std::thread::available_parallelism()
        .map(|count| count.get().min(4))
        .unwrap_or(2)
        .max(1);

    if looks_like_cold_monorepo_install(package_targets) {
        return 1;
    }

    default
}

fn looks_like_cold_monorepo_install(package_targets: &[PathBuf]) -> bool {
    if package_targets.len() <= 1 {
        return false;
    }

    package_targets.iter().all(|target| {
        !target.join("node_modules").exists()
            && !target.join("mgc.lock").exists()
            && !target.join(".magicore").join("cache").join("web").exists()
    })
}

async fn install_web_target_quiet(
    adapter: &dyn PackageAdapter,
    target: &Path,
    frozen: bool,
    ignore_scripts: bool,
    allow_scripts: bool,
    prefer_dedupe: bool,
    repair: bool,
) -> Result<()> {
    let execution = shared::prepare_install_execution(adapter, target, frozen, None).await?;
    if execution.graph.is_empty() {
        return Ok(());
    }
    let opts = mgc_types::adapter::InstallOptions {
        ignore_scripts,
        allow_scripts,
        prefer_dedupe,
        repair,
        legacy_flat: shared::should_use_legacy_flat_layout("web"),
        frozen,
        ..Default::default()
    };
    adapter.install(&execution.graph, target, opts).await?;
    Ok(())
}

// Issue #4: Disabled due to lockfile v2 migration (uses LockPackage, WorkspaceLock, ResolutionMeta)
fn write_monorepo_root_lockfile(_project_root: &Path, _targets: &[PathBuf]) -> Result<()> {
    // Workspace lockfile merging requires v2 schema rewrite
    // For now, each workspace maintains its own lockfile
    Ok(())
}

fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp_path = dir.join(format!(".mgc-tmp-{}", std::process::id()));
    std::fs::write(&tmp_path, data)
        .with_context(|| format!("failed to write temp file '{}'", tmp_path.display()))?;
    std::fs::rename(&tmp_path, path).with_context(|| {
        let _ = std::fs::remove_file(&tmp_path);
        format!(
            "failed to rename temp file '{}' into '{}'",
            tmp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

#[cfg(unix)]
fn create_workspace_dir_link(source: &Path, target: &Path) -> Result<()> {
    std::os::unix::fs::symlink(source, target).with_context(|| {
        format!(
            "failed to symlink workspace package '{}' -> '{}'",
            target.display(),
            source.display()
        )
    })?;
    Ok(())
}

#[cfg(windows)]
fn create_workspace_dir_link(source: &Path, target: &Path) -> Result<()> {
    std::os::windows::fs::symlink_dir(source, target).with_context(|| {
        format!(
            "failed to symlink workspace package '{}' -> '{}'",
            target.display(),
            source.display()
        )
    })?;
    Ok(())
}

fn dev_targets(
    project_root: &Path,
    host: Option<String>,
    port: Option<u16>,
) -> Result<Vec<DevTarget>> {
    let fullstack_backend_port = Some(3415);
    let monorepo_backend_port = Some(3415);

    match detect_project_mode(project_root)? {
        WebProjectMode::Standalone => Ok(vec![DevTarget {
            dir: detect_dev_target(project_root)?,
            role: "frontend",
            script_name: "dev",
            host,
            port,
        }]),
        WebProjectMode::FullstackSplit => Ok(vec![
            DevTarget {
                dir: project_root.to_path_buf(),
                role: "frontend",
                script_name: "dev:client",
                host,
                port,
            },
            DevTarget {
                dir: project_root.join("server"),
                role: "backend",
                script_name: "dev",
                host: None,
                port: fullstack_backend_port,
            },
        ]),
        WebProjectMode::Monorepo => Ok(vec![
            DevTarget {
                dir: workspace_frontend_dir(project_root)?,
                role: "frontend",
                script_name: "dev",
                host,
                port,
            },
            DevTarget {
                dir: workspace_backend_dir(project_root)?,
                role: "backend",
                script_name: "dev",
                host: None,
                port: monorepo_backend_port,
            },
        ]),
    }
}

fn read_script(project_root: &Path, script_name: &str) -> Result<Option<String>> {
    let package_json_path = project_root.join("package.json");
    if !package_json_path.exists() {
        return Ok(None);
    }

    let parsed: DevPackageJson =
        serde_json::from_str(&std::fs::read_to_string(package_json_path)?)?;
    let scripts = match parsed.scripts {
        Some(scripts) => scripts,
        None => return Ok(None),
    };

    if let Some(script) = scripts.get(script_name).cloned() {
        return Ok(Some(script));
    }

    if script_name == "dev" {
        return Ok(scripts.get("start").cloned());
    }

    Ok(None)
}

fn read_dev_script(project_root: &Path) -> Result<Option<String>> {
    read_script(project_root, "dev")
}

fn has_arg(args: &[OsString], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn append_dev_endpoint_args(
    args: &mut Vec<OsString>,
    host_flag: &str,
    port_flag: &str,
    host: Option<String>,
    port: Option<u16>,
) {
    if let Some(host) = host {
        if !has_arg(args, host_flag) {
            args.push(OsString::from(host_flag));
            args.push(OsString::from(host));
        }
    }
    if let Some(port) = port {
        if !has_arg(args, port_flag) {
            args.push(OsString::from(port_flag));
            args.push(OsString::from(port.to_string()));
        }
    }
}

/// Validate runtime args for Bun/Deno using shared launcher policy
/// Kiểm tra args runtime cho Bun/Deno dùng policy launcher chung
///
/// SAFETY: Uses centralized launcher_policy module for consistent validation
/// AN TOÀN: Dùng module launcher_policy tập trung để kiểm tra nhất quán
fn validate_runtime_args(runtime: &str, args: &[&str]) -> Result<()> {
    use crate::commands::launcher_policy::{LauncherPolicy, Runtime};

    let rt = match runtime {
        "bun" => Runtime::Bun,
        "deno" => Runtime::Deno,
        "node" => Runtime::Node,
        _ => return Ok(()), // Unknown runtime, skip validation
    };

    let policy = LauncherPolicy::dev_server(rt);
    policy.validate_args(args)
}

/// Detect runtime from script tokens for optimizer env loading
/// Phát hiện runtime từ script tokens để load env optimizer
fn detect_runtime_from_tokens(
    tokens: &[&str],
) -> crate::commands::optimizer::runtime_detect::DetectedRuntime {
    use crate::commands::optimizer::runtime_detect::{DetectedRuntime, PackageManager};

    if tokens.is_empty() {
        return DetectedRuntime::Unknown;
    }

    match tokens[0] {
        "bun" => DetectedRuntime::Bun,
        "deno" => DetectedRuntime::Deno,
        "node" | "npm" | "pnpm" | "yarn" | "vite" | "next" | "webpack" | "react-scripts" => {
            DetectedRuntime::NodeJs {
                package_manager: PackageManager::Npm, // Default, actual PM doesn't matter for env loading
            }
        }
        _ => DetectedRuntime::Unknown,
    }
}

fn build_dev_launch(
    project_root: &Path,
    script_name: &str,
    host: Option<String>,
    port: Option<u16>,
) -> Result<DevLaunch> {
    let script = match read_script(project_root, script_name)? {
        Some(script) => script,
        None => return infer_native_dev_launch(project_root, host, port),
    };

    reject_external_package_manager_script(&script, &project_root.join("package.json"))?;

    let tokens: Vec<&str> = script.split_whitespace().collect();
    if tokens.is_empty() {
        return Err(crate::error::web_empty_dev_script(project_root));
    }

    // Detect runtime from script to load correct optimizer config
    // Phát hiện runtime từ script để load đúng config optimizer
    let runtime = detect_runtime_from_tokens(&tokens);
    let optimizer_envs =
        crate::commands::optimizer::env_loader::load_optimizer_env(project_root, &runtime)
            .map_err(|e| {
                mgc_ui::warning(&format!("Failed to load optimizer config: {}", e));
                e
            })
            .unwrap_or_default();
    let base_envs: Vec<(OsString, OsString)> = optimizer_envs
        .into_iter()
        .map(|(k, v)| (OsString::from(k), OsString::from(v)))
        .collect();

    match tokens.as_slice() {
        // Bun runtime (allowed in DevServer scope with project script)
        // Runtime Bun (cho phép trong scope DevServer với script của project)
        // SAFETY: Validate args - no --eval, no arbitrary code execution
        ["bun", "run", rest @ ..] => {
            validate_runtime_args("bun", rest)?;
            Ok(DevLaunch {
                program: PathBuf::from("bun"),
                args: {
                    let mut args = vec![OsString::from("run")];
                    args.extend(rest.iter().map(OsString::from));
                    args
                },
                envs: base_envs.clone(),
            })
        }
        ["bun", rest @ ..] => {
            validate_runtime_args("bun", rest)?;
            Ok(DevLaunch {
                program: PathBuf::from("bun"),
                args: rest.iter().map(OsString::from).collect(),
                envs: base_envs.clone(),
            })
        }
        // Deno runtime (allowed in DevServer scope with project script)
        // Runtime Deno (cho phép trong scope DevServer với script của project)
        // SAFETY: Validate args - no --eval, restrict dangerous permissions
        ["deno", "run", rest @ ..] => {
            validate_runtime_args("deno", rest)?;
            Ok(DevLaunch {
                program: PathBuf::from("deno"),
                args: {
                    let mut args = vec![OsString::from("run")];
                    args.extend(rest.iter().map(OsString::from));
                    args
                },
                envs: base_envs.clone(),
            })
        }
        ["deno", "task", rest @ ..] => {
            validate_runtime_args("deno", rest)?;
            Ok(DevLaunch {
                program: PathBuf::from("deno"),
                args: {
                    let mut args = vec![OsString::from("task")];
                    args.extend(rest.iter().map(OsString::from));
                    args
                },
                envs: base_envs.clone(),
            })
        }
        ["vite"] | ["vite", "dev"] => {
            let mut args = Vec::new();
            append_dev_endpoint_args(&mut args, "--host", "--port", host, port);
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "vite")?,
                args,
                envs: base_envs,
            })
        }
        ["vite", rest @ ..] => {
            let mut args: Vec<OsString> = rest.iter().map(OsString::from).collect();
            append_dev_endpoint_args(&mut args, "--host", "--port", host, port);
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "vite")?,
                args,
                envs: base_envs,
            })
        }
        ["next", "dev"] => {
            let mut args = vec![OsString::from("dev")];
            append_dev_endpoint_args(&mut args, "-H", "-p", host, port);
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "next")?,
                args,
                envs: base_envs,
            })
        }
        ["next", "dev", rest @ ..] => {
            let mut args = vec![OsString::from("dev")];
            args.extend(rest.iter().map(OsString::from));
            append_dev_endpoint_args(&mut args, "-H", "-p", host, port);
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "next")?,
                args,
                envs: base_envs,
            })
        }
        ["nuxt", "dev"] | ["nuxt", "dev", "--host"] => {
            let mut args = vec![OsString::from("dev")];
            append_dev_endpoint_args(&mut args, "--host", "--port", host, port);
            let mut envs = base_envs.clone();
            envs.extend(vec![
                (
                    OsString::from("NUXT_TELEMETRY_DISABLED"),
                    OsString::from("1"),
                ),
                (
                    OsString::from("NUXT_TELEMETRY_CONSENT"),
                    OsString::from("0"),
                ),
            ]);
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "nuxt")?,
                args,
                envs,
            })
        }
        ["nuxt", "dev", rest @ ..] => {
            let mut args = vec![OsString::from("dev")];
            args.extend(rest.iter().map(OsString::from));
            append_dev_endpoint_args(&mut args, "--host", "--port", host, port);
            let mut envs = base_envs.clone();
            envs.extend(vec![
                (
                    OsString::from("NUXT_TELEMETRY_DISABLED"),
                    OsString::from("1"),
                ),
                (
                    OsString::from("NUXT_TELEMETRY_CONSENT"),
                    OsString::from("0"),
                ),
            ]);
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "nuxt")?,
                args,
                envs,
            })
        }
        ["astro", "dev"] => {
            let mut args = vec![OsString::from("dev")];
            append_dev_endpoint_args(&mut args, "--host", "--port", host, port);
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "astro")?,
                args,
                envs: base_envs.clone(),
            })
        }
        ["astro", "dev", rest @ ..] => {
            let mut args = vec![OsString::from("dev")];
            args.extend(rest.iter().map(OsString::from));
            append_dev_endpoint_args(&mut args, "--host", "--port", host, port);
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "astro")?,
                args,
                envs: base_envs.clone(),
            })
        }
        ["remix", "vite:dev"] => {
            let mut args = vec![OsString::from("vite:dev")];
            append_dev_endpoint_args(&mut args, "--host", "--port", host, port);
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "remix")?,
                args,
                envs: base_envs.clone(),
            })
        }
        ["remix", "vite:dev", rest @ ..] => {
            let mut args = vec![OsString::from("vite:dev")];
            args.extend(rest.iter().map(OsString::from));
            append_dev_endpoint_args(&mut args, "--host", "--port", host, port);
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "remix")?,
                args,
                envs: base_envs.clone(),
            })
        }
        ["ng", "serve"] => {
            let mut args = vec![OsString::from("serve")];
            append_dev_endpoint_args(&mut args, "--host", "--port", host, port);
            let mut envs = base_envs.clone();
            envs.extend(vec![
                (OsString::from("NG_CLI_ANALYTICS"), OsString::from("false")),
                (OsString::from("CI"), OsString::from("1")),
            ]);
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "ng")?,
                args,
                envs,
            })
        }
        ["ng", "serve", rest @ ..] => {
            let mut args = vec![OsString::from("serve")];
            args.extend(rest.iter().map(OsString::from));
            append_dev_endpoint_args(&mut args, "--host", "--port", host, port);
            let mut envs = base_envs.clone();
            envs.extend(vec![
                (OsString::from("NG_CLI_ANALYTICS"), OsString::from("false")),
                (OsString::from("CI"), OsString::from("1")),
            ]);
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "ng")?,
                args,
                envs,
            })
        }
        ["node", rest @ ..] => Ok(DevLaunch {
            program: PathBuf::from("node"),
            args: rest.iter().map(OsString::from).collect(),
            envs: base_envs.clone(),
        }),
        ["tsx", rest @ ..] => Ok(DevLaunch {
            program: resolve_local_bin(project_root, "tsx")?,
            args: rest.iter().map(OsString::from).collect(),
            envs: base_envs,
        }),
        _ => Err(crate::error::web_unsupported_dev_script(
            &script,
            project_root,
        )),
    }
}

fn reject_external_package_manager_script(script: &str, manifest_path: &Path) -> Result<()> {
    // Allow bun/deno when used as runtime (not package manager)
    // Cho phép bun/deno khi dùng như runtime (không phải package manager)
    let script_lower = script.to_lowercase();
    if script_lower.starts_with("bun run ")
        || script_lower.starts_with("deno run ")
        || script_lower.starts_with("deno task ")
    {
        return Ok(()); // Runtime usage allowed in DevServer scope
    }

    if let Some(pm) = mgc_exec::allowlist::find_forbidden_tool_in_script(script) {
        return Err(crate::error::web_forbidden_pm(script, manifest_path, pm));
    }
    Ok(())
}

fn has_backend_manifest(dir: &Path) -> bool {
    dir.join("package.json").exists()
        || dir.join("go.mod").exists()
        || dir.join("Cargo.toml").exists()
        || dir.join("manage.py").exists()
        || dir.join("main.py").exists()
        || dir.join("src/main.py").exists()
        || dir.join("pom.xml").exists()
        || dir.join("artisan").exists()
        || dir.join("composer.json").exists()
}

fn infer_native_dev_launch(
    project_root: &Path,
    host: Option<String>,
    port: Option<u16>,
) -> Result<DevLaunch> {
    let host_str = host.as_deref().unwrap_or("localhost");
    if project_root.join("go.mod").exists() {
        let go_dir = if project_root.join("cmd/server").exists() {
            OsString::from("./cmd/server")
        } else {
            OsString::from(".")
        };
        return Ok(DevLaunch {
            program: PathBuf::from("go"),
            args: vec![OsString::from("run"), go_dir],
            envs: env_host_port_pairs(host, port),
        });
    }

    if project_root.join("manage.py").exists() {
        let bind = format!("{host_str}:{}", port.unwrap_or(3415));
        let python = native_python_program(project_root);
        return Ok(DevLaunch {
            program: python,
            args: vec![
                OsString::from("manage.py"),
                OsString::from("runserver"),
                OsString::from(bind),
            ],
            envs: env_host_port_pairs(host, port),
        });
    }

    if project_root.join("main.py").exists() {
        let python = native_python_program(project_root);
        return Ok(DevLaunch {
            program: python,
            args: vec![OsString::from("main.py")],
            envs: env_host_port_pairs(host, port),
        });
    }

    if project_root.join("src/main.py").exists() {
        let python = native_python_program(project_root);
        return Ok(DevLaunch {
            program: python,
            args: vec![OsString::from("-m"), OsString::from("src.main")],
            envs: env_host_port_pairs(host, port),
        });
    }

    if project_root.join("Cargo.toml").exists() && project_root.join("src/main.rs").exists() {
        return Ok(DevLaunch {
            program: PathBuf::from("cargo"),
            args: vec![OsString::from("run")],
            envs: env_host_port_pairs(host, port),
        });
    }

    if project_root.join("artisan").exists() {
        return Ok(DevLaunch {
            program: PathBuf::from("php"),
            args: vec![
                OsString::from("artisan"),
                OsString::from("serve"),
                OsString::from(format!("--host={host_str}")),
                OsString::from(format!("--port={}", port.unwrap_or(8000))),
            ],
            envs: env_host_port_pairs(host, port),
        });
    }

    if project_root.join("composer.json").exists()
        && project_root.join("public/index.php").exists()
        && project_root.join("bin/console").exists()
    {
        return Ok(DevLaunch {
            program: PathBuf::from("php"),
            args: vec![
                OsString::from("-S"),
                OsString::from(format!("{host_str}:{}", port.unwrap_or(8000))),
                OsString::from("-t"),
                OsString::from("public"),
                OsString::from("public/index.php"),
            ],
            envs: env_host_port_pairs(host, port),
        });
    }

    if project_root.join("composer.json").exists() && project_root.join("public/index.php").exists()
    {
        return Ok(DevLaunch {
            program: PathBuf::from("php"),
            args: vec![
                OsString::from("-S"),
                OsString::from(format!("{host_str}:{}", port.unwrap_or(8000))),
                OsString::from("-t"),
                OsString::from("public"),
                OsString::from("public/index.php"),
            ],
            envs: env_host_port_pairs(host, port),
        });
    }

    if project_root.join("pom.xml").exists() {
        let pom = std::fs::read_to_string(project_root.join("pom.xml")).unwrap_or_default();
        if pom.contains("quarkus.platform") || pom.contains("io.quarkus") {
            let mut args = vec![
                OsString::from("quarkus:dev"),
                OsString::from(format!("-Dquarkus.http.host={host_str}")),
                OsString::from("-Dquarkus.analytics.disabled=true"),
            ];
            if let Some(port) = port {
                args.push(OsString::from(format!("-Dquarkus.http.port={port}")));
            }
            return Ok(DevLaunch {
                program: PathBuf::from("mvn"),
                args,
                envs: env_host_port_pairs(host, port),
            });
        }

        let host_arg = format!("--server.address={host_str}");
        let mut args = vec![OsString::from("spring-boot:run")];
        if let Some(port) = port {
            args.push(OsString::from(format!(
                "-Dspring-boot.run.arguments=--server.port={port} {host_arg}"
            )));
        } else {
            args.push(OsString::from(format!(
                "-Dspring-boot.run.arguments={host_arg}"
            )));
        }
        return Ok(DevLaunch {
            program: PathBuf::from("mvn"),
            args,
            envs: env_host_port_pairs(host, port),
        });
    }

    Err(crate::error::web_no_dev_entrypoint(project_root))
}

fn env_host_port_pairs(host: Option<String>, port: Option<u16>) -> Vec<(OsString, OsString)> {
    let mut envs = Vec::new();
    if let Some(host) = host {
        envs.push((OsString::from("HOST"), OsString::from(host)));
    }
    if let Some(port) = port {
        let port = port.to_string();
        envs.push((OsString::from("PORT"), OsString::from(port.clone())));
        envs.push((OsString::from("SERVER_PORT"), OsString::from(port.clone())));
        envs.push((OsString::from("QUARKUS_HTTP_PORT"), OsString::from(port)));
    }
    envs
}

fn native_python_program(project_root: &Path) -> PathBuf {
    let venv_python = native_venv_executable(project_root, "python");
    if venv_python.exists() {
        return venv_python;
    }

    #[cfg(windows)]
    {
        return PathBuf::from("python");
    }

    #[allow(unreachable_code)]
    PathBuf::from("python3")
}

fn native_pip_program(project_root: &Path) -> PathBuf {
    let venv_pip = native_venv_executable(project_root, "pip");
    if venv_pip.exists() {
        return venv_pip;
    }

    #[cfg(windows)]
    {
        return PathBuf::from("pip");
    }

    #[allow(unreachable_code)]
    PathBuf::from("pip3")
}

fn native_venv_executable(project_root: &Path, bin_name: &str) -> PathBuf {
    #[cfg(windows)]
    {
        return project_root
            .join(".venv")
            .join("Scripts")
            .join(format!("{bin_name}.exe"));
    }

    #[allow(unreachable_code)]
    project_root.join(".venv").join("bin").join(bin_name)
}

fn native_install_target(project_root: &Path) -> Result<()> {
    if project_root.join("go.mod").exists() {
        info(&format!(
            "Installing native Go dependencies in {}",
            project_root.display()
        ));
        return run_native_install(project_root, "go", &["mod", "tidy"]);
    }

    if project_root.join("requirements.txt").exists() {
        info(&format!(
            "Installing native Python dependencies in {}",
            project_root.display()
        ));
        run_native_install(project_root, "python3", &["-m", "venv", ".venv"])?;
        return run_native_install(
            project_root,
            &native_pip_program(project_root).to_string_lossy(),
            &["install", "-r", "requirements.txt"],
        );
    }

    if project_root.join("Cargo.toml").exists() {
        info(&format!(
            "Fetching native Rust dependencies in {}",
            project_root.display()
        ));
        return run_native_install(project_root, "cargo", &["fetch"]);
    }

    if project_root.join("pom.xml").exists() {
        info(&format!(
            "Fetching native Maven dependencies in {}",
            project_root.display()
        ));
        return run_native_install(
            project_root,
            "mvn",
            &["-q", "-DskipTests", "dependency:go-offline"],
        );
    }

    if project_root.join("composer.json").exists() || project_root.join("artisan").exists() {
        info(&format!(
            "Installing native PHP dependencies in {}",
            project_root.display()
        ));
        return run_native_install(project_root, "composer", &["install"]);
    }

    Err(crate::error::web_no_install_flow(project_root))
}

fn run_native_install(project_root: &Path, program: &str, args: &[&str]) -> Result<()> {
    let env = native_install_env(project_root, program)?;
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        log_path: Some(project_root.join(".magicore").join("exec.log")),
        clean_env: true,
        env,
        ..Default::default()
    };
    let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    mgc_exec::prelude::run(program, &args, &opts)
        .with_context(|| format!("failed to run native install '{}'", program))?;
    Ok(())
}

fn native_install_env(project_root: &Path, program: &str) -> Result<Vec<(String, String)>> {
    let mut env = Vec::new();
    if program == "go" {
        let go_root = project_root.join(".magicore").join("cache").join("go");
        let mod_cache = go_root.join("pkg").join("mod");
        let build_cache = go_root.join("build");
        std::fs::create_dir_all(&mod_cache)?;
        std::fs::create_dir_all(&build_cache)?;
        env.push(("GOPATH".to_string(), go_root.display().to_string()));
        env.push(("GOMODCACHE".to_string(), mod_cache.display().to_string()));
        env.push(("GOCACHE".to_string(), build_cache.display().to_string()));
    }
    Ok(env)
}

async fn run_single_dev_target(target: &DevTarget) -> Result<()> {
    let launch = build_dev_launch(
        &target.dir,
        target.script_name,
        target.host.clone(),
        target.port,
    )?;

    if launch.program.to_string_lossy().ends_with("vite") {
        info(&format!(
            "🚀 Starting MgDevServer (Native Rust) in {}",
            target.dir.display()
        ));

        let entry = if target.dir.join("src/main.tsx").exists() {
            target.dir.join("src/main.tsx")
        } else if target.dir.join("src/main.ts").exists() {
            target.dir.join("src/main.ts")
        } else {
            target.dir.join("src/index.tsx")
        };

        let config = crate::bundler::dev_server::DevServerConfig {
            root: target.dir.clone(),
            entry,
            host: target
                .host
                .clone()
                .unwrap_or_else(|| "localhost".to_string()),
            port: target.port.unwrap_or(4315),
        };

        let server = crate::bundler::dev_server::MgDevServer::new(config);
        return server.serve().await;
    }

    info(&format!(
        "Starting web dev server in {}",
        target.dir.display()
    ));
    info(&format!("  {}", launch.describe()));
    run_dev_launch_with_guard(target, &launch)
}

async fn run_multi_dev_targets(targets: &[DevTarget]) -> Result<()> {
    let mut children = Vec::new();

    for target in targets {
        let launch = build_dev_launch(
            &target.dir,
            target.script_name,
            target.host.clone(),
            target.port,
        )?;
        if launch.program.to_string_lossy().ends_with("vite") {
            info(&format!(
                "🚀 Starting MgDevServer (Native Rust) for {} in {}",
                target.role,
                target.dir.display()
            ));

            let entry = if target.dir.join("src/main.tsx").exists() {
                target.dir.join("src/main.tsx")
            } else if target.dir.join("src/main.ts").exists() {
                target.dir.join("src/main.ts")
            } else {
                target.dir.join("src/index.tsx")
            };

            let config = crate::bundler::dev_server::DevServerConfig {
                root: target.dir.clone(),
                entry,
                host: target
                    .host
                    .clone()
                    .unwrap_or_else(|| "localhost".to_string()),
                port: target.port.unwrap_or(4315),
            };

            let server = crate::bundler::dev_server::MgDevServer::new(config);
            tokio::spawn(async move {
                if let Err(e) = server.serve().await {
                    tracing::error!("MgDevServer error: {}", e);
                }
            });
            continue;
        }

        info(&format!(
            "Starting {} dev server in {}",
            target.role,
            target.dir.display()
        ));
        info(&format!("  {}", launch.describe()));
        children.push(tokio::spawn({
            let target = target.clone();
            async move { run_dev_launch_with_guard(&target, &launch) }
        }));
    }

    for child in children {
        child.await??;
    }
    Ok(())
}

fn run_dev_launch_with_guard(target: &DevTarget, launch: &DevLaunch) -> Result<()> {
    let local_bin = target.dir.join("node_modules").join(".bin");
    let mut env = vec![(
        "PATH".to_string(),
        prepend_path(&local_bin)?.to_string_lossy().to_string(),
    )];
    env.extend(native_runtime_env(&target.dir, &launch.program)?);
    env.extend(launch.envs.iter().map(|(key, value)| {
        (
            key.to_string_lossy().to_string(),
            value.to_string_lossy().to_string(),
        )
    }));
    if let Some(port) = target.port {
        env.push(("PORT".to_string(), port.to_string()));
    }

    let args = launch
        .args
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    // SAFETY: Enable audit log for dev server execution
    // AN TOÀN: Bật audit log cho dev server execution
    let audit_log = target.dir.join(".mgc").join("exec.log");
    if let Some(parent) = audit_log.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(target.dir.clone()),
        env,
        clean_env: true,
        disable_timeout: true,
        execution_scope: Some(mgc_exec::prelude::ExecutionScope::DevServer),
        log_path: Some(audit_log),
        ..Default::default()
    };

    if launch.program.components().count() > 1 {
        mgc_exec::prelude::run_project_binary_inherited(&launch.program, &args, &opts)
    } else {
        mgc_exec::prelude::run_inherited(&launch.program.to_string_lossy(), &args, &opts)
    }
    .with_context(|| format!("failed to start '{}'", launch.program.to_string_lossy()))?;
    Ok(())
}

fn native_runtime_env(project_root: &Path, program: &Path) -> Result<Vec<(String, String)>> {
    let Some(name) = program.file_name().and_then(|name| name.to_str()) else {
        return Ok(Vec::new());
    };
    native_install_env(project_root, name)
}

fn resolve_local_bin(project_root: &Path, bin_name: &str) -> Result<PathBuf> {
    let bin_dir = project_root.join("node_modules").join(".bin");
    let candidates = local_bin_candidates(&bin_dir, bin_name);
    if let Some(path) = candidates.into_iter().find(|path| path.exists()) {
        return Ok(path);
    }

    Err(crate::error::web_missing_executable(
        bin_name,
        install_hint_command(),
        project_root,
    ))
}

fn local_bin_candidates(bin_dir: &Path, bin_name: &str) -> Vec<PathBuf> {
    let candidates = vec![bin_dir.join(bin_name)];
    #[cfg(windows)]
    {
        let mut candidates = candidates;
        candidates.push(bin_dir.join(format!("{bin_name}.cmd")));
        candidates.push(bin_dir.join(format!("{bin_name}.exe")));
        return candidates;
    }
    candidates
}

fn prepend_path(local_bin: &Path) -> Result<OsString> {
    let mut paths = vec![local_bin.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    Ok(std::env::join_paths(paths)?)
}

// ── Scaffold (create) ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameworkRequest {
    pub raw: String,
    pub normalized: String,
    pub version: Option<String>,
}

pub async fn run_create_with_options(
    framework: &str,
    project_name: &str,
    flags: Option<ScaffoldFlags>,
) -> Result<()> {
    let mut flags = flags.unwrap_or_default();

    if let Some(preset_name) = &flags.preset.clone() {
        apply_preset(preset_name, &mut flags);
    }

    // Phase 4: Parse scaffold spec sớm với typo detection
    use crate::scaffold::spec::{parse_scaffold_spec, CoreKind};
    let spec = parse_scaffold_spec(CoreKind::Web, framework)
        .map_err(|e| anyhow::anyhow!("Invalid framework specification '{}': {}", framework, e))?;

    // Use normalized name cho resolve
    let fe_framework = resolve_framework(Some(&spec.name), &flags)?;
    enforce_framework_language_defaults(&fe_framework, &mut flags);
    validate_flags(&flags, &fe_framework)?;

    info(&format!(
        "Creating new web project '{}' with {}",
        project_name, fe_framework
    ));

    let config = build_web_config(&fe_framework, project_name, &flags)?;

    // Registry-first preflight — ensure only layers this scaffold mode uses.
    // Kiểm đúng layer theo mode, không bắt backend/monorepo partials cho frontend.
    let frontend = parse_framework_request(&fe_framework);
    let rels = required_web_layers_for_config(&config, &frontend);

    // Phase 3: Typed resolution với MissingLayersReport thay vì warning spam
    use crate::scaffold::resolver::MissingLayersReport;
    let mut report = MissingLayersReport::new();

    for rel in &rels {
        match crate::commands::template::ensure_layer(rel).await {
            Ok(status) => {
                if !status.is_available() {
                    report.add_optional(rel.clone());
                }
            }
            Err(_) => {
                if web_layer_has_scaffold_fallback(&config, rel) {
                    report.add_optional(rel.clone());
                } else {
                    report.add_required(rel.clone());
                }
            }
        }
    }

    // Fail early nếu có required layers missing
    if report.has_required_missing() {
        bail!(report.format_error("web", &frontend.normalized));
    }

    let project_dir = crate::scaffold::Scaffolder::scaffold(&config)?;

    let proj_config = mgc_config::project::ProjectConfig::from_scaffold(
        crate::scaffold::Scaffolder::display_name(&project_dir),
        "web",
        &config.sub_type,
        config.frameworks.clone(),
        config.template_dir.to_string_lossy(),
        config.features.clone(),
    );
    proj_config.save(&project_dir)?;

    // Write scaffold provenance (R10 - supply chain tracking)
    let provenance = crate::scaffold::provenance::ScaffoldProvenance::new(
        spec.name.clone(),
        "web".to_string(),
        match &spec.requested_ref {
            crate::scaffold::spec::ScaffoldRef::DistTag(t) => t.clone(),
            crate::scaffold::spec::ScaffoldRef::Version(v) => v.clone(),
            _ => "default".to_string(),
        },
        None, // Registry URL would come from ensure_layer results
        rels.clone(),
    );
    if let Err(e) = provenance.write(&project_dir) {
        mgc_ui::warning(&format!("Failed to write provenance: {}", e));
    }

    let frontend = parse_framework_request(&fe_framework);
    let be_name = detect_backend_framework(&flags);
    let backend = be_name
        .as_deref()
        .map(parse_framework_request)
        .or_else(|| fullstack_backend_framework(&frontend.normalized).map(parse_framework_request));
    enrich_web_project_manifest(&project_dir, &frontend, backend.as_ref(), &flags).await?;

    info(&format!("Project '{}' created!", project_dir.display()));
    info(&format!(
        "  cd {} && {}",
        project_name,
        install_hint_command()
    ));

    Ok(())
}

fn required_web_layers_for_config(
    config: &crate::wizard::engine::ScaffoldConfig,
    frontend: &FrameworkRequest,
) -> Vec<String> {
    let mut rels = vec!["web/shared/partials/base".to_string()];
    let primary = config
        .frameworks
        .first()
        .cloned()
        .unwrap_or_else(|| frontend.normalized.clone());

    match config.sub_type.as_str() {
        "frontend" => {
            rels.push("web/shared/partials/frontend-foundation".to_string());
            rels.push("web/shared/partials/frontend-rust-ready".to_string());
            rels.push("web/shared/partials/frontend".to_string());
            if matches!(primary.as_str(), "react-vite" | "solidjs") {
                rels.push("web/shared/partials/frontend-common".to_string());
            }
            rels.push(format!("web/frontend/{primary}"));
        }
        "backend" => {
            rels.push("web/shared/partials/backend".to_string());
            if let Some(lang) = crate::scaffold::processor::infer_backend_language(&primary) {
                rels.push(format!("web/backend/{lang}/{primary}"));
            }
        }
        "fullstack" => {
            rels.push("web/shared/partials/fullstack".to_string());
            let bucket = if crate::scaffold::processor::is_all_in_one_fullstack(&primary) {
                "all-in-one"
            } else {
                "split"
            };
            rels.push(format!("web/fullstack/{bucket}/{primary}"));
        }
        "monorepo" => {
            rels.push("web/shared/partials/monorepo".to_string());
            rels.push("web/monorepo/base".to_string());
            rels.push(format!("web/monorepo/frontend/{primary}"));
            if let Some(backend) = config.frameworks.get(1) {
                if let Some(lang) = crate::scaffold::processor::infer_backend_language(backend) {
                    rels.push(format!("web/monorepo/backend/{lang}/{backend}"));
                }
            }
        }
        _ => rels.push(format!("web/frontend/{primary}")),
    }

    rels
}

fn web_layer_has_scaffold_fallback(
    config: &crate::wizard::engine::ScaffoldConfig,
    rel: &str,
) -> bool {
    let framework = config.frameworks.first().map(String::as_str).unwrap_or("");
    let frontend_leaf = format!("web/frontend/{framework}");
    if rel == frontend_leaf {
        return crate::scaffold::embedded_kernel::get_embedded_template("web", framework).is_some();
    }

    let backend_leaf = crate::scaffold::processor::infer_backend_language(framework)
        .map(|lang| format!("web/backend/{lang}/{framework}"));
    backend_leaf.as_deref() == Some(rel)
}

fn resolve_framework(pos: Option<&str>, flags: &ScaffoldFlags) -> Result<String> {
    if let Some(fw) = pos {
        return Ok(fw.to_string());
    }
    if flags.react {
        Ok("react".into())
    } else if flags.next {
        Ok("next".into())
    } else if flags.vue {
        Ok("vue".into())
    } else if flags.nuxt {
        Ok("nuxt".into())
    } else if flags.svelte {
        Ok("svelte".into())
    } else if flags.sveltekit {
        Ok("sveltekit".into())
    } else if flags.solid {
        Ok("solid".into())
    } else if flags.astro {
        Ok("astro".into())
    } else if flags.remix {
        Ok("remix".into())
    } else {
        Err(crate::error::no_framework_specified())
    }
}

fn detect_backend_framework(flags: &ScaffoldFlags) -> Option<String> {
    if flags.express {
        Some("express".into())
    } else if flags.fastify {
        Some("fastify".into())
    } else if flags.nestjs {
        Some("nestjs".into())
    } else if flags.hono {
        Some("hono".into())
    } else if flags.koa {
        Some("koa".into())
    } else if flags.trpc {
        Some("trpc".into())
    } else if flags.axum {
        Some("axum".into())
    } else if flags.actix_web {
        Some("actix-web".into())
    } else if flags.gin {
        Some("gin".into())
    } else if flags.echo {
        Some("echo".into())
    } else if flags.fiber {
        Some("fiber".into())
    } else if flags.fastapi {
        Some("fastapi".into())
    } else if flags.django {
        Some("django".into())
    } else if flags.flask {
        Some("flask".into())
    } else if flags.quarkus {
        Some("quarkus".into())
    } else if flags.symfony {
        Some("symfony".into())
    } else {
        None
    }
}

fn validate_flags(flags: &ScaffoldFlags, fe_framework: &str) -> Result<()> {
    if let Some(pm) = flags.pm.as_deref() {
        return Err(crate::error::pm_not_supported(pm));
    }

    if flags.ts && flags.js {
        return Err(crate::error::ts_js_exclusive());
    }

    let fe_count = [
        flags.react,
        flags.next,
        flags.vue,
        flags.nuxt,
        flags.svelte,
        flags.sveltekit,
        flags.solid,
        flags.astro,
        flags.remix,
    ]
    .iter()
    .filter(|&&b| b)
    .count();

    if fe_count > 1 {
        return Err(crate::error::multiple_frameworks());
    }

    if flags.pinia && fe_framework != "vue" && fe_framework != "nuxt" {
        return Err(crate::error::pinia_requires_vue());
    }

    if flags.shadcn && !flags.tailwindcss {
        // auto-enable tailwindcss (side-effect: user sees --tailwindcss in flags)
    }

    Ok(())
}

fn apply_preset(name: &str, flags: &mut ScaffoldFlags) {
    match name {
        "t3" => {
            flags.next = true;
            flags.ts = true;
            flags.tailwindcss = true;
            flags.trpc = true;
            flags.prisma = true;
            flags.zod = true;
            flags.nextauth = true;
        }
        "mern" => {
            flags.react = true;
            flags.express = true;
            flags.mongoose = true;
            flags.mongodb = true;
            flags.zod = true;
            flags.jwt = true;
        }
        "jamstack" => {
            flags.astro = true;
            flags.tailwindcss = true;
            flags.dotenv = true;
            flags.vercel = true;
        }
        "saas" => {
            flags.next = true;
            flags.tailwindcss = true;
            flags.shadcn = true;
            flags.prisma = true;
            flags.postgres = true;
            flags.clerk = true;
            flags.docker = true;
        }
        "mevn" => {
            flags.vue = true;
            flags.express = true;
            flags.mongoose = true;
            flags.mongodb = true;
            flags.zod = true;
        }
        _ => {}
    }
}

fn build_web_config(
    framework: &str,
    project_name: &str,
    flags: &ScaffoldFlags,
) -> Result<crate::wizard::engine::ScaffoldConfig> {
    let frontend = parse_framework_request(framework);
    // Fullstack combos (react-axum, vue-gin, ...) carry the backend in the
    // combined name; check the raw request so normalization to the shared
    // frontend leaf (react-vite) does not hide it.
    let raw_backend = fullstack_backend_framework(&frontend.raw)
        .or_else(|| fullstack_backend_framework(&frontend.normalized))
        .map(str::to_string);

    let mut config = if flags.monorepo {
        match detect_backend_framework(flags).or(raw_backend) {
            Some(be) => {
                let backend = parse_framework_request(&be);
                crate::wizard::engine::ScaffoldConfig {
                    core: "web".to_string(),
                    sub_type: "monorepo".to_string(),
                    frameworks: vec![frontend.normalized.clone(), backend.normalized],
                    project_name: project_name.to_string(),
                    features: vec![],
                    template_dir: std::path::PathBuf::new(),
                }
            }
            None => {
                info("--monorepo ignored: no backend framework specified (add --express, --fastify, etc.)");
                crate::scaffold::Scaffolder::infer_web_create_config(
                    &frontend.normalized,
                    project_name,
                )?
            }
        }
    } else {
        crate::scaffold::Scaffolder::infer_web_create_config(&frontend.normalized, project_name)?
    };

    config.features = web_features(flags);

    if config.frameworks.len() >= 2 {
        let lang = &config.frameworks[0];
        if !config.features.contains(lang) {
            config.features.push(lang.clone());
        }
    }

    Ok(config)
}

fn enforce_framework_language_defaults(framework: &str, flags: &mut ScaffoldFlags) {
    let frontend = parse_framework_request(framework);
    let backend = detect_backend_framework(flags)
        .or_else(|| fullstack_backend_framework(&frontend.normalized).map(str::to_string));

    if frontend.normalized == "nestjs" || backend.as_deref() == Some("nestjs") {
        flags.ts = true;
        flags.js = false;
    }
}

fn web_features(flags: &ScaffoldFlags) -> Vec<String> {
    let mut features = Vec::new();

    // Language
    if flags.ts {
        features.push("typescript".into());
    }
    if flags.js {
        features.push("javascript".into());
    }

    // Styling
    if flags.tailwindcss || flags.shadcn || flags.daisyui {
        features.push("tailwindcss".into());
    }
    if flags.css_modules {
        features.push("css-modules".into());
    }
    if flags.styled_components {
        features.push("styled-components".into());
    }
    if flags.sass {
        features.push("sass".into());
    }
    if flags.unocss {
        features.push("unocss".into());
    }
    if flags.shadcn {
        features.push("shadcn".into());
    }
    if flags.daisyui {
        features.push("daisyui".into());
    }

    // State
    if flags.zustand {
        features.push("zustand".into());
    }
    if flags.redux {
        features.push("redux".into());
    }
    if flags.jotai {
        features.push("jotai".into());
    }
    if flags.recoil {
        features.push("recoil".into());
    }
    if flags.pinia {
        features.push("pinia".into());
    }
    if flags.tanstack_query {
        features.push("tanstack-query".into());
    }

    // Backend
    if flags.express {
        features.push("express".into());
    }
    if flags.fastify {
        features.push("fastify".into());
    }
    if flags.nestjs {
        features.push("nestjs".into());
    }
    if flags.hono {
        features.push("hono".into());
    }
    if flags.koa {
        features.push("koa".into());
    }
    if flags.trpc {
        features.push("trpc".into());
    }

    // Database / ORM
    if flags.prisma {
        features.push("prisma".into());
    }
    if flags.drizzle {
        features.push("drizzle".into());
    }
    if flags.typeorm {
        features.push("typeorm".into());
    }
    if flags.mongoose {
        features.push("mongoose".into());
    }
    if flags.postgres {
        features.push("postgres".into());
    }
    if flags.mysql {
        features.push("mysql".into());
    }
    if flags.sqlite {
        features.push("sqlite".into());
    }
    if flags.mongodb {
        features.push("mongodb".into());
    }

    // Validation
    if flags.zod {
        features.push("zod".into());
    }
    if flags.yup {
        features.push("yup".into());
    }
    if flags.joi {
        features.push("joi".into());
    }
    if flags.valibot {
        features.push("valibot".into());
    }

    // Auth
    if flags.nextauth {
        features.push("nextauth".into());
    }
    if flags.clerk {
        features.push("clerk".into());
    }
    if flags.lucia {
        features.push("lucia".into());
    }
    if flags.jwt {
        features.push("jwt".into());
    }
    if flags.oauth {
        features.push("oauth".into());
    }

    // Testing
    if flags.vitest {
        features.push("vitest".into());
    }
    if flags.jest {
        features.push("jest".into());
    }
    if flags.playwright {
        features.push("playwright".into());
    }
    if flags.cypress {
        features.push("cypress".into());
    }
    if flags.testing_library {
        features.push("testing-library".into());
    }

    // Linting
    if flags.eslint {
        features.push("eslint".into());
    }
    if flags.prettier {
        features.push("prettier".into());
    }
    if flags.biome {
        features.push("biome".into());
    }
    if flags.husky {
        features.push("husky".into());
    }
    if flags.lint_staged {
        features.push("lint-staged".into());
    }
    if flags.commitlint {
        features.push("commitlint".into());
    }

    // Monorepo
    if flags.monorepo {
        features.push("monorepo".into());
    }
    if flags.turborepo {
        features.push("turborepo".into());
    }
    if flags.nx {
        features.push("nx".into());
    }
    if flags.workspaces {
        features.push("workspaces".into());
    }
    if flags.changesets {
        features.push("changesets".into());
    }

    // API
    if flags.rest {
        features.push("rest".into());
    }
    if flags.graphql {
        features.push("graphql".into());
    }
    if flags.trpc_api {
        features.push("trpc-api".into());
    }
    if flags.grpc {
        features.push("grpc".into());
    }

    // Deployment
    if flags.docker {
        features.push("docker".into());
    }
    if flags.github_actions {
        features.push("github-actions".into());
    }
    if flags.vercel {
        features.push("vercel".into());
    }
    if flags.railway {
        features.push("railway".into());
    }
    if flags.fly {
        features.push("fly".into());
    }

    // Misc
    if flags.dotenv {
        features.push("dotenv".into());
    }
    if flags.i18n {
        features.push("i18n".into());
    }
    if flags.pwa {
        features.push("pwa".into());
    }
    if flags.storybook {
        features.push("storybook".into());
    }
    if flags.sentry {
        features.push("sentry".into());
    }
    if flags.analytics {
        features.push("analytics".into());
    }

    // Extra
    for feature in &flags.features {
        if !features.contains(feature) {
            features.push(feature.clone());
        }
    }

    features
}

pub(crate) fn parse_framework_request(input: &str) -> FrameworkRequest {
    let (framework, version) = match input.rsplit_once('@') {
        Some((name, version)) if !name.is_empty() && !version.is_empty() => {
            (name.to_string(), Some(version.to_string()))
        }
        _ => (input.to_string(), None),
    };

    FrameworkRequest {
        raw: input.to_string(),
        normalized: normalize_cli_web_framework(&framework),
        version,
    }
}

fn normalize_cli_web_framework(framework: &str) -> String {
    match framework {
        "react" | "react-app" => "react-vite".to_string(),
        "vue" | "vue-app" => "vue-vite".to_string(),
        "next" | "next-app" => "nextjs".to_string(),
        "svelte" => "sveltekit".to_string(),
        "solid" | "solid-app" => "solidjs".to_string(),
        "angular-app" => "angular".to_string(),
        "qwik-app" => "qwik".to_string(),
        // Fullstack combos with non-Node backends: frontend half resolves to
        // the shared frontend leaf (react-vite / vue-vite / sveltekit).
        "react-axum" | "react-actix-web" | "react-gin" | "react-echo" | "react-fiber"
        | "react-fastapi" | "react-django" | "react-flask" | "react-quarkus" | "react-symfony" => {
            "react-vite".to_string()
        }
        "vue-axum" | "vue-actix-web" | "vue-gin" | "vue-echo" | "vue-fiber" | "vue-fastapi"
        | "vue-django" | "vue-flask" | "vue-quarkus" | "vue-symfony" => "vue-vite".to_string(),
        "svelte-axum" | "svelte-gin" | "svelte-fastapi" | "svelte-quarkus" => {
            "sveltekit".to_string()
        }
        other => other.to_string(),
    }
}

struct WebFrameworkSeed {
    name: &'static str,
    /// Packages that use the user's requested version
    primary: &'static [&'static str],
    /// Packages resolved independently from the registry latest path
    supplemental: &'static [&'static str],
    toolchain: &'static [WebToolchainPackage],
}

struct WebToolchainPackage {
    section: &'static str,
    package: &'static str,
    typescript_only: bool,
    /// If set, use this version instead of fetching from npm registry.
    version: Option<&'static str>,
}

const FRAMEWORK_SEEDS: &[WebFrameworkSeed] = &[
    WebFrameworkSeed {
        name: "vanilla",
        primary: &[],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "solidjs",
        primary: &["solid-js"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite-plugin-solid",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "sveltekit",
        primary: &["@sveltejs/kit"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@sveltejs/vite-plugin-svelte",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@sveltejs/adapter-auto",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "svelte",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: Some("^6.0.3"),
            },
        ],
    },
    WebFrameworkSeed {
        name: "react-vite",
        primary: &["react", "react-dom"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@vitejs/plugin-react",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/react",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/react-dom",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "nextjs",
        primary: &["next"],
        supplemental: &["react", "react-dom"],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/node",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/react",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/react-dom",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "fastify",
        primary: &["fastify"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/node",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "tsx",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "vue-vite",
        primary: &["vue"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@vitejs/plugin-vue",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "nuxt",
        primary: &["nuxt"],
        supplemental: &["vue"],
        toolchain: &[WebToolchainPackage {
            section: "devDependencies",
            package: "typescript",
            typescript_only: true,
            version: None,
        }],
    },
    WebFrameworkSeed {
        name: "angular",
        primary: &[
            "@angular/core",
            "@angular/compiler",
            "@angular/common",
            "@angular/platform-browser",
            "@angular/platform-browser-dynamic",
            "@angular/router",
        ],
        supplemental: &["rxjs", "zone.js", "tslib"],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "@angular/cli",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@angular/compiler-cli",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@angular-devkit/build-angular",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "qwik",
        primary: &["@builder.io/qwik"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "@builder.io/qwik-city",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite",
                typescript_only: false,
                version: Some("^7.3.6"),
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "astro",
        primary: &["astro"],
        supplemental: &[],
        toolchain: &[WebToolchainPackage {
            section: "devDependencies",
            package: "typescript",
            typescript_only: true,
            version: None,
        }],
    },
    WebFrameworkSeed {
        name: "express",
        primary: &["express"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/express",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/node",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "tsx",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "hono",
        primary: &["hono"],
        supplemental: &["@hono/node-server"],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/node",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "tsx",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "nestjs",
        primary: &["@nestjs/core", "@nestjs/common", "@nestjs/platform-express"],
        supplemental: &["reflect-metadata", "rxjs"],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/node",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "tsx",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "trpc",
        primary: &["@trpc/server"],
        supplemental: &["express", "zod"],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/express",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/node",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "tsx",
                typescript_only: true,
                version: None,
            },
        ],
    },
];

struct WebFeaturePackage {
    feature: &'static str,
    section: &'static str,
    package: &'static str,
}

const FEATURE_PACKAGES: &[WebFeaturePackage] = &[
    WebFeaturePackage {
        feature: "prisma",
        section: "dependencies",
        package: "@prisma/client",
    },
    WebFeaturePackage {
        feature: "prisma",
        section: "devDependencies",
        package: "prisma",
    },
    WebFeaturePackage {
        feature: "vitest",
        section: "devDependencies",
        package: "vitest",
    },
    WebFeaturePackage {
        feature: "eslint",
        section: "devDependencies",
        package: "eslint",
    },
    WebFeaturePackage {
        feature: "eslint",
        section: "devDependencies",
        package: "eslint-config-next",
    },
    WebFeaturePackage {
        feature: "prettier",
        section: "devDependencies",
        package: "prettier",
    },
    WebFeaturePackage {
        feature: "tailwindcss",
        section: "devDependencies",
        package: "@tailwindcss/postcss",
    },
    WebFeaturePackage {
        feature: "tailwindcss",
        section: "devDependencies",
        package: "@tailwindcss/vite",
    },
    WebFeaturePackage {
        feature: "postgres",
        section: "dependencies",
        package: "pg",
    },
    WebFeaturePackage {
        feature: "zustand",
        section: "dependencies",
        package: "zustand",
    },
    WebFeaturePackage {
        feature: "tanstack-query",
        section: "dependencies",
        package: "@tanstack/react-query",
    },
    WebFeaturePackage {
        feature: "zod",
        section: "dependencies",
        package: "zod",
    },
    WebFeaturePackage {
        feature: "shadcn",
        section: "dependencies",
        package: "clsx",
    },
    WebFeaturePackage {
        feature: "shadcn",
        section: "dependencies",
        package: "tailwind-merge",
    },
    WebFeaturePackage {
        feature: "shadcn",
        section: "dependencies",
        package: "class-variance-authority",
    },
    WebFeaturePackage {
        feature: "nextauth",
        section: "dependencies",
        package: "next-auth",
    },
    WebFeaturePackage {
        feature: "playwright",
        section: "devDependencies",
        package: "@playwright/test",
    },
    WebFeaturePackage {
        feature: "husky",
        section: "devDependencies",
        package: "husky",
    },
    WebFeaturePackage {
        feature: "lint-staged",
        section: "devDependencies",
        package: "lint-staged",
    },
    WebFeaturePackage {
        feature: "biome",
        section: "devDependencies",
        package: "@biomejs/biome",
    },
    WebFeaturePackage {
        feature: "sass",
        section: "devDependencies",
        package: "sass",
    },
    WebFeaturePackage {
        feature: "unocss",
        section: "devDependencies",
        package: "unocss",
    },
    WebFeaturePackage {
        feature: "daisyui",
        section: "devDependencies",
        package: "daisyui",
    },
    WebFeaturePackage {
        feature: "redux",
        section: "dependencies",
        package: "@reduxjs/toolkit",
    },
    WebFeaturePackage {
        feature: "redux",
        section: "dependencies",
        package: "react-redux",
    },
    WebFeaturePackage {
        feature: "jest",
        section: "devDependencies",
        package: "jest",
    },
    WebFeaturePackage {
        feature: "jest",
        section: "devDependencies",
        package: "@testing-library/react",
    },
    WebFeaturePackage {
        feature: "jest",
        section: "devDependencies",
        package: "@testing-library/jest-dom",
    },
    WebFeaturePackage {
        feature: "jest",
        section: "devDependencies",
        package: "jest-environment-jsdom",
    },
    WebFeaturePackage {
        feature: "cypress",
        section: "devDependencies",
        package: "cypress",
    },
    WebFeaturePackage {
        feature: "drizzle",
        section: "dependencies",
        package: "drizzle-orm",
    },
    WebFeaturePackage {
        feature: "drizzle",
        section: "devDependencies",
        package: "drizzle-kit",
    },
    WebFeaturePackage {
        feature: "biome",
        section: "devDependencies",
        package: "@biomejs/biome",
    },
    WebFeaturePackage {
        feature: "sass",
        section: "devDependencies",
        package: "sass",
    },
    WebFeaturePackage {
        feature: "unocss",
        section: "devDependencies",
        package: "unocss",
    },
    WebFeaturePackage {
        feature: "daisyui",
        section: "devDependencies",
        package: "daisyui",
    },
    WebFeaturePackage {
        feature: "redux",
        section: "dependencies",
        package: "@reduxjs/toolkit",
    },
    WebFeaturePackage {
        feature: "redux",
        section: "dependencies",
        package: "react-redux",
    },
    WebFeaturePackage {
        feature: "jest",
        section: "devDependencies",
        package: "jest",
    },
    WebFeaturePackage {
        feature: "jest",
        section: "devDependencies",
        package: "@testing-library/react",
    },
    WebFeaturePackage {
        feature: "jest",
        section: "devDependencies",
        package: "@testing-library/jest-dom",
    },
    WebFeaturePackage {
        feature: "jest",
        section: "devDependencies",
        package: "jest-environment-jsdom",
    },
    WebFeaturePackage {
        feature: "cypress",
        section: "devDependencies",
        package: "cypress",
    },
    WebFeaturePackage {
        feature: "drizzle",
        section: "dependencies",
        package: "drizzle-orm",
    },
    WebFeaturePackage {
        feature: "drizzle",
        section: "devDependencies",
        package: "drizzle-kit",
    },
    WebFeaturePackage {
        feature: "clerk",
        section: "dependencies",
        package: "@clerk/nextjs",
    },
    WebFeaturePackage {
        feature: "sass",
        section: "devDependencies",
        package: "sass",
    },
    WebFeaturePackage {
        feature: "styled-components",
        section: "dependencies",
        package: "styled-components",
    },
    WebFeaturePackage {
        feature: "styled-components",
        section: "devDependencies",
        package: "@types/styled-components",
    },
    WebFeaturePackage {
        feature: "commitlint",
        section: "devDependencies",
        package: "@commitlint/cli",
    },
    WebFeaturePackage {
        feature: "commitlint",
        section: "devDependencies",
        package: "@commitlint/config-conventional",
    },
    WebFeaturePackage {
        feature: "rest",
        section: "dependencies",
        package: "express",
    },
    WebFeaturePackage {
        feature: "graphql",
        section: "dependencies",
        package: "@apollo/server",
    },
    WebFeaturePackage {
        feature: "graphql",
        section: "dependencies",
        package: "graphql",
    },
    WebFeaturePackage {
        feature: "graphql",
        section: "dependencies",
        package: "@as-integrations/next",
    },
    WebFeaturePackage {
        feature: "trpc",
        section: "dependencies",
        package: "@trpc/server",
    },
    WebFeaturePackage {
        feature: "storybook",
        section: "devDependencies",
        package: "@storybook/react",
    },
    WebFeaturePackage {
        feature: "storybook",
        section: "devDependencies",
        package: "@storybook/addon-essentials",
    },
    WebFeaturePackage {
        feature: "i18n",
        section: "devDependencies",
        package: "i18next",
    },
    WebFeaturePackage {
        feature: "i18n",
        section: "devDependencies",
        package: "react-i18next",
    },
    WebFeaturePackage {
        feature: "i18n",
        section: "devDependencies",
        package: "i18next-browser-languagedetector",
    },
    WebFeaturePackage {
        feature: "i18n",
        section: "devDependencies",
        package: "i18next-http-backend",
    },
    WebFeaturePackage {
        feature: "fastify",
        section: "dependencies",
        package: "fastify",
    },
    WebFeaturePackage {
        feature: "nestjs",
        section: "dependencies",
        package: "@nestjs/core",
    },
    WebFeaturePackage {
        feature: "nestjs",
        section: "dependencies",
        package: "@nestjs/common",
    },
    WebFeaturePackage {
        feature: "nestjs",
        section: "dependencies",
        package: "@nestjs/platform-express",
    },
    WebFeaturePackage {
        feature: "hono",
        section: "dependencies",
        package: "hono",
    },
    WebFeaturePackage {
        feature: "koa",
        section: "dependencies",
        package: "koa",
    },
    WebFeaturePackage {
        feature: "koa",
        section: "dependencies",
        package: "@koa/router",
    },
    WebFeaturePackage {
        feature: "typeorm",
        section: "dependencies",
        package: "typeorm",
    },
    WebFeaturePackage {
        feature: "typeorm",
        section: "devDependencies",
        package: "@types/typeorm",
    },
    WebFeaturePackage {
        feature: "mongoose",
        section: "dependencies",
        package: "mongoose",
    },
    WebFeaturePackage {
        feature: "mysql",
        section: "dependencies",
        package: "mysql2",
    },
    WebFeaturePackage {
        feature: "sqlite",
        section: "dependencies",
        package: "better-sqlite3",
    },
    WebFeaturePackage {
        feature: "mongodb",
        section: "dependencies",
        package: "mongodb",
    },
    WebFeaturePackage {
        feature: "yup",
        section: "dependencies",
        package: "yup",
    },
    WebFeaturePackage {
        feature: "joi",
        section: "dependencies",
        package: "joi",
    },
    WebFeaturePackage {
        feature: "valibot",
        section: "dependencies",
        package: "valibot",
    },
    WebFeaturePackage {
        feature: "lucia",
        section: "dependencies",
        package: "lucia",
    },
    WebFeaturePackage {
        feature: "jwt",
        section: "dependencies",
        package: "jsonwebtoken",
    },
    WebFeaturePackage {
        feature: "jwt",
        section: "devDependencies",
        package: "@types/jsonwebtoken",
    },
    WebFeaturePackage {
        feature: "oauth",
        section: "dependencies",
        package: "oauth",
    },
    WebFeaturePackage {
        feature: "testing-library",
        section: "devDependencies",
        package: "@testing-library/react",
    },
    WebFeaturePackage {
        feature: "testing-library",
        section: "devDependencies",
        package: "@testing-library/jest-dom",
    },
    WebFeaturePackage {
        feature: "turborepo",
        section: "devDependencies",
        package: "turbo",
    },
    WebFeaturePackage {
        feature: "nx",
        section: "devDependencies",
        package: "nx",
    },
    WebFeaturePackage {
        feature: "changesets",
        section: "devDependencies",
        package: "@changesets/cli",
    },
    WebFeaturePackage {
        feature: "grpc",
        section: "dependencies",
        package: "@grpc/grpc-js",
    },
    WebFeaturePackage {
        feature: "grpc",
        section: "dependencies",
        package: "@grpc/proto-loader",
    },
    WebFeaturePackage {
        feature: "pwa",
        section: "devDependencies",
        package: "workbox-webpack-plugin",
    },
    WebFeaturePackage {
        feature: "sentry",
        section: "dependencies",
        package: "@sentry/node",
    },
    WebFeaturePackage {
        feature: "sentry",
        section: "dependencies",
        package: "@sentry/react",
    },
    WebFeaturePackage {
        feature: "analytics",
        section: "dependencies",
        package: "@vercel/analytics",
    },
];

fn framework_primary_package(framework: &str) -> Option<String> {
    let seed_name = resolve_seed_name(framework);
    FRAMEWORK_SEEDS
        .iter()
        .find(|s| s.name == seed_name)
        .and_then(|s| s.primary.first())
        .copied()
        .map(str::to_string)
}

async fn fetch_npm_latest_version(package: &str) -> Result<String> {
    if let Some(version) = scaffold_version_override(package) {
        return Ok(version);
    }
    match fetch_npm_latest_version_from_registry(DEFAULT_NPM_REGISTRY, package).await {
        Ok(version) => Ok(version),
        Err(error) => {
            if let Some(version) = scaffold_baseline_version(package) {
                return Ok(version.to_string());
            }
            eprintln!(
                "warning: could not resolve version for '{package}' ({}); using 'latest'",
                error
            );
            Ok("latest".to_string())
        }
    }
}

fn global_cli_http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .pool_max_idle_per_host(50)
            .pool_idle_timeout(std::time::Duration::from_secs(120))
            .tcp_keepalive(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(60))
            .user_agent(format!("MagiCore/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build HTTP client")
    })
}

async fn fetch_npm_latest_version_from_registry(
    registry_url: &str,
    package: &str,
) -> Result<String> {
    let url = format!("{}/{package}/latest", registry_url.trim_end_matches('/'));
    let resp = global_cli_http_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| crate::error::network_error_fetching(package, &e))?;
    if !resp.status().is_success() {
        return Err(crate::error::npm_registry_status(package, &resp.status()));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| crate::error::bad_npm_response(package, &e))?;
    parse_latest_version_response(package, &body)
}

fn parse_latest_version_response(package: &str, body: &serde_json::Value) -> Result<String> {
    body["version"]
        .as_str()
        .map(|s| format!("^{}", s))
        .ok_or_else(|| crate::error::no_version_field(package))
}

fn scaffold_version_override(package: &str) -> Option<String> {
    std::env::var(SCAFFOLD_VERSION_OVERRIDES_ENV)
        .ok()
        .and_then(|raw| {
            raw.split(',')
                .filter_map(|entry| entry.trim().split_once('='))
                .find_map(|(name, version)| {
                    (name.trim() == package && !version.trim().is_empty())
                        .then(|| version.trim().to_string())
                })
        })
}

fn fullstack_backend_framework(framework: &str) -> Option<&'static str> {
    // Explicit combos (legacy + Rust/Go/Python/Java/PHP backends).
    // Backend name is the template folder under templates/web/backend/<lang>/.
    match framework {
        "react-fastify" => Some("fastify"),
        "react-spring" => Some("spring-boot"),
        "vue-laravel" => Some("laravel"),
        "react-express" => Some("express"),
        "react-hono" => Some("hono"),
        "react-nestjs" => Some("nestjs"),
        "react-trpc" => Some("trpc"),
        "react-axum" => Some("axum"),
        "react-actix-web" => Some("actix-web"),
        "react-gin" => Some("gin"),
        "react-echo" => Some("echo"),
        "react-fiber" => Some("fiber"),
        "react-fastapi" => Some("fastapi"),
        "react-django" => Some("django"),
        "react-flask" => Some("flask"),
        "react-quarkus" => Some("quarkus"),
        "react-symfony" => Some("symfony"),
        "vue-express" => Some("express"),
        "vue-hono" => Some("hono"),
        "vue-nestjs" => Some("nestjs"),
        "vue-axum" => Some("axum"),
        "vue-actix-web" => Some("actix-web"),
        "vue-gin" => Some("gin"),
        "vue-echo" => Some("echo"),
        "vue-fiber" => Some("fiber"),
        "vue-fastapi" => Some("fastapi"),
        "vue-django" => Some("django"),
        "vue-flask" => Some("flask"),
        "vue-quarkus" => Some("quarkus"),
        "vue-symfony" => Some("symfony"),
        "svelte-express" => Some("express"),
        "svelte-hono" => Some("hono"),
        "svelte-axum" => Some("axum"),
        "svelte-fastapi" => Some("fastapi"),
        "svelte-gin" => Some("gin"),
        "svelte-quarkus" => Some("quarkus"),
        "next" | "nextjs" => None, // Next.js has built-in API routes
        "nuxt" | "nuxtjs" => Some("hono"),
        _ => None,
    }
}

fn resolve_seed_name(framework: &str) -> &str {
    match framework {
        "react-fastify" | "react-spring" | "react-express" | "react-hono" | "react-nestjs"
        | "react-trpc" => "react-vite",
        "vue-laravel" | "vue-express" | "vue-hono" | "vue-nestjs" => "vue-vite",
        "svelte-express" | "svelte-hono" => "sveltekit",
        _ => framework,
    }
}

#[derive(Debug, Deserialize)]
struct ScaffoldBaselineVersions {
    versions: HashMap<String, String>,
}

fn scaffold_baseline_versions() -> &'static HashMap<String, String> {
    static VERSIONS: std::sync::OnceLock<HashMap<String, String>> = std::sync::OnceLock::new();
    VERSIONS.get_or_init(|| {
        toml::from_str::<ScaffoldBaselineVersions>(SCAFFOLD_BASELINE_VERSIONS_TOML)
            .expect("templates/web/versions/scaffold-baseline.toml must be valid")
            .versions
    })
}

fn scaffold_baseline_version(package: &str) -> Option<&'static str> {
    scaffold_baseline_versions()
        .get(package)
        .map(String::as_str)
}

async fn resolve_primary_version(request: &FrameworkRequest) -> Result<String> {
    match request.version.as_deref() {
        Some("latest") | None => {
            let pkg = framework_primary_package(&request.normalized)
                .ok_or_else(|| crate::error::no_primary_package(&request.normalized))?;
            fetch_npm_latest_version(&pkg).await
        }
        Some(version) => Ok(version.to_string()),
    }
}

fn ensure_package(root: &mut Map<String, Value>, section: &str, package: &str, version: &str) {
    let entry = root
        .entry(section.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(map) = entry {
        map.insert(package.to_string(), Value::String(version.to_string()));
    }
}

async fn apply_web_manifest_seed(
    package_json_path: &Path,
    request: &FrameworkRequest,
    flags: &ScaffoldFlags,
) -> Result<()> {
    if !package_json_path.exists() {
        return Ok(());
    }
    let mut value: Value = serde_json::from_str(&std::fs::read_to_string(package_json_path)?)?;
    let object = value
        .as_object_mut()
        .ok_or_else(crate::error::package_json_root_object)?;

    let seed_name = resolve_seed_name(&request.normalized);
    if let Some(seed) = FRAMEWORK_SEEDS.iter().find(|s| s.name == seed_name) {
        if !seed.primary.is_empty() {
            let primary = resolve_primary_version(request).await?;
            for &package in seed.primary {
                ensure_package(object, "dependencies", package, &primary);
            }
        }
        for &package in seed.supplemental {
            let version = scaffold_baseline_version(package)
                .map(str::to_string)
                .unwrap_or(fetch_npm_latest_version(package).await?);
            ensure_package(object, "dependencies", package, &version);
        }
        for tool in seed.toolchain {
            if tool.typescript_only && !flags.ts {
                continue;
            }
            let version = match tool.version {
                Some(v) => v.to_string(),
                None => scaffold_baseline_version(tool.package)
                    .map(str::to_string)
                    .unwrap_or(fetch_npm_latest_version(tool.package).await?),
            };
            ensure_package(object, tool.section, tool.package, &version);
        }
    }

    if flags.tailwindcss || flags.shadcn || flags.daisyui {
        let version = fetch_npm_latest_version("tailwindcss").await?;
        ensure_package(object, "devDependencies", "tailwindcss", &version);
    }

    for feat_pkg in FEATURE_PACKAGES {
        let is_active = match feat_pkg.feature {
            "prisma" => flags.prisma,
            "vitest" => flags.vitest,
            "eslint" => flags.eslint,
            "prettier" => flags.prettier,
            "tailwindcss" => flags.tailwindcss,
            "postgres" => flags.postgres,
            "zustand" => flags.zustand,
            "tanstack-query" => flags.tanstack_query,
            "zod" => flags.zod,
            "shadcn" => flags.shadcn,
            "nextauth" => flags.nextauth,
            "playwright" => flags.playwright,
            "husky" => flags.husky,
            "lint-staged" => flags.lint_staged,
            "biome" => flags.biome,
            "sass" => flags.sass,
            "unocss" => flags.unocss,
            "daisyui" => flags.daisyui,
            "redux" => flags.redux,
            "jest" => flags.jest,
            "cypress" => flags.cypress,
            "drizzle" => flags.drizzle,
            "clerk" => flags.clerk,
            "styled-components" => flags.styled_components,
            "commitlint" => flags.commitlint,
            "rest" => flags.rest,
            "graphql" => flags.graphql,
            "trpc" => flags.trpc,
            "grpc" => flags.grpc,
            "lucia" => flags.lucia,
            "jwt" => flags.jwt,
            "oauth" => flags.oauth,
            "dotenv" => flags.dotenv,
            "i18n" => flags.i18n,
            "pwa" => flags.pwa,
            "storybook" => flags.storybook,
            "sentry" => flags.sentry,
            "analytics" => flags.analytics,
            "railway" => flags.railway,
            "fly" => flags.fly,
            _ => false,
        };
        if is_active {
            let version = scaffold_baseline_version(feat_pkg.package)
                .map(str::to_string)
                .unwrap_or(fetch_npm_latest_version(feat_pkg.package).await?);
            ensure_package(object, feat_pkg.section, feat_pkg.package, &version);
        }
    }

    std::fs::write(package_json_path, serde_json::to_string_pretty(&value)?)?;
    Ok(())
}

pub(crate) async fn enrich_web_project_manifest(
    project_dir: &Path,
    frontend: &FrameworkRequest,
    backend: Option<&FrameworkRequest>,
    flags: &ScaffoldFlags,
) -> Result<()> {
    if flags.monorepo {
        apply_web_manifest_seed(
            &project_dir
                .join("apps")
                .join("frontend")
                .join("package.json"),
            frontend,
            flags,
        )
        .await?;
        if let Some(backend) = backend {
            apply_web_manifest_seed(
                &project_dir
                    .join("apps")
                    .join("backend")
                    .join("package.json"),
                backend,
                flags,
            )
            .await?;
        }
        return Ok(());
    }
    if let Some(backend) = backend {
        apply_web_manifest_seed(&project_dir.join("package.json"), frontend, flags).await?;
        apply_web_manifest_seed(
            &project_dir.join("server").join("package.json"),
            backend,
            flags,
        )
        .await
    } else {
        apply_web_manifest_seed(&project_dir.join("package.json"), frontend, flags).await
    }
}

#[cfg(test)]
#[path = "test/web.rs"]
mod tests;
