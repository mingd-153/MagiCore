use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use mgc_lockfile::Lockfile;
use mgc_types::adapter::{AddOptions, InstallOptions, PackageAdapter};
use mgc_types::{
    DependencySpec, Ecosystem, Manifest, PackageId, PackageName, ResolvedGraph, ResolvedPackage,
    Version, VersionRange, adapter::PreparedAdd,
};
use mgc_ui::{
    add_multi_bar, create_multi_progress, create_progress_bar, create_spinner, info, style_cmd,
    success,
};
#[cfg(feature = "web")]
use time::{Duration as TimeDuration, OffsetDateTime, format_description::well_known::Rfc3339};

#[allow(dead_code)]
pub fn find_project_root(cwd: &Path) -> Result<Option<PathBuf>> {
    Ok(mgc_config::project::ProjectConfig::find_project_root(cwd))
}

fn install_command_for_adapter(adapter: &dyn PackageAdapter) -> &'static str {
    if adapter.name() == "web" {
        return "mgc install";
    }

    "mgc install"
}

/// Run the REAL toolchain add for a toolchain-owned manifest (go.mod,
/// platformio.ini — the tool is the sole writer), then re-read the file.
/// Fail closed when the tool reports success but the dep is absent from
/// the re-read file — never report a phantom add.
/// (Chạy add thật của toolchain rồi đọc lại file; tool báo xong mà file
/// không có dep thì lỗi, không báo thêm giả.)
#[allow(clippy::too_many_arguments)]
async fn tool_add_real(
    adapter: &dyn PackageAdapter,
    root: &Path,
    name: &PackageName,
    range: Option<&VersionRange>,
    opts: AddOptions,
    before: Option<&Manifest>,
    after: &mut Option<Manifest>,
    added_ids: &mut Vec<PackageId>,
    added_packages: &mut Vec<AddedPackage>,
    changed_any: &mut bool,
    dev: bool,
    optional: bool,
    peer: bool,
    group: &str,
) -> Result<()> {
    let mut real_opts = opts;
    real_opts.no_save = false;
    let pkg_id = adapter.add(root, name, range, real_opts).await?;
    let fresh = adapter.parse_manifest(root).await?;
    if !fresh
        .all_dependencies()
        .any(|d| d.name.as_str() == name.as_str())
    {
        return Err(crate::error::tool_manifest_mismatch(
            name.as_str(),
            adapter.name(),
            "recorded",
        ));
    }
    let was_present = before
        .map(|m| {
            m.all_dependencies()
                .any(|d| d.name.as_str() == name.as_str())
        })
        .unwrap_or(false);
    *after = Some(fresh);
    if was_present {
        info(&format!(
            "  {} already present in {}, skipping",
            name.as_str(),
            group
        ));
        return Ok(());
    }
    *changed_any = true;
    added_ids.push(pkg_id.clone());
    added_packages.push(AddedPackage {
        id: pkg_id.clone(),
        dev,
        optional,
        peer,
    });
    info(&format!(
        "  {}@{} added to {} (by {})",
        pkg_id.name_str(),
        pkg_id.version(),
        group,
        adapter.name()
    ));
    success(&format!("Added {}", name.as_str()));
    Ok(())
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
        return Err(crate::error::too_many_packages(packages.len(), "add"));
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
    mgc_ui::info(&format!("Adding {} package(s) to {}...", total, group));
    // Toolchain-owned manifest (go.mod, platformio.ini — manifest_owned
    // false): the provider tool is the SOLE writer, so booking the add in
    // memory would fake a mutation the tool never sees. Run the REAL
    // toolchain add and re-read the file instead.
    // (Manifest do toolchain sở hữu: chạy add thật rồi đọc lại file.)
    let toolchain_owned = !adapter.manifest_owned();

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
        let spec = mgc_types::DependencySpec::parse(&package)?;
        let name = spec.name;
        let range = if let Some(v) = version.as_ref() {
            Some(mgc_types::VersionRange::parse(v)?)
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
        if toolchain_owned && !no_save {
            tool_add_real(
                adapter,
                root,
                &name,
                range.as_ref(),
                opts,
                manifest_before_add.as_ref(),
                &mut manifest_after_add,
                &mut added_ids,
                &mut added_packages,
                &mut changed_any,
                dev,
                optional,
                peer,
                group,
            )
            .await?;
            spinner.finish_and_clear();
            continue;
        }
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
                        mgc_ui::info(&format!(
                            "  {}@{} saved to {}",
                            pkg_id.name_str(),
                            requested_range,
                            group
                        ));
                    } else {
                        mgc_ui::info(&format!(
                            "  {}@{} added to {}",
                            pkg_id.name_str(),
                            resolved_version,
                            group
                        ));
                    }
                    mgc_ui::success(&format!("Added {}", package));
                } else {
                    mgc_ui::info(&format!(
                        "  {} already present in {}, skipping",
                        pkg_id.name_str(),
                        group
                    ));
                }
            }
        } else {
            mgc_ui::info(&format!(
                "  {}@{} checked (--no-save, manifest unchanged)",
                pkg_id.name_str(),
                requested_range
            ));
        }
    }

    if !no_save {
        if changed_any && !toolchain_owned {
            if let Some(manifest) = manifest_after_add.as_ref() {
                let write_started_at = std::time::Instant::now();
                adapter.write_manifest(root, manifest).await?;
                profile_install_mark("add_write_manifest", write_started_at);
            }
        } else if changed_any {
            // Toolchain-owned: the tool already rewrote its own file —
            // mgc must NOT rewrite it (formatting/ownership belongs to
            // the tool; the lib/go writer is a no-op by design).
            // (Tool đã tự viết file của nó — mgc không viết lại.)
            info("Manifest updated by the toolchain (mgc does not rewrite it).");
        } else {
            info("Manifest unchanged.");
        }
    }

    if !no_save && !changed_any {
        info("Skipping install because dependencies were already present.");
        return Ok(());
    }

    if !no_save && install {
        // Toolchain-owned manifests (go.mod, platformio.ini, …) whose
        // adapter has NO mgc resolver (probe fails: iot/game delegated
        // lanes): the provider tool already installed during
        // tool_add_real above — routing through the mgc-native install
        // tail would die in resolve ("does not support 'resolve'").
        // Lanes WITH a native resolver (lib Go) keep the normal tail so
        // mgc.lock still gets written. Probe is network-free by contract.
        // (Manifest do toolchain sở hữu mà adapter không có resolver:
        // bỏ qua tail install của mgc — tool đã cài.)
        if toolchain_owned && adapter.probe_dependency_resolver().is_err() {
            info("Installed by the toolchain (mgc does not own this lifecycle).");
            return Ok(());
        }
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

/// Run the REAL toolchain remove for a toolchain-owned manifest, then
/// re-read and verify each dep is GONE. Fail closed on a phantom
/// removal (tool ok, dep still present).
/// (Chạy remove thật của toolchain rồi đọc lại verify đã mất.)
async fn tool_remove_real(
    adapter: &dyn PackageAdapter,
    root: &Path,
    packages: Vec<String>,
) -> Result<()> {
    for package in &packages {
        let name = PackageName::new(package)?;
        adapter.remove(root, &name).await?;
        let fresh = adapter.parse_manifest(root).await?;
        if fresh
            .all_dependencies()
            .any(|d| d.name.as_str() == name.as_str())
        {
            return Err(crate::error::tool_manifest_mismatch(
                name.as_str(),
                adapter.name(),
                "removed but still present",
            ));
        }
        success(&format!("Removed {}", name.as_str()));
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
        return Err(crate::error::too_many_packages(packages.len(), "remove"));
    }
    info(&format!("Removing {} package(s)...", packages.len()));
    let parse_started_at = std::time::Instant::now();
    let mut manifest = adapter.parse_manifest(root).await?;
    profile_install_mark("remove_parse_manifest", parse_started_at);
    // Toolchain-owned manifest (go.mod, platformio.ini): the provider
    // tool is the SOLE writer — run the REAL toolchain remove per dep,
    // re-read, and verify absence (fail closed on a phantom removal).
    // Bookkeeping-only removal here would either fake success or die on
    // the (correctly failing) mgc writer.
    // (Manifest do toolchain sở hữu: chạy remove thật rồi đọc lại.)
    if !adapter.manifest_owned() {
        return tool_remove_real(adapter, root, packages).await;
    }
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
    // Issue #4: load_pruned_locked_graph disabled - restore after lockfile v2 migration complete
    let graph = None;
    if let Some(graph) = graph {
        info("Using mgc.lock for remaining dependency graph.");
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
        return Ok(());
    }
    install_with_adapter(
        adapter,
        root,
        install_command_for_adapter(adapter),
        false,
        mgc_types::adapter::InstallOptions {
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
                    mgc_types::adapter::InstallOptions {
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
                mgc_types::adapter::InstallOptions {
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
    opts: mgc_types::adapter::InstallOptions,
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
    let mut summary = adapter.install(&graph, root, opts).await?;
    spinner.finish_and_clear();
    profile_install_mark("adapter_install", started_at);
    summary.duration_ms = started_at.elapsed().as_millis() as u64;

    // Cache-source label (B-series honesty): name WHERE the bytes
    // came from — the shared mgc store or the native toolchain cache.
    // Nhãn nguồn cache (B-series): nêu byte đến TỪ ĐÂU — store chung
    // của mgc hay cache toolchain gốc.
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
    profile_install_mark("install_with_adapter_total", command_started_at);
    Ok(())
}

pub(crate) struct InstallExecution {
    pub graph: ResolvedGraph,
    pub summary: mgc_types::adapter::InstallSummary,
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
            summary: mgc_types::adapter::InstallSummary::default(),
            used_lockfile: false,
        });
    }

    let (graph, used_lockfile) = if let Some(graph) =
        load_locked_graph(root, adapter.name(), &manifest)?
    {
        info("Using mgc.lock for install state.");
        profile_install_mark("load_locked_graph", started_at);
        (graph, true)
    } else {
        if frozen {
            let cmd = install_command_for_adapter(adapter);
            // Tamper/mismatch (lock present but not matching) fails
            // LOUDLY — silently re-resolving would bless a tampered lock
            // and rewrite it. Missing lock is a different error.
            if root.join("mgc.lock").is_file() {
                return Err(crate::error::frozen_lock_mismatch(cmd));
            }
            return Err(crate::error::frozen_lock_missing(cmd));
        }
        let spinner = create_spinner(&format!("  Resolving {} dependencies...", all_deps.len()));
        let resolve_started_at = std::time::Instant::now();
        // P0/F6: arm the age gate from THIS operation's project (never a
        // process-global first-wins); broken config fails the op here.
        adapter.arm_age_gate_for(root)?;
        let graph = adapter.resolve(&manifest).await?;
        spinner.finish_and_clear();
        profile_install_mark("resolve_graph", resolve_started_at);
        (graph, false)
    };
    enforce_audit_strict_policy(adapter, root, &graph).await?;
    profile_install_mark("prepare_install_execution_total", started_at);
    Ok(InstallExecution {
        graph,
        summary: mgc_types::adapter::InstallSummary {
            duration_ms: started_at.elapsed().as_millis() as u64,
            ..Default::default()
        },
        used_lockfile,
    })
}

async fn enforce_audit_strict_policy(
    adapter: &dyn PackageAdapter,
    root: &std::path::Path,
    graph: &ResolvedGraph,
) -> Result<()> {
    if std::env::var_os("MGC_AUDIT_STRICT").is_none() || graph.packages.is_empty() {
        return Ok(());
    }

    if adapter.name() != "web" {
        return Err(crate::error::audit_strict_web_only(adapter.name()));
    }

    #[cfg(not(feature = "web"))]
    {
        return Err(crate::error::audit_strict_no_web_adapter());
    }

    #[cfg(feature = "web")]
    {
        enforce_web_audit_strict_policy(root, graph).await
    }
}

#[cfg(feature = "web")]
async fn enforce_web_audit_strict_policy(
    root: &std::path::Path,
    graph: &ResolvedGraph,
) -> Result<()> {
    use mgc_types::adapter::VulnerabilitySeverity;

    let registry = mgc_web_adapter::native::npm_registry::NpmRegistry::new(
        &crate::commands::web_registry_config::web_registry_url(),
    );
    let now = OffsetDateTime::now_utc();
    // Quarantine cutoff: THIS operation's mgc.toml [security] when
    // present, else the historical 24h default (--audit-strict keeps
    // working with no config file present). Broken config fails here
    // (P0/F6), never silently unfiltered.
    // (Cutoff cách ly từ project của operation này.)
    let policy = mgc_web_adapter::WebAdapter::load_age_policy_for(root)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    registry.set_age_gate_armed(policy.is_some_and(|p| p.cutoff_hours > 0));
    let cutoff_hours = policy.map(|p| p.cutoff_hours).unwrap_or(24);
    let quarantine_cutoff = now - TimeDuration::hours(cutoff_hours as i64);

    for pkg in &graph.packages {
        let metadata = registry.fetch_metadata(pkg.id.name_str()).await?;
        if let Some(published_at) = metadata.time.get(&pkg.id.version().to_string()) {
            let published = OffsetDateTime::parse(published_at, &Rfc3339).map_err(|err| {
                crate::error::audit_parse_time_failed(
                    pkg.id.name_str(),
                    &pkg.id.version().to_string(),
                    &err,
                )
            })?;
            if published > quarantine_cutoff {
                return Err(crate::error::audit_recent_blocked(
                    pkg.id.name_str(),
                    &pkg.id.version().to_string(),
                    published_at,
                ));
            }
        }
    }

    let mut body = serde_json::Map::new();
    for pkg in &graph.packages {
        body.insert(
            pkg.id.name_str().to_string(),
            serde_json::json!([pkg.id.version().to_string()]),
        );
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(format!("magicore/{}", env!("CARGO_PKG_VERSION")))
        .build()?;
    let advisory_url = crate::commands::web_registry_config::advisory_bulk_endpoint(
        &crate::commands::web_registry_config::web_registry_url(),
    )?;
    let response = client.post(advisory_url).json(&body).send().await?;
    if !response.status().is_success() {
        return Err(crate::error::audit_api_status(&response.status()));
    }
    let advisories: serde_json::Value = response.json().await?;
    if let Some(map) = advisories.as_object() {
        for (pkg_name, advisory_list) in map {
            if let Some(advisories_arr) = advisory_list.as_array() {
                for advisory in advisories_arr {
                    let severity = VulnerabilitySeverity::from_str(
                        advisory["severity"].as_str().unwrap_or("info"),
                    );
                    if severity.is_at_least(&VulnerabilitySeverity::Medium) {
                        let title = advisory["title"]
                            .as_str()
                            .unwrap_or("unknown vulnerability");
                        return Err(crate::error::audit_advisory_blocked(
                            pkg_name,
                            severity.as_str(),
                            title,
                        ));
                    }
                }
            }
        }
    }

    Ok(())
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
    // P0/F6: same per-operation arming for the delta resolve.
    adapter.arm_age_gate_for(root)?;
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
    profile_install_mark("install_delta_with_lock_total", started_at);
    Ok(true)
}

fn build_delta_manifest(manifest: &Manifest, added_packages: &[AddedPackage]) -> Result<Manifest> {
    let mut delta = Manifest::new(&manifest.name, manifest.ecosystem);
    for package in added_packages {
        let mut spec = DependencySpec::new(
            package.id.name().clone(),
            mgc_types::VersionRange::parse(&format!("={}", package.id.version()))?,
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
    let enabled = std::env::var("MAGICORE_WEB_PROFILE_INSTALL")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false);
    if enabled {
        eprintln!(
            "[magicore:web:command-profile] {}={}ms",
            label,
            started_at.elapsed().as_millis()
        );
    }
}

#[allow(dead_code)]
fn load_locked_graph(
    project_root: &Path,
    _adapter_name: &str,
    manifest: &Manifest,
) -> Result<Option<ResolvedGraph>> {
    // P0-3 (2026-09-10 audit): silent seeding từ legacy lockfile ĐÃ GỠ —
    // mgc.lock là nguồn chân lý duy nhất; migration phải tường minh qua
    // `mgc import`. Thiếu mgc.lock → resolver tự resolve (không có rival
    // lockfile nào được đọc trên đường vận hành).
    let Some(lock) = read_checked_lockfile(project_root)? else {
        return Ok(None);
    };
    // Issue #4: Re-enable lock.core, lock.version, lock.resolution checks after lockfile v2 migration
    // let state_ok = matches!(lock.resolution.state.as_str(), "locked" | "installing");
    // if lock.core != adapter_name || !state_ok || lock.version != 1 || lock.packages.is_empty() {
    if lock.packages.is_empty() {
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
    mgc_lockfile::read_lockfile_checked(project_root).map_err(|e| anyhow::anyhow!("{}", e))
}

#[allow(dead_code)]
fn lock_matches_manifest(lock: &Lockfile, manifest: &Manifest) -> bool {
    // Any-match over same-named packages: multi-version locks are
    // legitimate (a peer edge may resolve another version), so the
    // manifest range passes when ANY instance satisfies it — first-match
    // order must never decide.
    // (Khớp bất kỳ instance nào — lock đa-version hợp lệ.)
    manifest.all_dependencies().all(|dependency| {
        lock.get_packages(dependency.name.as_str())
            .filter_map(|package| Version::parse(&package.version).ok())
            .any(|version| dependency.range.matches(&version))
    })
}

// Issue #4: Disabled due to lockfile v2 migration
// fn load_pruned_locked_graph(
//     project_root: &Path,
//     adapter_name: &str,
//     manifest: &Manifest,
// ) -> Result<Option<ResolvedGraph>> {
//     // Function body removed — needs v2 schema rewrite
//     unimplemented!("load_pruned_locked_graph disabled pending lockfile v2 migration")
// }

#[allow(dead_code)]
fn graph_from_lockfile(lock: &Lockfile) -> Result<ResolvedGraph> {
    let packages = lock
        .packages
        .iter()
        .map(|package| {
            let id = PackageId::parse(&format!("{}@{}", package.name, package.version))?;
            let deps = package
                .dependencies
                .iter()
                .map(|dependency| {
                    // npm-style `name@version` first; Go-style bare module
                    // paths (`github.com/google/uuid`) resolve their
                    // version from the same lock by name — a bare dep that
                    // names nothing in the lock fails closed.
                    // (`name@version` trước; path trần kiểu Go tra version
                    // trong cùng lock.)
                    if let Ok(id) = PackageId::parse(dependency) {
                        return Ok::<PackageId, mgc_types::MgError>(id);
                    }
                    let target = lock.packages.iter().find(|p| p.name == *dependency)
                        .ok_or_else(|| {
                            mgc_types::MgError::Other(format!(
                                "lockfile dependency '{dependency}' names no locked package (fail-closed)"
                            ))
                        })?;
                    Ok(PackageId::new(
                        mgc_types::PackageName::new(&target.name)?,
                        mgc_types::Version::parse(&target.version)?,
                    ))
                })
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(ResolvedPackage {
                id,
                integrity: package.integrity.clone(),
                tarball_url: package.resolved.clone(),
                deps,
                peer_deps: vec![],
                direct: false,
                dev: false,
            })
        })
        .collect::<Result<Vec<_>, mgc_types::MgError>>()?;
    Ok(ResolvedGraph { packages })
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
        return Err(crate::error::link_web_only());
    }
    let pkg = package.ok_or_else(crate::error::link_usage)?;
    info(&format!("Linking {}...", pkg));

    let node_modules = root.join("node_modules");
    let name = link_name(pkg);
    let link_path = node_modules.join(name);

    if link_path.exists() {
        return Err(crate::error::link_exists(name));
    }

    std::fs::create_dir_all(&node_modules)?;
    let source = find_package_source(root, pkg)?;

    #[cfg(unix)]
    std::os::unix::fs::symlink(&source, &link_path)?;

    #[cfg(windows)]
    {
        if source.is_dir() {
            std::os::windows::fs::symlink_dir(&source, &link_path)?;
        } else {
            std::os::windows::fs::symlink_file(&source, &link_path)?;
        }
    }

    success(&format!("Linked {} -> {}", name, source.display()));
    Ok(())
}

pub async fn unlink(
    adapter: &dyn PackageAdapter,
    root: &Path,
    package: Option<&str>,
) -> Result<()> {
    if adapter.name() != "web" {
        return Err(crate::error::unlink_web_only());
    }
    let pkg = package.ok_or_else(crate::error::unlink_usage)?;
    info(&format!("Unlinking {}...", pkg));

    let name = link_name(pkg);
    let link_path = root.join("node_modules").join(name);
    if !link_path.exists() {
        return Err(crate::error::unlink_not_linked(name));
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
        return Err(crate::error::why_web_only());
    }
    let lock_path = root.join("mgc.lock");
    if !lock_path.exists() {
        return Err(crate::error::lock_missing_install());
    }
    // Reverse-dependency lookup over the lockfile graph (v2/v3/v4): who
    // pulls this package in. Replaces the old `unimplemented!()` stub
    // (P0-4: a user-reachable panic) — Issue #4 closed by reading the
    // edges the installer actually wrote instead of a `pkg.direct` field.
    // (Tra cứu phụ thuộc ngược trên graph lockfile — thay stub panic cũ.)
    let text = std::fs::read_to_string(&lock_path)?;
    let version = toml::from_str::<toml::Value>(&text)
        .ok()
        .and_then(|v| v.get("version")?.as_str().map(str::to_string))
        .unwrap_or_default();
    // (name, version, outgoing dep edges) in one shape for every schema.
    type WhyNode = (String, String, Vec<String>);
    let (nodes, roots): (Vec<WhyNode>, Vec<String>) = match version.as_str() {
        "4" => {
            let doc = mgc_lockfile::canonical::parse_v4_document(&text)
                .map_err(|e| anyhow::anyhow!("parse mgc.lock v4 failed: {e}"))?;
            let payload = doc.payload();
            let nodes = payload
                .packages
                .iter()
                .map(|p| {
                    (
                        p.key.name.clone(),
                        p.key.version.clone(),
                        p.edges.iter().map(|e| e.target_key.name.clone()).collect(),
                    )
                })
                .collect();
            let roots = payload
                .root_dependencies
                .iter()
                .map(|e| edge_name(e))
                .collect();
            (nodes, roots)
        }
        _ => {
            let lock = mgc_lockfile::parser::parse_lockfile(&text)
                .map_err(|e| anyhow::anyhow!("parse mgc.lock failed: {e}"))?;
            let nodes = lock
                .packages
                .iter()
                .map(|p| {
                    (
                        p.name.clone(),
                        p.version.clone(),
                        p.dependencies.iter().map(|e| edge_name(e)).collect(),
                    )
                })
                .collect();
            let roots = lock
                .root_dependencies
                .iter()
                .map(|e| edge_name(e))
                .collect();
            (nodes, roots)
        }
    };
    let target = edge_name(package);
    let target_node = nodes.iter().find(|(name, _, _)| name == &target);
    let Some((_, target_version, _)) = target_node else {
        return Err(crate::error::why_package_not_in_lock(package));
    };
    // Reverse BFS: dependents first, root last. Visited-set keeps cycles
    // finite; depth + line caps keep output readable on huge graphs.
    // (BFS ngược: dependent trước, root sau; visited-set chặn cycle.)
    let mut dependents: Vec<&WhyNode> = nodes
        .iter()
        .filter(|(name, _, deps)| {
            name.as_str() != target.as_str() && deps.iter().any(|d| d == &target)
        })
        .collect();
    dependents.sort_by(|a, b| a.0.cmp(&b.0));
    if dependents.is_empty() && !roots.iter().any(|r| r == &target) {
        mgc_ui::info(&format!(
            "{target}@{target_version} is locked but nothing in mgc.lock depends on it (likely a leftover pin — `mgc dedupe`/`prune` may drop it)."
        ));
        return Ok(());
    }
    let mut lines = vec![format!("{target}@{target_version} is required by:")];
    for (name, version, _) in dependents.iter().take(20) {
        lines.push(format!("  {name}@{version}"));
    }
    if dependents.len() > 20 {
        lines.push(format!("  ... and {} more", dependents.len() - 20));
    }
    if roots.iter().any(|r| r == &target) {
        lines.push("(project root — direct dependency)".to_string());
    }
    for line in &lines {
        mgc_ui::info(line);
    }
    Ok(())
}

/// Bare package name from a lock edge (`name`, `name@1.2.3`, or
/// `@scope/name@1.2.3`) — version/range suffixes never participate in
/// identity matching.
/// (Tên trần từ cạnh lock — hậu tố version không tham gia so khớp.)
fn edge_name(edge: &str) -> String {
    let edge = edge.trim();
    if let Some(rest) = edge.strip_prefix('@') {
        match rest.find('@') {
            Some(idx) => format!("@{}", &rest[..idx]),
            None => edge.to_string(),
        }
    } else {
        match edge.find('@') {
            Some(idx) => edge[..idx].to_string(),
            None => edge.to_string(),
        }
    }
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
        return Err(crate::error::local_path_not_found(&path));
    }

    // Check local node_modules
    let local = root.join("node_modules").join(package);
    if local.exists() {
        return Ok(local);
    }

    for global in magicore_global_package_roots() {
        let global_pkg = global.join(package);
        if global_pkg.exists() {
            return Ok(global_pkg);
        }
    }

    Err(crate::error::pkg_not_found_local(package))
}

fn magicore_global_package_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(path) = std::env::var_os("MAGICORE_GLOBAL_PACKAGE_ROOT") {
        roots.push(PathBuf::from(path));
    }
    if let Some(cache_dir) = dirs::cache_dir() {
        roots.push(
            cache_dir
                .join("magicore")
                .join("global")
                .join("web")
                .join("node_modules"),
        );
    }
    roots
}

pub fn should_use_legacy_flat_layout(core_type: &str) -> bool {
    if core_type != "web" {
        return false;
    }
    let env_val = std::env::var("MAGICORE_WEB_STRICT_LAYOUT")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase());
    match env_val.as_deref() {
        Some("1" | "true" | "yes" | "on") => false,
        Some("0" | "false" | "no" | "off" | "legacy" | "flat") => true,
        Some(_) => false,
        None => false,
    }
}

// ── Core helpers chung (Phase 7 v5 — user chốt 2026-08-19: LỆNH = folder, CORE = file con) ──
// Mỗi lệnh folder (add/, install/, ...) tái dùng project_root + adapter của core ở đây,
// thay vì duplic per file. Message giữ từng core để không mất context lỗi.

/// project_root của core — seeded từ find_project_root (mgc.toml/.mgc.core/package.json...).
#[cfg(any(feature = "game", feature = "clo"))]
pub fn core_project_root(core: &str) -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = find_project_root(&cwd)?.ok_or_else(|| crate::error::no_mgc_project_found(core))?;
    Ok(root)
}

