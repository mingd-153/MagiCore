use crate::context::ProjectContext;
use anyhow::{Result, bail};
use mgc_cache::PackageCache;
use mgc_lockfile::Lockfile;
use mgc_types::adapter::{AddOptions, PreparedAdd};
use mgc_types::{DependencySpec, Manifest, PackageId, ResolvedGraph, ResolvedPackage};
use mgc_ui::{
    add_multi_bar, create_multi_progress, create_progress_bar, create_spinner, info, style_cmd,
    success,
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
    frozen: bool,  // Frozen mode: fail if lockfile needs update (CI mode)
) -> Result<()> {
    // T4.1: Offline mode — thread env flag; adapters read MGC_OFFLINE_MODE
    // (web resolve gate) and receive InstallOptions.offline (install gate).
    if offline {
        crate::offline::set_offline_mode(true);
    }

    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();

    // Pre-install hooks (mgc.hooks.toml [hooks.pre-install]) — fail → abort install
    // (21 §9 fail-closed; hooks cannot bypass security checks).
    crate::commands::hooks::run_event(ctx.root(), "pre-install")?;

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
                        frozen, // frozen mode
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
        frozen,  // frozen mode
    )
    .await
}

/// Cloud branch of the adapter-path firewall (T0.3-clo-gap): mirrors
/// the `clo` CLI lanes exactly — terraform gates as delegated, every
/// other detected type rides the native web engine inside the adapter.
/// An undetected type fails closed (never assumed native).
/// (Nhánh cloud của tường lửa adapter-path: terraform gate delegate,
/// type khác đi engine web native; type không nhận diện được thì
/// fail-closed.)
#[cfg(feature = "clo")]
fn clo_adapter_path_gate(project_root: &Path) -> Result<()> {
    use mgc_cloud_adapter::CloudType;
    match mgc_cloud_adapter::detect_type(project_root) {
        Some(CloudType::Terraform) => {
            let compat = crate::commands::dep_gate::from_dep_flag(None)?;
            crate::commands::dep_gate::gate(
                &crate::commands::dep_gate::DepContext::new(
                    "clo",
                    Some(crate::commands::dep_gate::eco::TERRAFORM),
                    None,
                    None,
                    crate::commands::dep_gate::DepOp::Install,
                ),
                Some("terraform"),
                &compat,
                Some(&project_root.join(".magicore").join("exec.log")),
            )
        }
        Some(_) => Ok(()),
        None => Err(crate::error::detect_core_failed("clo")),
    }
}

/// No cloud support compiled in — a cloud adapter reaching this path is
/// a build-configuration error, failed closed.
/// (Không biên dịch hỗ trợ cloud — adapter cloud tới được đây là lỗi
/// cấu hình build, fail-closed.)
#[cfg(not(feature = "clo"))]
fn clo_adapter_path_gate(project_root: &Path) -> Result<()> {
    let _ = project_root;
    Err(crate::error::core_not_in_build("clo"))
}