/// Adapter của core — wrapper factory (expect giữ fail-closed như từng core cũ).
#[cfg(any(feature = "game", feature = "clo"))]
pub fn core_adapter(eco: &Ecosystem) -> Arc<dyn PackageAdapter> {
    crate::factory::create_adapter(eco, None, None)
        .expect("core adapter always available in this core build")
}

/// game: materialize optimizer template + hook dep (bevy). Dùng chung add/install game.
#[cfg(feature = "game")]
pub async fn game_optimizer_template(root: &Path) -> Result<()> {
    materialize_template(root, OPTIMIZER_PKG).await?;
    game_hook_optimizer_dep(root)
}

/// game: thêm dep path `mgc-optimizer = { path = "./optimizer" }` vào root Cargo.toml (bevy only).
#[cfg(feature = "game")]
fn game_hook_optimizer_dep(root: &Path) -> Result<()> {
    let manifest = root.join("Cargo.toml");
    if !manifest.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&manifest)?;
    let mut v: toml::Value = toml::from_str(&content)?;
    let deps = v["dependencies"]
        .as_table_mut()
        .ok_or_else(crate::error::cargo_toml_no_deps)?;
    if deps.contains_key("mgc-optimizer") {
        return Ok(());
    }
    deps.insert(
        "mgc-optimizer".to_string(),
        toml::Value::Table(toml::map::Map::from_iter([(
            "path".to_string(),
            toml::Value::String("./optimizer".to_string()),
        )])),
    );
    std::fs::write(&manifest, toml::to_string_pretty(&v)?)?;
    Ok(())
}

// ── ai helpers (Phase 7 v5) ───────────────────────────────────────────────────

/// ai project root — detect qua mgc_ai_adapter (không dùng find_project_root).
#[cfg(feature = "ai")]
pub fn ai_project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    mgc_ai_adapter::adapter_for(&cwd)
        .map(|_| cwd.clone())
        .ok_or_else(crate::error::ai_project_not_detected)
}

/// ai: chọn tool theo lock file DUY NHẤT — uv.lock/pyproject → uv,
/// requirements → pip. KHÔNG scan PATH, KHÔNG spawn probe (`--version`,
/// `which`): tool probing trước gate là bypass C0 (P0 fix) — gate chạy
/// trước, spawn thật (hoặc lỗi tool-missing rõ ràng) xảy ra sau gate.
/// (ai: pick tool by lock file ONLY — no PATH scan, no spawn probe.)
#[cfg(feature = "ai")]
pub fn ai_pick_tool(root: &std::path::Path) -> &'static str {
    if root.join("uv.lock").exists() || root.join("pyproject.toml").exists() {
        "uv"
    } else {
        "pip"
    }
}

/// Actual provider tool a lib mutating verb (add/remove/update) will
/// spawn for this language — MUST match adapters/lib/src/adapter.rs
/// arm-for-arm: Rust→cargo, Python→pip (the adapter hardcodes pip — uv
/// NEVER spawns here, so the gate set excludes uv), Go→go, Ts→None
/// (native web engine, no spawn), Java/DotNet→None (no runner; the gate
/// fails Unsupported before any spawn).
/// (Tool thật lane lib sẽ spawn theo ngôn ngữ — khớp từng arm adapter.)
#[cfg(feature = "lib")]
pub fn lib_edit_tool(lang: mgc_lib_adapter::LibLanguage) -> Option<&'static str> {
    match lang {
        mgc_lib_adapter::LibLanguage::Ts => None,
        mgc_lib_adapter::LibLanguage::Rust => Some("cargo"),
        mgc_lib_adapter::LibLanguage::Python => Some("pip"),
        mgc_lib_adapter::LibLanguage::Go => Some("go"),
        mgc_lib_adapter::LibLanguage::Java | mgc_lib_adapter::LibLanguage::DotNet => None,
    }
}