async fn install_into_root(
    adapter: &dyn mgc_types::adapter::PackageAdapter,
    project_root: &Path,
    packages: &[String],
    ignore_scripts: bool,
    allow_scripts: bool,
    offline: bool, // T4.1: offline mode
    frozen: bool,  // Frozen mode: fail if lockfile needs update
) -> Result<()> {
    // T4.1: Offline mode validation
    if offline {
        // R2.1 FIX (AUDIT VÒNG 2): Atomic check-and-load (no TOCTOU)
        info("🔒 Offline mode enabled");
        debug_assert!(crate::offline::is_offline_mode());

        // Try load lockfile immediately (check = use, atomic)
        let lockfile_path = project_root.join("mgc.lock");
        if !lockfile_path.exists() {
            anyhow::bail!(
                "Offline mode requires mgc.lock\n  \
                 Run 'mgc install' online first to create lockfile"
            );
        }

        // T4.5: Verify lockfile integrity BEFORE using cache
        crate::commands::trust::policy::enforce_project_policy(&lockfile_path)?;
        let status = crate::commands::trust::policy::verify_project_lockfile(&lockfile_path)?;
        match status {
            mgc_lockfile::VerificationStatus::Tampered(msg) => {
                // T4.5: Invalidate cache on tamper detection
                info("WARN: Lockfile tampered — invalidating cache");
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
                info("WARN: Lockfile not signed — run 'mgc trust sign' for tamper detection");
            }
            mgc_lockfile::VerificationStatus::Valid => {
                info("✓ Lockfile signature valid");
            }
            mgc_lockfile::VerificationStatus::UntrustedKey(key_id) => {
                info(&format!(
                    "WARN: Lockfile signer '{key_id}' is not trusted by this project"
                ));
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

    // Mutation gateway (P0-1): the generic path mutates the manifest
    // below (packages loop + write) and MUST NOT bypass lock, recovery,
    // journal and rollback like the per-core lanes get. MCP, benchmark,
    // workspace and monorepo installs all funnel through here.
    // (Install generic qua gateway — hết bypass.)
    let write_lock = crate::commands::core::shared::begin_dependency_mutation(
        adapter,
        project_root,
        crate::commands::core::shared::MutationOperation::Install,
    )
    .await?;

    if !packages.is_empty() {
        mgc_lockfile::ensure_lockfile_mutation_allowed(&project_root.join("mgc.lock"))?;
    }

    // Ownership preflight FIRST (P0-1): unauthorized ops fail here,
    // before parse/prepare_add (network) or any manifest write.
    // (Gate trước mọi side effect.)
    validate_install_owner(adapter, project_root)?;

    // Toolchain-owned manifests with package arguments fail BEFORE any
    // adapter call (P0-1): mgc cannot journal what it does not own, and
    // the provider toolchain must run explicitly (add-lib lane or the
    // tool itself) — never a silent pre-gate write without rollback.
    // (Manifest của tool + packages → lỗi trước khi gọi adapter.)
    reject_toolchain_owned_packages(adapter, packages)?;

    let add_cmd = match adapter.name() {
        "web" => "mgc add".to_string(),
        other => format!("mgc add-{other}"),
    };

    let spinner = create_spinner("  Reading project manifest...");
    let mut manifest = adapter.parse_manifest(project_root).await?;
    spinner.finish_and_clear();

    // Pre-image for the install journal (P0-1): staged only when mgc
    // actually rewrites an owned manifest below.
    // (Snapshot pre-image cho journal install.)
    let install_snapshot = if !packages.is_empty() && adapter.manifest_owned() {
        Some(crate::commands::core::shared::MutationSnapshot::capture(
            &manifest,
            project_root,
            &write_lock,
            crate::commands::core::shared::MutationOperation::Install,
        )?)
    } else {
        None
    };
    let mut install_journaled = false;

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

        // Journaled write (P0-1): same transaction protocol as the
        // per-core lanes — stage, write, post-image, all rollback-routed.
        // Unjournaled (toolchain-owned manifest) keeps legacy behavior.
        // (Ghi có journal như lane per-core.)
        if let Some(snapshot) = install_snapshot.as_ref() {
            use crate::commands::core::shared as core_shared;
            core_shared::stage_mutation_journal(
                project_root,
                adapter,
                packages,
                snapshot,
                &write_lock,
            )?;
            core_shared::journaled_step(
                adapter,
                project_root,
                snapshot,
                &write_lock,
                "before-manifest-write",
                adapter.write_manifest(project_root, &manifest),
                "after-manifest-write",
            )
            .await?;
            core_shared::journaled_step(
                adapter,
                project_root,
                snapshot,
                &write_lock,
                "before-post-image",
                async { core_shared::record_post_image(project_root, &manifest, &write_lock) },
                "after-post-image",
            )
            .await?;
            install_journaled = true;
        } else {
            adapter.write_manifest(project_root, &manifest).await?;
        }
    }

    let all_deps: Vec<_> = manifest.all_dependencies().collect();
    if all_deps.is_empty() {
        info("No dependencies to install.");
        info(&format!(
            "Use '{} <package>' to add dependencies.",
            style_cmd(&add_cmd)
        ));
        if install_journaled {
            crate::commands::core::shared::finish_mutation_journal(project_root, &write_lock)?;
        }
        return Ok(());
    }

    // Resolve + materialize + hooks under the result boundary (P0-1):
    // every tail error rolls back to the pre-image when journaled.
    // (Tail qua rollback khi có journal.)
    match install_resolve_and_materialize(
        adapter,
        project_root,
        &manifest,
        frozen,
        ignore_scripts,
        allow_scripts,
        offline,
    )
    .await
    {
        Ok(()) => {
            if install_journaled {
                // P0-2: a disarm failure here rolls back like every other
                // op — succeeding with a live in_progress journal would
                // let the next run undo a finished install (half-updated
                // state by another name).
                // (Disarm lỗi thì rollback — không để journal sống.)
                if let Err(e) = crate::commands::core::shared::finish_mutation_journal(
                    project_root,
                    &write_lock,
                ) {
                    if let Some(snapshot) = install_snapshot.as_ref() {
                        return crate::commands::core::shared::rollback_mutation(
                            adapter,
                            project_root,
                            snapshot,
                            e,
                            &write_lock,
                        )
                        .await;
                    }
                    return Err(e);
                }
            }
            Ok(())
        }
        Err(e) => {
            if let Some(snapshot) = install_snapshot.as_ref() {
                return crate::commands::core::shared::rollback_mutation(
                    adapter,
                    project_root,
                    snapshot,
                    e,
                    &write_lock,
                )
                .await;
            }
            Err(e)
        }
    }
}

/// Reject package arguments on toolchain-owned manifests (pure,
/// unit-tested): mgc cannot journal what it does not own.
/// (Từ chối packages trên manifest của tool — hàm thuần.)
fn reject_toolchain_owned_packages(
    adapter: &dyn mgc_types::adapter::PackageAdapter,
    packages: &[String],
) -> Result<()> {
    if !packages.is_empty() && !adapter.manifest_owned() {
        return Err(crate::error::install_packages_toolchain_owned(
            adapter.name(),
        ));
    }
    Ok(())
}

/// Ownership preflight (P0-1): the capability/ownership gate MUST run
/// BEFORE any side effect (parse is read-only; prepare_add resolves
/// over the network; the manifest write mutates). An unauthorized op
/// must fail here — never after resolving, writing, or spawning.
/// Pure function of (adapter, root): no mutation, no network.
/// (Gate ownership chạy TRƯỚC mọi side effect — không resolve/write.)
fn validate_install_owner(
    adapter: &dyn mgc_types::adapter::PackageAdapter,
    project_root: &Path,
) -> Result<()> {
    // C0 ownership firewall (T0.3): the adapter path serves MCP +
    // workspace/monorepo installs — it passes the same gate as the
    // per-core lanes (tool set comes from the owner table: adapter-routed
    // lanes cannot name their exact tool here).
    // Cloud branch (T0.3-clo-gap): CDK/Pulumi projects ride the native
    // web engine and skip the gate; terraform projects gate as
    // delegated; an undetected cloud type fails closed (never assumed
    // native, never silently delegated).
    // (Tường lửa C0: đường adapter phục vụ MCP + workspace — qua cùng gate
    // như lane per-core. Nhánh cloud: CDK/Pulumi đi engine web native nên
    // không qua gate; terraform gate delegate; type không nhận diện được
    // thì fail-closed.)
    {
        use mgc_types::Ecosystem;
        // Full gate context (P0#2): the adapter path serves MCP +
        // workspace/monorepo installs — ecosystem is detected per core
        // exactly like the per-core lanes (undetectable ⇒ Unsupported).
        // (Context gate đầy đủ: detect ecosystem từng core như lane
        // per-core.)
        let app_eco: Option<&str> = {
            #[cfg(feature = "app")]
            {
                mgc_app_adapter::adapter_for(project_root).map(|a| a.language.ecosystem())
            }
            #[cfg(not(feature = "app"))]
            {
                None
            }
        };
        let lib_eco: Option<&str> = {
            #[cfg(feature = "lib")]
            {
                mgc_lib_adapter::detect_language(project_root).map(|l| l.ecosystem())
            }
            #[cfg(not(feature = "lib"))]
            {
                None
            }
        };
        let iot_fw = {
            #[cfg(feature = "iot")]
            {
                mgc_iot_adapter::adapter_for(project_root).map(|a| a.framework())
            }
            #[cfg(not(feature = "iot"))]
            {
                None
            }
        };
        // P1-1: detect the REAL game engine (bevy/godot/unity/unreal)
        // instead of hardcoding Bevy — non-Bevy engines hit the owner
        // table's catch-all Unsupported branch and fail closed there,
        // never evaluated under Bevy's ownership.
        // (Detect engine game thật — không hardcode Bevy.)
        let game_eco: Option<String> = {
            #[cfg(feature = "game")]
            {
                mgc_game_adapter::adapter_for(project_root).map(|a| a.engine().to_string())
            }
            #[cfg(not(feature = "game"))]
            {
                None
            }
        };
        let (core, ecosystem, framework): (&str, Option<&str>, Option<&str>) =
            match adapter.ecosystem() {
                Ecosystem::Web => ("web", Some(crate::commands::dep_gate::eco::JS), None),
                Ecosystem::Ai => ("ai", Some(crate::commands::dep_gate::eco::PYTHON), None),
                Ecosystem::App => ("app", app_eco, None),
                Ecosystem::Lib => ("lib", lib_eco, None),
                Ecosystem::Game => {
                    let engine = game_eco.as_deref();
                    // Bevy keeps its exact historical cell; any other
                    // detected engine flows into the table's catch-all
                    // (Unsupported, fail closed) — never Bevy's.
                    // (Chỉ Bevy giữ cell cũ — engine khác fail-closed.)
                    if engine == Some(crate::commands::dep_gate::eco::BEVY) {
                        (
                            "game",
                            Some(crate::commands::dep_gate::eco::BEVY),
                            Some(crate::commands::dep_gate::eco::BEVY),
                        )
                    } else {
                        ("game", engine, engine)
                    }
                }
                // iot carries the framework id in both slots (no separate
                // language layer exists in the iot lane).
                Ecosystem::Iot => ("iot", iot_fw, iot_fw),
                Ecosystem::Cicd => ("cicd", None, None),
                Ecosystem::Hardware => ("hardware", None, None),
                Ecosystem::Cloud => ("", None, None),
            };
        if !core.is_empty() {
            let compat = crate::commands::dep_gate::from_dep_flag(None)?;
            crate::commands::dep_gate::gate(
                &crate::commands::dep_gate::DepContext::new(
                    core,
                    ecosystem,
                    framework,
                    None,
                    crate::commands::dep_gate::DepOp::Install,
                ),
                None,
                &compat,
                Some(&project_root.join(".magicore").join("exec.log")),
            )?;
        } else {
            clo_adapter_path_gate(project_root)?;
        }
    }
    Ok(())
}

/// Generic-path resolve + materialize + hooks tail (P0-1): runs under
/// the caller's writer guard; every error after the manifest commit
/// point routes through rollback there. Test-only tail delay hook
/// (MGC_MUTATION_TAIL_DELAY_MS) parks SIGKILL E2E mid-tail.
/// (Tail resolve+install+hooks của generic path — lỗi là rollback.)
#[allow(clippy::too_many_arguments)]
async fn install_resolve_and_materialize(
    adapter: &dyn mgc_types::adapter::PackageAdapter,
    project_root: &Path,
    manifest: &Manifest,
    frozen: bool,
    ignore_scripts: bool,
    allow_scripts: bool,
    offline: bool,
) -> Result<()> {
    if let Ok(ms) = std::env::var("MGC_MUTATION_TAIL_DELAY_MS")
        && let Ok(ms) = ms.parse::<u64>()
    {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
    let started_at = std::time::Instant::now();
    let (graph, used_lockfile) =
        if let Some(graph) = load_locked_graph(project_root, adapter.name(), manifest)? {
            info("Using mgc.lock for install state.");
            (graph, true)
        } else {
            // Frozen mode fails LOUDLY on mismatch (mirrors core/shared.rs):
            // silently re-resolving would bless a tampered lock and rewrite
            // it. Missing lock vs mismatched lock get distinct errors.
            if frozen {
                if project_root.join("mgc.lock").is_file() {
                    return Err(crate::error::frozen_lock_mismatch("install"));
                }
                return Err(crate::error::frozen_lock_missing("install"));
            }
            let dep_count = manifest.all_dependencies().count();
            let spinner = create_spinner(&format!("  Resolving {dep_count} dependencies..."));
            // P0/F6: arm the age gate from THIS operation's project.
            adapter.arm_age_gate_for(project_root)?;
            let graph = adapter.resolve(manifest).await?;
            spinner.finish_and_clear();
            (graph, false)
        };

    let resolve_bar = create_progress_bar(graph.len() as u64, "Resolving...");
    if used_lockfile {
        resolve_bar.finish_with_message(format!(" Loaded {} locked packages", graph.len()));
    } else {
        resolve_bar.finish_with_message(format!(" Resolved {} packages", graph.len()));
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
        pb.finish_with_message(graph.packages[i].id.name_str().to_string());
    }

    let spinner = create_spinner("  Linking packages...");

    let opts = mgc_types::adapter::InstallOptions {
        ignore_scripts,
        allow_scripts,
        legacy_flat: crate::commands::core::shared::should_use_legacy_flat_layout(adapter.name()),
        frozen,
        offline,
        ..Default::default()
    };
    let mut summary = adapter.install(&graph, project_root, opts).await?;
    spinner.finish_and_clear();
    summary.duration_ms = started_at.elapsed().as_millis() as u64;

    // Cache-source label (B-series honesty) — see install_with_adapter.
    // Nhãn nguồn cache (B-series) — xem install_with_adapter.
    let cache_source = match summary.cache_mode {
        mgc_types::adapter::InstallCacheMode::MgCStore => "shared mgc store",
        mgc_types::adapter::InstallCacheMode::Delegated => "native toolchain cache",
    };
    mgc_ui::print_install_summary_source(
        summary.added.len(),
        summary.bytes_from_cache as usize,
        summary.duration_ms,
        "0 B",
        Some(cache_source),
    );

    mgc_ui::blank_line();
    success("All dependencies installed");

    // Post-install hooks (mgc.hooks.toml [hooks.post-install]) — fail → error
    // Post-install chạy sau khi node_modules đã materialize xong.
    crate::commands::hooks::run_event(project_root, "post-install")?;

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
    frozen: bool,  // Frozen mode: fail if lockfile needs update
) -> Result<()> {
    install_into_root(
        adapter,
        project_root,
        packages,
        ignore_scripts,
        allow_scripts,
        offline, // T4.1
        frozen,
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

// workspace_package_name moved to dispatch/engine.rs (its sole consumer,
// next to both call sites) — it was dead in the lib target and private
// to the recursive dispatch filter logic.
// Hàm đã chuyển sang dispatch/engine.rs (caller duy nhất).

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
        // P0-3 (2026-09-10 audit): rival lockfile mà thiếu mgc.lock →
        // FAIL-CLOSED kèm remediation `mgc import`, không còn warning
        // rồi resolve fresh âm thầm (nền tảng độc lập: mgc.lock là nguồn
        // chân lý duy nhất sau migration).
        let legacy = mgc_lockfile::import::detect_legacy_lockfiles(project_root);
        if !legacy.is_empty() {
            let names = legacy
                .iter()
                .map(|lock| lock.file_name)
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "Error: rival lockfile(s) detected [{names}] but mgc.lock is missing.\nRun `mgc import {}` to migrate this project before install.",
                if names.contains("deno.lock") {
                    "deno"
                } else {
                    "bun"
                }
            );
        }
        return Ok(None);
    };

    // T3.5: Auto-verify lockfile signature before install
    verify_lockfile_if_signed(project_root)?;

    // Issue #4: Re-enable lock.core, lock.version, lock.resolution checks after lockfile v2 migration
    // let state_ok = matches!(lock.resolution.state.as_str(), "locked" | "installing");
    // if lock.core != adapter_name || !state_ok || lock.version != 1 || lock.packages.is_empty() {
    if lock.packages.is_empty() {
        return Ok(None);
    }

    if lock.packages.iter().any(|pkg| pkg.name.is_empty()) {
        return Ok(None);
    }

    if !crate::commands::core::shared::lock_matches_manifest(&lock, manifest) {
        return Ok(None);
    }

    Ok(Some(graph_from_lockfile(&lock)?))
}

fn read_checked_lockfile(project_root: &std::path::Path) -> Result<Option<Lockfile>> {
    mgc_lockfile::read_lockfile_checked(project_root).map_err(|e| anyhow::anyhow!("{}", e))
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
    crate::commands::trust::policy::enforce_project_policy(&lockfile_path)?;
    let status = crate::commands::trust::policy::verify_project_lockfile(&lockfile_path)?;

    match status {
        mgc_lockfile::VerificationStatus::Valid => {
            mgc_ui::success("✓ Lockfile signature valid");
        }
        mgc_lockfile::VerificationStatus::UntrustedKey(key_id) => {
            mgc_ui::warning(&format!(
                "WARN: Lockfile signer '{key_id}' is not trusted by this project"
            ));
        }
        mgc_lockfile::VerificationStatus::Unsigned => {
            mgc_ui::warning("WARN: Lockfile not signed — run 'mgc trust sign' to sign it");
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
#[path = "../test/install_test.rs"]
mod tests;