/// Actual provider tool an iot lane will spawn for the detected
/// framework: esp32-rust→cargo, platformio→pio, zephyr→west.
/// (Tool thật lane iot sẽ spawn theo framework.)
#[cfg(feature = "iot")]
pub fn iot_framework_tool(framework: &str) -> Option<&'static str> {
    match framework {
        "esp32-rust" => Some("cargo"),
        "platformio" => Some("pio"),
        "zephyr" => Some("west"),
        _ => None,
    }
}

/// Post-gate pip binary resolution: the gate already approved the pip
/// owner (`pip` ~ `pip3` alias); this picks the binary that EXISTS for
/// the real spawn — `pip` preferred, `pip3` fallback, else `pip` so a
/// missing tool surfaces a clear spawn error. Filesystem lookup ONLY
/// (no `--version` probe spawn), `split_paths` for Windows correctness,
/// and called strictly AFTER the gate.
/// (Resolve binary pip SAU gate: chỉ lookup filesystem, không spawn probe.)
#[cfg(feature = "ai")]
fn resolve_ai_tool(tool: &str) -> &str {
    if tool != "pip" {
        return tool;
    }
    fn on_path(bin: &str) -> bool {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|dir| dir.join(bin))
            .any(|p| p.is_file())
    }
    if on_path("pip") || !on_path("pip3") {
        "pip"
    } else {
        "pip3"
    }
}

#[cfg(feature = "ai")]
pub fn ai_run_tool(root: &std::path::Path, tool: &str, args: &[String]) -> Result<()> {
    let tool = resolve_ai_tool(tool);
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    mgc_exec::prelude::run_inherited(tool, args, &opts)
        .map_err(|e| crate::error::tool_failed(tool, &e))?;
    Ok(())
}

#[cfg(feature = "ai")]
pub fn ai_run_tool_with_env(
    root: &std::path::Path,
    tool: &str,
    args: &[String],
    env: Vec<(String, String)>,
) -> Result<()> {
    let tool = resolve_ai_tool(tool);
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        env,
        clean_env: true,
        ..Default::default()
    };
    mgc_exec::prelude::run_inherited(tool, args, &opts)
        .map_err(|e| crate::error::tool_failed(tool, &e))?;
    Ok(())
}

/// Shared pypi store env (B-series, 2026-09-12): PIP_CACHE_DIR and
/// UV_CACHE_DIR point inside the mgc store so ai + lib python lanes
/// share the same wheel/sdist bytes machine-wide.
/// Env store pypi chia sẻ (B-series): PIP_CACHE_DIR và UV_CACHE_DIR trỏ
/// vào store mgc để lane ai + lib python chia sẻ cùng byte wheel/sdist
/// trên toàn máy.
#[cfg(feature = "ai")]
pub fn shared_pypi_store_env() -> Result<Vec<(String, String)>> {
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot resolve home dir for the shared store"))?;
    let pypi_root = home.join(".magicore").join("store").join("pypi");
    std::fs::create_dir_all(&pypi_root)
        .map_err(|e| anyhow::anyhow!("cannot create shared pypi store: {e}"))?;
    let root = pypi_root.display().to_string();
    Ok(vec![
        ("PIP_CACHE_DIR".to_string(), root.clone()),
        ("UV_CACHE_DIR".to_string(), root),
    ])
}

#[cfg(feature = "ai")]
pub fn ai_run_tool_capture(root: &std::path::Path, tool: &str, args: &[String]) -> Result<String> {
    let tool = resolve_ai_tool(tool);
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    let report = mgc_exec::prelude::run(tool, args, &opts)
        .map_err(|e| crate::error::tool_failed(tool, &e))?;
    Ok(report.stdout_tail)
}

/// ai: entry script qua python3 (Q20, allowlist §5.1) — `mgc dev` ai.
#[cfg(feature = "ai")]
pub async fn ai_dev(_dry_run: bool) -> Result<()> {
    let root = ai_project_root()?;
    let framework =
        mgc_ai_adapter::adapter_for(&root).ok_or_else(|| crate::error::no_ai_framework(&root))?;
    let script = framework.framework.entry_script().to_string();

    // Detect AI runtime for optimizer env loading
    // Phát hiện runtime AI để load env optimizer
    let runtime = detect_ai_runtime(&root, framework.framework.as_str());
    let optimizer_envs =
        crate::commands::optimizer::env_loader::load_optimizer_env(&root, &runtime)
            .map_err(|e| {
                mgc_ui::warning(&format!("Failed to load optimizer config: {}", e));
                e
            })
            .unwrap_or_default();
    let env: Vec<(String, String)> = optimizer_envs.into_iter().collect();

    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.clone()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        env,
        clean_env: true,
        execution_scope: Some(mgc_exec::prelude::ExecutionScope::DevServer),
        ..Default::default()
    };
    let cmd = "python3".to_string();
    mgc_ui::info(&format!("AI dev: running `{} {}`...", cmd, script));
    mgc_exec::prelude::run_inherited(&cmd, &[script], &opts)
        .map_err(|e| crate::error::python3_failed(&e))?;
    Ok(())
}

/// Detect AI runtime from project
/// Phát hiện runtime AI từ project
pub(crate) fn detect_ai_runtime(
    root: &std::path::Path,
    framework: &str,
) -> crate::commands::optimizer::runtime_detect::DetectedRuntime {
    use crate::commands::optimizer::runtime_detect::DetectedRuntime;

    match framework {
        "python-agent" | "mcp-server" => {
            // SAFETY: Don't assume PyTorch for generic Python AI frameworks
            // AN TOÀN: Không giả định PyTorch cho framework Python AI chung
            if crate::commands::optimizer::runtime_detect::python_project_declares_package(
                root, "torch",
            ) {
                DetectedRuntime::PythonPyTorch
            } else {
                DetectedRuntime::Unknown
            }
        }
        "pytorch" | "PyTorch" => DetectedRuntime::PythonPyTorch,
        "candle" | "Candle" => DetectedRuntime::RustCandle,
        "tensorflow-go" | "TensorFlow" => DetectedRuntime::GoTensorFlow,
        _ => DetectedRuntime::Unknown, // Safe fallback, no env injection
    }
}

/// Hardware core — optimizer/bench packages (shared cho game/ai/cloud).
/// Không có native package manager: packages được materialize từ templates/hardware/.
/// Unconditional (no feature gate): single-core builds reference these
/// names in match patterns, and a cfg'd-out const turns into a pattern
/// binding (E0408) instead of a comparison — this broke every
/// single-core CI build.
/// (Không gate feature: const bị cfg-out sẽ thành binding trong pattern.)
pub const OPTIMIZER_PKG: &str = "optimizer";
pub const BENCH_PKG: &str = "bench";

pub async fn materialize_template(root: &Path, framework: &str) -> anyhow::Result<()> {
    let target_dir = root.join(framework);
    if target_dir.exists() {
        return Ok(()); // đã có — không ghi đè
    }
    // Hardware templates are code-generated by the hardware processor
    // (optimizer.json / bench.json) — there is no registry layer to
    // fetch, so resolve them directly instead of failing on a missing
    // layer. Every other core keeps the layer gate below.
    // (Template hardware do processor sinh trực tiếp — không qua layer.)
    if matches!(framework, BENCH_PKG | OPTIMIZER_PKG) {
        let config = crate::wizard::engine::ScaffoldConfig {
            core: "hardware".to_string(),
            sub_type: String::new(),
            frameworks: vec![framework.to_string()],
            project_name: target_dir.to_string_lossy().to_string(),
            features: vec![],
            template_dir: std::path::PathBuf::new(),
        };
        crate::scaffold::processor::Scaffolder::scaffold(&config)?;
        return Ok(());
    }
    // Phase 3: Handle typed result - hardware không có fallback
    match crate::commands::template::ensure_layer(&format!("hardware/{framework}")).await {
        Ok(status) if status.is_available() => {}
        Ok(_) | Err(_) => {
            anyhow::bail!(
                "Required hardware template layer 'hardware/{}' missing - no fallback available",
                framework
            );
        }
    }
    let config = crate::wizard::engine::ScaffoldConfig {
        core: "hardware".to_string(),
        sub_type: String::new(),
        frameworks: vec![framework.to_string()],
        project_name: target_dir.to_string_lossy().to_string(),
        features: vec![],
        template_dir: std::path::PathBuf::new(),
    };
    crate::scaffold::processor::Scaffolder::scaffold(&config)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/shared.rs"]
mod tests;
