use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use mgc_lockfile::Lockfile;
use mgc_lockfile::project_lock::ProjectWriteLock;
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

/// Writer-lock acquire timeout: mgc.toml [lock].acquire_timeout_ms,
/// else 30s (mirrors migrate). A live holder is never robbed — timeout
/// surfaces LockBusy, never last-writer-wins.
/// (Timeout lock writer: config hoặc 30s — không cướp holder sống.)
pub(crate) fn writer_lock_timeout(root: &Path) -> std::time::Duration {
    let ms = mgc_config::project::ProjectConfig::load(root)
        .ok()
        .flatten()
        .and_then(|cfg| cfg.lock)
        .and_then(|lock| lock.acquire_timeout_ms)
        .unwrap_or(30_000);
    std::time::Duration::from_millis(ms)
}

/// Single mutation gateway (P0): EVERY manifest/lock mutation
/// (add/remove/update/install) MUST enter here — acquire the writer lock,
/// adopt any legacy journal path, recover a stale journal, return the
/// guard. Ordering proof (no in-memory state needed, hence no
/// cross-project confusion): this process holds the exclusive OS lock
/// and has staged NOTHING yet, so any journal present is stale by
/// construction — from a dead process (lock auto-released on death) or
/// a previous op that kept it on restore failure. Tails run under the
/// caller's guard via `_locked` variants and never re-enter the gateway
/// (the OS lock is not same-process re-entrant).
/// (Cổng duy nhất: thứ tự acquire→recover→stage chứng minh được journal
/// gặp ở đây là stale — không cần cờ in-memory, không lẫn project.)
pub(crate) async fn begin_dependency_mutation(
    adapter: &dyn PackageAdapter,
    root: &Path,
    op: MutationOperation,
) -> Result<ProjectWriteLock> {
    let write_lock = ProjectWriteLock::acquire(root, writer_lock_timeout(root)).map_err(|e| {
        anyhow::anyhow!(
            "{} cannot acquire the project writer lock: {e}",
            op.as_str()
        )
    })?;
    adopt_legacy_journal(root, &write_lock)?;
    recover_interrupted_remove(adapter, root, &write_lock).await?;
    Ok(write_lock)
}

/// Migrate-side guard (no adapter available there): refuse to migrate
/// over an unrestored remove journal instead of paving over it. A
/// completed journal is only deleted; anything else fails closed.
/// (Migrate không đè lên journal chưa phục hồi — lỗi chứ không đoán.)
pub(crate) fn ensure_no_pending_remove_journal(root: &Path, lock: &ProjectWriteLock) -> Result<()> {
    let journal_path = mutation_journal_dir(root).join("journal.json");
    match std::fs::symlink_metadata(&journal_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(anyhow::anyhow!(
                "cannot stat remove journal '{}': {e:#}",
                journal_path.display()
            ));
        }
        Ok(_) => {}
    }
    let journal = parse_mutation_journal(&read_project_string(&journal_path)?, &journal_path)?;
    if journal.state != JournalState::InProgress {
        clear_mutation_journal(root, lock);
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "refusing to migrate with an unrestored remove journal (staged by pid {}, op '{}'): run the interrupted remove/install first so it recovers, then migrate",
        journal.pid,
        journal.op.as_str(),
    ))
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
    // Mutation gateway (P0): the manifest write below and the install
    // tail must not interleave with a concurrent remove/update/install
    // on the same project — hold the writer lock for the whole op
    // (resolves included), and recover any stale journal FIRST so this
    // op never builds on unrestored state.
    // (Add qua gateway — lock + recovery trước mọi mutation.)
    let write_lock = begin_dependency_mutation(adapter, root, MutationOperation::Add).await?;
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
    // Pre-image for the add journal (P0-3): captured under the gateway
    // lock before any resolve/write. Staged only when mgc actually
    // rewrites an owned manifest (toolchain-owned files are the tool's
    // transactional domain — mgc must not roll them back).
    // (Snapshot pre-image cho journal add — chỉ stage khi mgc tự viết.)
    let add_snapshot = if !no_save && !toolchain_owned {
        manifest_before_add
            .as_ref()
            .map(|manifest| {
                MutationSnapshot::capture(manifest, root, &write_lock, MutationOperation::Add)
            })
            .transpose()?
    } else {
        None
    };
    let mut add_journaled = false;
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
                if let Some(snapshot) = add_snapshot.as_ref() {
                    stage_mutation_journal(
                        root,
                        adapter,
                        &added_packages
                            .iter()
                            .map(|added| added.id.name_str().to_string())
                            .collect::<Vec<_>>(),
                        snapshot,
                        &write_lock,
                    )?;
                    add_journaled = true;
                }
                let write_started_at = std::time::Instant::now();
                // Journaled (mgc-owned manifest) writes route every error
                // through rollback; unjournaled (toolchain-owned) writes
                // keep the legacy direct error (the tool owns that file).
                // (Chỉ đường journal khi có journal — tool-owned giữ cũ.)
                if let (true, Some(snapshot)) = (add_journaled, add_snapshot.as_ref()) {
                    journaled_step(
                        adapter,
                        root,
                        snapshot,
                        &write_lock,
                        "before-manifest-write",
                        adapter.write_manifest(root, manifest),
                        "after-manifest-write",
                    )
                    .await?;
                } else {
                    adapter.write_manifest(root, manifest).await?;
                }
                profile_install_mark("add_write_manifest", write_started_at);
                if let (true, Some(snapshot)) = (add_journaled, add_snapshot.as_ref()) {
                    journaled_step(
                        adapter,
                        root,
                        snapshot,
                        &write_lock,
                        "before-post-image",
                        async { record_post_image(root, manifest, &write_lock) },
                        "after-post-image",
                    )
                    .await?;
                }
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
        let tail: Result<()> = async {
            if !try_install_added_packages_from_lock(
                adapter,
                root,
                manifest_before_add.as_ref(),
                &added_packages,
            )
            .await?
            {
                install_with_adapter_locked(
                    adapter,
                    root,
                    install_command_for_adapter(adapter),
                    false,
                    InstallOptions {
                        incremental: true,
                        force_install: added_ids,
                        ..Default::default()
                    },
                    &write_lock,
                )
                .await?;
            }
            Ok(())
        }
        .await;
        match tail {
            Ok(()) => {
                if add_journaled && let Err(e) = finish_mutation_journal(root, &write_lock) {
                    if let Some(snapshot) = add_snapshot.as_ref() {
                        return rollback_mutation(adapter, root, snapshot, e, &write_lock).await;
                    }
                    return Err(e);
                }
            }
            Err(e) => {
                if let Some(snapshot) = add_snapshot.as_ref() {
                    return rollback_mutation(adapter, root, snapshot, e, &write_lock).await;
                }
                return Err(e);
            }
        }
    } else if !no_save {
        if add_journaled && let Err(e) = finish_mutation_journal(root, &write_lock) {
            if let Some(snapshot) = add_snapshot.as_ref() {
                return rollback_mutation(adapter, root, snapshot, e, &write_lock).await;
            }
            return Err(e);
        }
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
    // Single mutation gateway: lock + stale-journal recovery. Direct
    // lock acquisition that forgets recovery is a bug (P0).
    // (Qua cổng gateway duy nhất — không tự acquire rồi quên recovery.)
    let write_lock = begin_dependency_mutation(adapter, root, MutationOperation::Remove).await?;
    let parse_started_at = std::time::Instant::now();
    let mut manifest = adapter.parse_manifest(root).await?;
    profile_install_mark("remove_parse_manifest", parse_started_at);
    // Best-effort rollback scope (NOT full store-level atomicity — the
    // install tail re-resolves instead of using the pruned graph, and
    // store/cache additions are additive-only): snapshot the manifest
    // struct + mgc.lock bytes BEFORE the edit, and stage a crash journal
    // so a failure below (or a SIGKILL before the tail finishes) can
    // restore both files. Must capture BEFORE the remove loop mutates.
    // (Snapshot + journal TRƯỚC khi sửa — rollback/journal phục hồi cả
    // manifest lẫn lock; store/cache chỉ thêm, không xóa.)
    let snapshot =
        MutationSnapshot::capture(&manifest, root, &write_lock, MutationOperation::Remove)?;
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
    // Commit point: journal first (crash before this line mutated
    // nothing), then the manifest write.
    // (Ghi journal trước — crash trước dòng này chưa sửa gì.)
    stage_mutation_journal(root, adapter, &packages, &snapshot, &write_lock)?;
    // Post-stage result boundary (P0-3): every fallible step below
    // routes through rollback — no bare `?` may leak a staged journal.
    // (Mọi bước post-stage qua rollback — không `?` thẳng.)
    journaled_step(
        adapter,
        root,
        &snapshot,
        &write_lock,
        "before-manifest-write",
        adapter.write_manifest(root, &manifest),
        "after-manifest-write",
    )
    .await?;
    profile_install_mark("remove_write_manifest", write_started_at);
    journaled_step(
        adapter,
        root,
        &snapshot,
        &write_lock,
        "before-post-image",
        async { record_post_image(root, &manifest, &write_lock) },
        "after-post-image",
    )
    .await?;
    if !install {
        info(&format!(
            "Run '{}' to update lockfile and node_modules",
            style_cmd(install_command_for_adapter(adapter))
        ));
        if let Err(e) = finish_mutation_journal(root, &write_lock) {
            return rollback_mutation(adapter, root, &snapshot, e, &write_lock).await;
        }
        return Ok(());
    }
    info("Re-installing dependency graph...");
    // Test-only delay hook (precedent: MGC_LOCK_FAILPOINT): lets E2E
    // SIGKILL the process deterministically mid-tail. Production cost is
    // one env read when unset; never set it outside tests.
    // (Hook delay chỉ-cho-test — E2E SIGKILL đúng giữa tail.)
    if let Ok(ms) = std::env::var("MGC_MUTATION_TAIL_DELAY_MS")
        && let Ok(ms) = ms.parse::<u64>()
    {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
    // Issue #4: load_pruned_locked_graph disabled - restore after lockfile v2 migration complete
    let graph = None;
    if let Some(graph) = graph {
        info("Using mgc.lock for remaining dependency graph.");
        let started_at = std::time::Instant::now();
        let spinner = create_spinner("  Linking packages...");
        let install_result = adapter
            .install(
                &graph,
                root,
                InstallOptions {
                    incremental: true,
                    ..Default::default()
                },
            )
            .await;
        spinner.finish_and_clear();
        let mut summary = match install_result {
            Ok(summary) => summary,
            Err(e) => {
                return rollback_mutation(adapter, root, &snapshot, e, &write_lock).await;
            }
        };
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
        if let Err(e) = finish_mutation_journal(root, &write_lock) {
            return rollback_mutation(adapter, root, &snapshot, e, &write_lock).await;
        }
        return Ok(());
    }
    match install_with_adapter_locked(
        adapter,
        root,
        install_command_for_adapter(adapter),
        false,
        mgc_types::adapter::InstallOptions {
            incremental: true,
            ..Default::default()
        },
        &write_lock,
    )
    .await
    {
        Ok(()) => {
            if let Err(e) = finish_mutation_journal(root, &write_lock) {
                return rollback_mutation(adapter, root, &snapshot, e, &write_lock).await;
            }
            Ok(())
        }
        Err(e) => rollback_mutation(adapter, root, &snapshot, e, &write_lock).await,
    }
}

/// Pre-edit state for one remove operation: the parsed manifest plus the
/// raw mgc.lock bytes (None when no lock existed). The manifest restores
/// SEMANTICALLY (writers normalize formatting, so byte-identity is
/// impossible); the mgc-owned lock restores BYTE-IDENTICAL.
/// (Snapshot trước sửa: manifest theo nghĩa + lock theo byte.)
pub(crate) struct MutationSnapshot {
    manifest: Manifest,
    lock_bytes: Option<Vec<u8>>,
    op: MutationOperation,
}

impl MutationSnapshot {
    pub(crate) fn capture(
        manifest: &Manifest,
        root: &Path,
        _lock: &ProjectWriteLock,
        op: MutationOperation,
    ) -> Result<Self> {
        let lock_path = root.join("mgc.lock");
        refuse_project_link(&lock_path)?;
        let lock_bytes = match std::fs::read(&lock_path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "remove cannot snapshot '{}': {e:#}",
                    lock_path.display()
                ));
            }
        };
        Ok(Self {
            manifest: manifest.clone(),
            lock_bytes,
            op,
        })
    }
}

/// Refuse symlink/junction/reparse-point project paths: a swapped path
/// would move a read or write outside the project (shares the rule with
/// the writer-lock guard).
/// (Từ chối symlink/junction/reparse — chống path bị tráo.)
fn refuse_project_link(path: &Path) -> Result<()> {
    if mgc_lockfile::project_lock::path_is_link_or_reparse(path) {
        return Err(anyhow::anyhow!(
            "refusing link-swapped project path '{}'",
            path.display()
        ));
    }
    Ok(())
}

/// Read a project file ONLY after refusing link-swaps (P0: recovery
/// must never follow a planted symlink — validate first, read second).
/// (Đọc file SAU khi chống link — không bao giờ đọc trước.)
/// Read a project file ONLY after refusing link-swaps, and (on unix)
/// opening with O_NOFOLLOW so a final-component swap between the check
/// and the open still fails instead of redirecting the read. Residual
/// window: a parent-dir swap in the same instant by an attacker who can
/// already rewrite project files directly — full dirfd chaining is
/// tracked as hardening follow-up, not a correctness dependency.
/// (Đọc sau khi chống link + O_NOFOLLOW lúc mở.)
fn read_project_file(path: &Path) -> Result<Vec<u8>> {
    refuse_project_link(path)?;
    read_project_bytes(path)
}

/// Open a file for reading with O_NOFOLLOW (unix): a planted
/// final-component symlink fails the open instead of redirecting.
/// (Mở đọc có O_NOFOLLOW.)
#[cfg(unix)]
fn open_nofollow_read(path: &Path) -> Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| anyhow::anyhow!("cannot open '{}': {e:#}", path.display()))
}

#[cfg(unix)]
fn read_project_bytes(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut file = open_nofollow_read(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| anyhow::anyhow!("cannot read '{}': {e:#}", path.display()))?;
    Ok(bytes)
}

#[cfg(not(unix))]
fn read_project_bytes(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| anyhow::anyhow!("cannot read '{}': {e:#}", path.display()))
}

fn read_project_string(path: &Path) -> Result<String> {
    refuse_project_link(path)?;
    read_project_text(path)
}

#[cfg(unix)]
fn read_project_text(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut content = String::new();
    open_nofollow_read(path)?
        .read_to_string(&mut content)
        .map_err(|e| anyhow::anyhow!("cannot read '{}': {e:#}", path.display()))?;
    Ok(content)
}

#[cfg(not(unix))]
fn read_project_text(path: &Path) -> Result<String> {
    std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read '{}': {e:#}", path.display()))
}

/// Canonical digest of a manifest for restore verification: project
/// name+version plus EVERY dependency with group, range and flags,
/// sorted. Name-only comparison would bless a restore that changed
/// versions, groups or project fields.
/// (Digest chuẩn manifest: tên+version project, mọi dep kèm group,
/// range, cờ — đã sắp xếp.)
fn manifest_canonical_digest(manifest: &Manifest) -> Vec<String> {
    let mut items = vec![format!(
        "project:{}@{}",
        manifest.name,
        manifest
            .version
            .as_ref()
            .map(|version| version.to_string())
            .unwrap_or_default()
    )];
    let mut deps: Vec<String> = [
        ("dep:", &manifest.dependencies),
        ("dev:", &manifest.dev_dependencies),
        ("peer:", &manifest.peer_dependencies),
        ("opt:", &manifest.optional_dependencies),
    ]
    .into_iter()
    .flat_map(|(group, specs)| {
        specs.iter().map(move |spec| {
            format!(
                "{group}{}@{}{}{}{}",
                spec.name.as_str(),
                spec.range.as_str(),
                if spec.dev { "+dev" } else { "" },
                if spec.optional { "+opt" } else { "" },
                if spec.peer { "+peer" } else { "" },
            )
        })
    })
    .collect();
    deps.sort();
    items.extend(deps);
    items
}

fn mutation_journal_dir(root: &Path) -> std::path::PathBuf {
    root.join(".magicore")
        .join("journal")
        .join("dependency-mutation")
}

fn project_id_path(root: &Path) -> std::path::PathBuf {
    root.join(".magicore").join("project.id")
}

/// Stable per-project identity (P1-2): a random UUID stored in
/// `.magicore/project.id`, created once under the writer lock. Unlike
/// the absolute path it survives renames/moves of the checkout (the
/// file moves WITH the project); unlike nothing, it distinguishes two
/// checkouts that merely share history. Missing or garbage file fails
/// closed — never invented.
/// (UUID project bền vững — move checkout vẫn nhận ra, copy khác UUID.)
fn project_identity_id(root: &Path, _lock: &ProjectWriteLock) -> Result<String> {
    let path = project_id_path(root);
    match std::fs::symlink_metadata(&path) {
        Ok(_) => read_project_identity_id(root),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let magicore = root.join(".magicore");
            refuse_project_link(&magicore)?;
            if std::fs::symlink_metadata(&magicore).is_err() {
                std::fs::create_dir(&magicore)?;
            }
            let id = uuid::Uuid::new_v4().to_string();
            atomic_write_file(&magicore, "project.id", id.as_bytes())?;
            Ok(id)
        }
        Err(e) => Err(anyhow::anyhow!(
            "cannot stat project identity '{}': {e:#}",
            path.display()
        )),
    }
}

/// Read-only twin: never creates — recovery must not mutate before it
/// has verified anything.
/// (Chỉ đọc — recovery không tạo gì trước khi verify.)
fn read_project_identity_id(root: &Path) -> Result<String> {
    let path = project_id_path(root);
    let raw = read_project_string(&path).map_err(|_| {
        anyhow::anyhow!(
            "missing project identity '{}' alongside a staged journal — the project directory was replaced or the file deleted; refusing to guess (restore the directory or delete '{}' manually)",
            path.display(),
            mutation_journal_dir(root).display(),
        )
    })?;
    let id = raw.trim().to_string();
    uuid::Uuid::parse_str(&id).map_err(|_| {
        anyhow::anyhow!(
            "corrupt project identity '{}' (not a UUID) — refusing to guess",
            path.display()
        )
    })?;
    Ok(id)
}

fn legacy_journal_dir(root: &Path) -> std::path::PathBuf {
    root.join(".magicore").join("journal").join("remove")
}

/// One-time move of a pre-neutral-path journal: under the held writer
/// lock, an atomic rename carries a staged legacy journal to the new
/// location (no copies, no partial states). Runs at every gateway entry
/// before recovery reads the new path.
/// (Dời journal cũ sang path mới — rename nguyên tử dưới lock.)
fn adopt_legacy_journal(root: &Path, _lock: &ProjectWriteLock) -> Result<()> {
    let legacy = legacy_journal_dir(root);
    let legacy_marker = legacy.join("journal.json");
    let current = mutation_journal_dir(root);
    if std::fs::symlink_metadata(&legacy_marker).is_err() {
        return Ok(());
    }
    if std::fs::symlink_metadata(current.join("journal.json")).is_ok() {
        return Err(anyhow::anyhow!(
            "both legacy '{}' and current '{}' journals exist — refusing to merge; resolve manually",
            legacy.display(),
            current.display()
        ));
    }
    refuse_project_link(&legacy)?;
    if let Some(parent) = current.parent() {
        refuse_project_link(parent)?;
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&legacy, &current).map_err(|e| {
        anyhow::anyhow!("cannot adopt legacy journal '{}': {e:#}", legacy.display())
    })?;
    // fsync the parent so the rename survives power loss (durability
    // parity with the atomic file writes). Windows rename replaces only
    // when the destination is absent — checked above, so no
    // copy-and-delete fallback can resurrect a torn state.
    // (fsync cha sau rename — rename bền vững khi mất điện.)
    if let Some(parent) = current.parent() {
        fsync_dir(parent)?;
    }
    Ok(())
}

/// Mutation kind — a CLOSED enum (P1): the parser rejects anything
/// else, so no "installl"/"evil" string can ever steer recovery.
/// (Loại mutation dạng enum đóng — chuỗi lạ bị từ chối.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MutationOperation {
    Add,
    Remove,
    Update,
    Install,
    AuditFix,
    Optimizer,
}

impl MutationOperation {
    fn as_str(self) -> &'static str {
        match self {
            MutationOperation::Add => "add",
            MutationOperation::Remove => "remove",
            MutationOperation::Update => "update",
            MutationOperation::Install => "install",
            MutationOperation::AuditFix => "audit-fix",
            MutationOperation::Optimizer => "optimizer",
        }
    }
}

impl std::fmt::Display for MutationOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Typed, versioned mutation journal (P0: free-form JSON is FORBIDDEN —
/// a "valid but wrong-schema" journal must fail closed with artifacts
/// preserved, never silently delete-and-pretend-completed).
/// Required fields have NO defaults: a missing `lock_existed` or a
/// misspelled `state` is corruption, not "completed". Covers remove AND
/// add/update tails (one transaction protocol for every manifest+lock
/// mutation — a lock alone is serialization, not atomicity).
/// (Journal typed có version cho mọi mutation — sai schema là hỏng.)
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MutationJournal {
    /// Schema version — must be 4 (v4: op enum + full owner identity +
    /// canonical root + stable project UUID; older fail closed — never
    /// public, RC branch only).
    v: u32,
    /// Mutation kind (closed enum — unknown values fail closed).
    op: MutationOperation,
    /// Staging process id — DIAGNOSTIC ONLY, never a recovery authority
    /// (pids get reused after a crash).
    pid: u64,
    packages: Vec<String>,
    /// REQUIRED: whether mgc.lock existed at snapshot time. Absence of
    /// this field must NEVER read as "no lock" (that misread could
    /// delete a real lockfile).
    lock_existed: bool,
    /// Owner identity (P0-3): core + language/framework + manifest
    /// format + relative path. Recovery restores ONLY through an
    /// identical identity — one lane must never rewrite another lane's
    /// manifest, even inside one adapter (bevy vs godot, tf vs cdk).
    /// (Định danh owner đầy đủ — khác lane thì từ chối phục hồi.)
    identity: mgc_types::ManifestIdentity,
    /// Canonicalized project root at stage time (diagnostic + moved
    /// detection messaging; the AUTHORITY is project_id below).
    /// (Root chuẩn để chẩn đoán — quyền quyết định là project_id.)
    project_root: String,
    /// Stable project UUID (P1-2): travels WITH the checkout, so a
    /// legitimately moved/renamed project still recovers, while a
    /// foreign project (different UUID) fails. Absolute paths leak
    /// usernames/build dirs into artifacts and break on every move —
    /// never stable identity.
    /// (UUID project — move vẫn nhận, project lạ thì từ chối.)
    project_id: String,
    /// Canonical manifest digest BEFORE the edit (conflict baseline).
    pre_digest: Vec<String>,
    /// Canonical manifest digest AFTER the edit (None until the write
    /// lands). Recovery restores pre only when current matches pre or
    /// post — anything else means newer user edits that must NOT be
    /// overwritten.
    post_digest: Option<Vec<String>>,
    state: JournalState,
}

/// Journal lifecycle: only InProgress journals from another process may
/// restore; Completed journals are delete-only.
/// (Chỉ InProgress của pid khác mới được restore.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum JournalState {
    InProgress,
    Completed,
}

const MUTATION_JOURNAL_SCHEMA: u32 = 4;

fn parse_mutation_journal(raw: &str, journal_path: &Path) -> Result<MutationJournal> {
    // Version gate FIRST (P1-3): a schema mismatch fails with an
    // explicit version error before field-level parsing can mislead
    // (missing-field errors would hide "wrong schema" from operators).
    // (Cổng version trước — lỗi schema rõ ràng.)
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
        anyhow::anyhow!(
            "corrupt mutation journal '{}': {e:#} (artifacts preserved — refusing to guess; inspect or delete it manually)",
            journal_path.display()
        )
    })?;
    let version = value.get("v").and_then(|v| v.as_u64());
    if version != Some(u64::from(MUTATION_JOURNAL_SCHEMA)) {
        return Err(anyhow::anyhow!(
            "unsupported mutation journal schema v{} in '{}' (this binary reads v{MUTATION_JOURNAL_SCHEMA}; artifacts preserved — inspect or delete it manually)",
            version
                .map(|v| v.to_string())
                .unwrap_or_else(|| "missing".to_string()),
            journal_path.display()
        ));
    }
    serde_json::from_value(value).map_err(|e| {
        anyhow::anyhow!(
            "corrupt mutation journal '{}': {e:#} (artifacts preserved — refusing to guess; inspect or delete it manually)",
            journal_path.display()
        )
    })
}
/// create_new temp (never overwrite blindly), O_NOFOLLOW on unix, fsync
/// file + parent dir, atomic rename. Mirrors mgc-lockfile atomic semantics.
/// (Ghi file nguyên tử + bền: chống symlink, tmp create_new, fsync.)
fn atomic_write_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    atomic_write_file_checked(dir, name, bytes, true)
}

/// Checked variant: `check_dir_link` refuses a swapped parent directory.
/// Journal paths always check; the project root itself may legitimately
/// be a symlinked checkout, so direct project-file writes skip the dir
/// check (the temp file still gets O_NOFOLLOW + create_new, the commit
/// is still an atomic rename).
/// (Biến thể có/không chống link thư mục cha.)
fn atomic_write_file_checked(
    dir: &Path,
    name: &str,
    bytes: &[u8],
    check_dir_link: bool,
) -> Result<()> {
    if check_dir_link {
        refuse_project_link(dir)?;
    }
    let dest = dir.join(name);
    refuse_project_link(&dest)?;
    let tmp = dir.join(format!(
        "{name}.tmp.{}.{}.bak",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0),
    ));
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options.open(&tmp).map_err(|e| {
            anyhow::anyhow!("cannot create journal temp '{}': {e:#}", tmp.display())
        })?;
        use std::io::Write;
        file.write_all(bytes)
            .map_err(|e| anyhow::anyhow!("cannot write journal temp '{}': {e:#}", tmp.display()))?;
        file.sync_all()
            .map_err(|e| anyhow::anyhow!("cannot fsync journal temp '{}': {e:#}", tmp.display()))?;
    }
    std::fs::rename(&tmp, &dest)?;
    fsync_dir(dir)?;
    Ok(())
}

/// fsync a directory so renames inside it survive power loss.
/// (fsync thư mục — rename bền vững khi mất điện.)
fn fsync_dir(dir: &Path) -> Result<()> {
    match std::fs::File::open(dir) {
        Ok(handle) => handle
            .sync_all()
            .map_err(|e| anyhow::anyhow!("cannot fsync dir '{}': {e:#}", dir.display())),
        // Windows cannot open directories: durability there relies on
        // file fsync + atomic rename; refusal still applies.
        // (Windows không mở được thư mục — bền vững nhờ fsync file.)
        Err(_) => Ok(()),
    }
}

/// Run one post-stage mutation step with correctly ordered failpoints:
/// before-check, work, after-check — ANY error rolls the op back to its
/// pre-image instead of leaking a staged journal. Phase names mean what
/// they say: `before-manifest-write` fires BEFORE the write,
/// `after-manifest-write` fires AFTER it (proving rollback of a
/// completed write, not a skipped one).
/// (Failpoint đúng thứ tự before/work/after — tên đúng nghĩa.)
pub(crate) async fn journaled_step<E>(
    adapter: &dyn PackageAdapter,
    root: &Path,
    snapshot: &MutationSnapshot,
    lock: &ProjectWriteLock,
    before_phase: &str,
    work: impl std::future::Future<Output = Result<(), E>>,
    after_phase: &str,
) -> Result<()>
where
    E: std::fmt::Display,
{
    if let Err(e) = mutation_failpoint(before_phase) {
        return rollback_mutation(adapter, root, snapshot, e, lock).await;
    }
    if let Err(e) = work.await {
        return rollback_mutation(adapter, root, snapshot, e, lock).await;
    }
    if let Err(e) = mutation_failpoint(after_phase) {
        return rollback_mutation(adapter, root, snapshot, e, lock).await;
    }
    Ok(())
}

/// Test-only fault injection (precedent: MGC_LOCK_FAILPOINT): when
/// `MGC_MUTATION_FAILPOINT` names a phase, that phase fails with an
/// injected error so E2E can prove every post-stage path rolls back.
/// Production cost is one env read when unset; never set it outside
/// tests. Phases: before-manifest-write, after-manifest-write,
/// before-post-image, after-post-image, before-complete, after-complete,
/// before-cleanup.
/// (Hook lỗi chỉ-cho-test — tên đúng thứ tự before/work/after.)
pub(crate) fn mutation_failpoint(phase: &str) -> Result<()> {
    let target = std::env::var("MGC_MUTATION_FAILPOINT")
        .ok()
        .filter(|value| !value.trim().is_empty());
    if target.as_deref() == Some(phase) {
        return Err(anyhow::anyhow!(
            "injected fault at mutation phase '{phase}' (MGC_MUTATION_FAILPOINT)"
        ));
    }
    Ok(())
}

fn remove_journal_file(dir: &Path, name: &str) -> Result<()> {
    match std::fs::remove_file(dir.join(name)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "cannot remove journal file '{name}': {e:#}"
        )),
    }
}

/// Stage the crash journal (commit point is the manifest write AFTER
/// this returns): refuse swapped paths, drop pre-commit garbage from an
/// older crash (journal.json absent ⇒ nothing was ever mutated), then
/// lock backup, manifest backup, then journal.json LAST with
/// state=in_progress + pre-edit digest — recovery only acts on a parsed
/// in_progress journal, so a crash mid-stage provably mutated nothing
/// yet. A leftover journal.json here means a NESTED mutation in this
/// process — fail closed instead of destroying the outer op's backups.
/// (Ghi journal bền vững cho mọi op; journal cũ còn marker là lồng nhau.)
pub(crate) fn stage_mutation_journal(
    root: &Path,
    adapter: &dyn PackageAdapter,
    packages: &[String],
    snapshot: &MutationSnapshot,
    _lock: &ProjectWriteLock,
) -> Result<()> {
    let dir = mutation_journal_dir(root);
    for parent in [
        root.join(".magicore"),
        root.join(".magicore").join("journal"),
    ] {
        refuse_project_link(&parent)?;
    }
    std::fs::create_dir_all(&dir)?;
    refuse_project_link(&dir)?;
    if dir.join("journal.json").exists() {
        return Err(anyhow::anyhow!(
            "nested mutation detected: '{}' already staged (same-process re-entry would destroy the outer backup)",
            dir.join("journal.json").display()
        ));
    }
    // Pre-commit garbage from a crash before any journal.json existed.
    // (Rác tiền-commit của lần crash trước — xóa an toàn.)
    remove_journal_file(&dir, "manifest.json")?;
    remove_journal_file(&dir, "mgc.lock.bak")?;
    if let Some(bytes) = &snapshot.lock_bytes {
        atomic_write_file(&dir, "mgc.lock.bak", bytes)?;
    }
    let manifest_json = serde_json::to_string_pretty(&snapshot.manifest)?;
    atomic_write_file(&dir, "manifest.json", manifest_json.as_bytes())?;
    // No permissive default (Blocker 1): lanes without a natively-owned
    // manifest report no identity — staging for them fails closed here
    // instead of silently journaling under a coarse adapter name.
    // (Không identity thì không stage — fail-closed.)
    let identity = adapter.manifest_identity().ok_or_else(|| {
        anyhow::anyhow!(
            "cannot journal a '{}' mutation: the '{}' lane claims no natively-owned manifest (delegated/scaffold lanes are not journaled)",
            snapshot.op,
            adapter.name(),
        )
    })?;
    let journal = MutationJournal {
        v: MUTATION_JOURNAL_SCHEMA,
        op: snapshot.op,
        pid: u64::from(std::process::id()),
        packages: packages.to_vec(),
        lock_existed: snapshot.lock_bytes.is_some(),
        identity,
        project_root: root
            .canonicalize()
            .map(|root| root.display().to_string())
            .unwrap_or_else(|_| root.display().to_string()),
        project_id: project_identity_id(root, _lock)?,
        pre_digest: manifest_canonical_digest(&snapshot.manifest),
        post_digest: None,
        state: JournalState::InProgress,
    };
    atomic_write_file(
        &dir,
        "journal.json",
        serde_json::to_string_pretty(&journal)?.as_bytes(),
    )?;
    Ok(())
}

/// Record the post-edit image after the manifest write lands: recovery
/// can then tell "crash before write" (current == pre), "crash after
/// write" (current == post) and "someone else mutated since" (neither —
/// fail closed, never overwrite user edits).
/// (Ghi post-image sau khi sửa manifest — recovery phân biệt 3 trạng thái.)
pub(crate) fn record_post_image(
    root: &Path,
    manifest: &Manifest,
    _lock: &ProjectWriteLock,
) -> Result<()> {
    let dir = mutation_journal_dir(root);
    let journal_path = dir.join("journal.json");
    let mut journal = parse_mutation_journal(&read_project_string(&journal_path)?, &journal_path)?;
    journal.post_digest = Some(manifest_canonical_digest(manifest));
    atomic_write_file(
        &dir,
        "journal.json",
        serde_json::to_string_pretty(&journal)?.as_bytes(),
    )?;
    Ok(())
}

/// Flip a journal to completed (durable rewrite) BEFORE deleting it: a
/// crash between flip and delete leaves a completed journal, which
/// recovery only DELETES — stale cleanup never regains rollback rights
/// (P0: a leftover journal must not resurrect a successful remove).
/// (Đánh dấu completed TRƯỚC khi xóa — dọn sót không được rollback.)
fn mark_journal_completed(root: &Path, _lock: &ProjectWriteLock) -> Result<()> {
    mutation_failpoint("before-complete")?;
    let dir = mutation_journal_dir(root);
    let journal_path = dir.join("journal.json");
    // A missing journal here is ALWAYS a bug (stage runs before any
    // finish/mark caller) — say so explicitly instead of a bare ENOENT.
    // (Journal mất lúc disarm là bug — báo rõ.)
    if std::fs::symlink_metadata(&journal_path).is_err() {
        return Err(anyhow::anyhow!(
            "mutation journal missing at disarm time '{}' (stage was skipped — internal bug, report it)",
            journal_path.display()
        ));
    }
    let mut journal = parse_mutation_journal(&read_project_string(&journal_path)?, &journal_path)?;
    journal.state = JournalState::Completed;
    atomic_write_file(
        &dir,
        "journal.json",
        serde_json::to_string_pretty(&journal)?.as_bytes(),
    )?;
    mutation_failpoint("after-complete")?;
    Ok(())
}

/// Disarm (mark completed) then clear the journal on the success path.
/// A mark failure fails the op LOUDLY: leaving an in_progress journal
/// behind a success would let the next run roll back a finished op.
/// (Vô hiệu rồi xóa khi thành công — đánh dấu lỗi thì báo lỗi.)
pub(crate) fn finish_mutation_journal(root: &Path, lock: &ProjectWriteLock) -> Result<()> {
    mark_journal_completed(root, lock)?;
    clear_mutation_journal(root, lock);
    Ok(())
}

/// Best-effort journal-dir deletion, only ever called AFTER
/// mark_journal_completed: a leftover dir is state=completed and can
/// never trigger a restore — delete failure degrades to a warning plus
/// a same-state delete retry next run, never a rollback.
/// (Chỉ xóa sau khi completed — sót cũng không rollback.)
fn clear_mutation_journal(root: &Path, _lock: &ProjectWriteLock) {
    // Fault-injection point: simulate a cleanup failure (completed
    // journal lingers; the next run deletes it — never a rollback).
    // (Điểm lỗi cleanup — journal completed sót, lần sau xóa.)
    if mutation_failpoint("before-cleanup").is_err() {
        mgc_ui::warning(
            "injected fault at mutation phase 'before-cleanup' (MGC_MUTATION_FAILPOINT)",
        );
        return;
    }
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Err(e) = std::fs::remove_dir_all(mutation_journal_dir(root))
        && e.kind() != std::io::ErrorKind::NotFound
    {
        mgc_ui::warning(&format!("mutation journal cleanup failed: {e:#}"));
    }
}

/// Restore manifest + lock from a snapshot, VERIFIED: manifest by
/// canonical-digest equality after re-parse (project name+version, every
/// dependency with group+range+flags — never name-only), lock
/// byte-identical via the locked atomic writer (or absence restored).
/// Any mismatch is an error — a silent almost-restore is worse than a
/// loud failure.
/// (Phục hồi CÓ VERIFY: manifest digest chuẩn, lock nguyên tử đúng byte.)
async fn restore_mutation_snapshot(
    adapter: &dyn PackageAdapter,
    root: &Path,
    snapshot: &MutationSnapshot,
    lock: &ProjectWriteLock,
) -> Result<()> {
    adapter.write_manifest(root, &snapshot.manifest).await?;
    let reparsed = adapter.parse_manifest(root).await?;
    if manifest_canonical_digest(&reparsed) != manifest_canonical_digest(&snapshot.manifest) {
        return Err(anyhow::anyhow!(
            "manifest restore verify failed: re-parsed digest {:?} != snapshot {:?}",
            manifest_canonical_digest(&reparsed),
            manifest_canonical_digest(&snapshot.manifest)
        ));
    }
    let lock_path = root.join("mgc.lock");
    match &snapshot.lock_bytes {
        Some(bytes) => {
            mgc_lockfile::atomic::atomic_write_locked(
                lock,
                &lock_path,
                bytes,
                std::time::Duration::from_secs(60),
            )
            .map_err(|e| anyhow::anyhow!("lock restore failed: {e}"))?;
            let back = std::fs::read(&lock_path)?;
            if back != *bytes {
                return Err(anyhow::anyhow!(
                    "lock restore verify failed: re-read bytes differ from snapshot"
                ));
            }
        }
        None => {
            refuse_project_link(&lock_path)?;
            if lock_path.exists() {
                std::fs::remove_file(&lock_path)?;
            }
            if lock_path.exists() {
                return Err(anyhow::anyhow!(
                    "lock restore verify failed: lock should be absent but still exists"
                ));
            }
        }
    }
    Ok(())
}

/// Recover a mutation interrupted by a crash. Authority is the LOCK +
/// journal STATE, never the on-disk pid (pids get reused after a crash;
/// the pid field is diagnostic only). Only pre/post manifest states may
/// be restored — anything else fails closed instead of overwriting newer
/// edits. Completed journals are only deleted, corrupt ones error out.
/// (Phục hồi: tin lock+state, không tin pid; manifest lạ thì lỗi.)
async fn recover_interrupted_remove(
    adapter: &dyn PackageAdapter,
    root: &Path,
    lock: &ProjectWriteLock,
) -> Result<()> {
    let dir = mutation_journal_dir(root);
    let journal_path = dir.join("journal.json");
    // Existence probe WITHOUT following links: truly absent ⇒ no-op.
    // A present-but-unreadable path (dangling link, permissions, race)
    // falls through to the refusing read below, which fails closed.
    // (Probe không theo link: không có thì thôi; còn lại đọc có chống.)
    match std::fs::symlink_metadata(&journal_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(anyhow::anyhow!(
                "cannot stat remove journal '{}': {e:#}",
                journal_path.display()
            ));
        }
        Ok(_) => {}
    }
    let raw = read_project_string(&journal_path)?;
    let journal = parse_mutation_journal(&raw, &journal_path)?;
    if journal.state != JournalState::InProgress {
        clear_mutation_journal(root, lock);
        return Ok(());
    }
    // Owner + project identity (Blocker 1): the running adapter must BE
    // the journal's owner — full ManifestIdentity equality (core,
    // language, format, relpath), not just the adapter name, so two
    // frameworks sharing one adapter (bevy vs godot, tf vs cdk) still
    // mismatch. Project lineage is the stable UUID (P1-2): a moved or
    // renamed checkout keeps `.magicore/project.id` and recovers (with
    // a notice); a foreign project carries a different UUID and fails.
    // Anything else fails with remediation instead of rewriting a
    // foreign manifest; the current adapter is never taken as authority
    // over a mismatched journal.
    // (Đúng identity + UUID mới được phục hồi — move thì báo, lạ thì lỗi.)
    let current_root = root
        .canonicalize()
        .map(|root| root.display().to_string())
        .unwrap_or_else(|_| root.display().to_string());
    let current_id = read_project_identity_id(root)?;
    let current_identity = adapter.manifest_identity();
    let identity_matches = match (&current_identity, &journal.identity) {
        (Some(current), staged) => current == staged,
        (None, _) => false,
    };
    if current_id != journal.project_id || !identity_matches {
        let current_desc = current_identity
            .map(|identity| {
                format!(
                    "{}/{}/{}/{}",
                    identity.core, identity.language, identity.format, identity.relpath
                )
            })
            .unwrap_or_else(|| format!("{} (no native manifest identity)", adapter.name()));
        let staged = &journal.identity;
        return Err(anyhow::anyhow!(
            "mutation journal belongs to '{}' ({}/{}/{}) of project '{}' (id '{}') but the current command runs '{}' of project '{}' (id '{}') — refusing to restore a foreign manifest (re-run the original '{}' op in its own project, or delete '{}' manually)",
            staged.core,
            staged.language,
            staged.format,
            staged.relpath,
            journal.project_root,
            journal.project_id,
            current_desc,
            current_root,
            current_id,
            journal.op,
            journal_path.display(),
        ));
    }
    if current_root != journal.project_root {
        mgc_ui::warning(&format!(
            "project directory moved since the journal was staged ('{}' → '{}'); same project UUID, continuing recovery",
            journal.project_root, current_root,
        ));
    }
    // No in-memory skip: at gateway time this process holds the lock and
    // has staged nothing, so ANY in_progress journal is stale by
    // construction (own tails use `_locked` and never re-enter). Same-
    // process nesting blocks on the OS lock first and fails LockBusy —
    // it can never reach recovery with a live outer journal.
    // (Không bỏ qua theo cờ: journal gặp ở đây chắc chắn là stale.)
    let manifest_raw = read_project_string(&dir.join("manifest.json"))?;
    let manifest: Manifest = serde_json::from_str(&manifest_raw).map_err(|e| {
        anyhow::anyhow!(
            "corrupt mutation journal backup '{}': {e:#} (artifacts preserved — refusing to guess)",
            dir.join("manifest.json").display()
        )
    })?;
    let lock_bytes = if journal.lock_existed {
        Some(read_project_file(&dir.join("mgc.lock.bak"))?)
    } else {
        None
    };
    // Conflict detection: only pre/post states may be restored. A
    // current digest matching NEITHER means newer edits landed after the
    // crash (user edit, another tool) — overwriting them would destroy
    // work, so fail closed and ask for manual resolution.
    // (Manifest hiện lạ → có sửa đổi mới hơn — lỗi, không ghi đè.)
    let current = manifest_canonical_digest(&adapter.parse_manifest(root).await?);
    let at_pre = current == manifest_canonical_digest(&manifest);
    let at_post = journal
        .post_digest
        .as_ref()
        .is_some_and(|post| current == *post);
    if !at_pre && !at_post {
        return Err(anyhow::anyhow!(
            "interrupted '{}' op left a journal, but the current manifest matches neither its pre- nor post-image (newer edits since the crash?) — refusing to overwrite; resolve manually (journal at '{}')",
            journal.op.as_str(),
            dir.display(),
        ));
    }
    let snapshot = MutationSnapshot {
        manifest,
        lock_bytes,
        op: journal.op,
    };
    restore_mutation_snapshot(adapter, root, &snapshot, lock).await?;
    mark_journal_completed(root, lock)?;
    clear_mutation_journal(root, lock);
    mgc_ui::warning(
        "recovered an interrupted mutation (crash journal): manifest and lock restored — re-run the command if needed",
    );
    Ok(())
}

/// Roll a failed mutation tail back: restore manifest + lock from the
/// pre-edit snapshot (verified), disarm + clear the journal, and return
/// the tail error. If the restore itself fails the journal is KEPT for
/// the next run's recovery, and BOTH errors propagate in one combined
/// error (warning-only would hide a half-updated project).
/// Honest scope: manifest+lock files are restored; store/cache additions
/// made before the failure are additive-only and stay.
/// (Rollback manifest+lock CÓ VERIFY cho mọi op; restore lỗi thì GIỮ
/// journal + gộp cả 2 lỗi trả về.)
pub(crate) async fn rollback_mutation(
    adapter: &dyn PackageAdapter,
    root: &Path,
    snapshot: &MutationSnapshot,
    tail_error: impl std::fmt::Display,
    lock: &ProjectWriteLock,
) -> Result<()> {
    let op = snapshot.op;
    match restore_mutation_snapshot(adapter, root, snapshot, lock).await {
        Ok(()) => {
            // Disarm BEFORE clearing: even if the delete below fails or
            // the process dies here, the journal is completed and can
            // never roll back again.
            // (Vô hiệu TRƯỚC khi xóa — journal completed không rollback.)
            if let Err(e) = mark_journal_completed(root, lock) {
                return Err(combine_rollback_errors(
                    tail_error,
                    format!("{op} rollback restored files but disarming the journal failed: {e:#}"),
                ));
            }
            clear_mutation_journal(root, lock);
            mgc_ui::info(&format!(
                "{op} rolled back: manifest and lock restored (tail failed; store/cache additions, if any, are additive-only)",
            ));
            Err(anyhow::anyhow!("{tail_error:#}"))
        }
        Err(restore_error) => {
            // Journal is KEPT (not cleared) so the next run's recovery can
            // retry the restore — point the operator at it.
            // (GIỮ journal để lần chạy sau phục hồi — chỉ rõ đường dẫn.)
            let journal = mutation_journal_dir(root);
            Err(combine_rollback_errors(
                tail_error,
                format!(
                    "{restore_error:#} (crash journal kept at '{}' — fix the cause, then re-run to recover)",
                    journal.display()
                ),
            ))
        }
    }
}

/// Combine the install failure with a failed manifest restore into ONE
/// error carrying both (P0: a warning-only restore failure is invisible
/// to CI/API callers). Pure — unit-tested.
/// (Gộp 2 lỗi thành một — hàm thuần, có unit test.)
fn combine_rollback_errors(
    install_error: impl std::fmt::Display,
    restore_error: impl std::fmt::Display,
) -> anyhow::Error {
    anyhow::anyhow!(
        "mutation failed: {install_error:#} — AND the manifest rollback also failed: {restore_error:#} \
         (project may be half-updated: manifest dropped the dependency while lock/store still carry it — \
         re-add the package and retry)"
    )
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
/// Native update: re-resolve each target to latest (star range through
/// prepare_add — the same resolve-first path as add), rewrite the pins
/// mgc-side, then run the native install tail. Unknown package names
/// fail loudly (never silently skipped); unchanged versions are reported
/// and skipped without rewriting.
/// (Update native: resolve latest + viết lại pin + install.)
/// Direct-lane entry: enters the mutation gateway, then runs the locked
/// body. Lane files MUST call this (never self-acquire: forgetting
/// recovery is a bug).
/// (Lane gọi hàm này — gateway nằm trong, không tự acquire.)
pub(crate) async fn native_update(
    adapter: &dyn PackageAdapter,
    root: &Path,
    packages: Vec<String>,
    install: bool,
) -> Result<()> {
    let write_lock = begin_dependency_mutation(adapter, root, MutationOperation::Update).await?;
    native_update_locked(adapter, root, packages, install, &write_lock).await
}

pub(crate) async fn native_update_locked(
    adapter: &dyn PackageAdapter,
    root: &Path,
    packages: Vec<String>,
    install: bool,
    write_lock: &ProjectWriteLock,
) -> Result<()> {
    let mut manifest = adapter.parse_manifest(root).await?;
    // Pre-image for the update journal (P0-3): staged only when mgc
    // owns the manifest rewrite (toolchain-owned files stay in the
    // tool's transactional domain).
    // (Snapshot pre-image cho journal update.)
    let update_snapshot =
        MutationSnapshot::capture(&manifest, root, write_lock, MutationOperation::Update)?;
    let targets: Vec<(String, String, bool, bool, bool)> = if packages.is_empty() {
        manifest
            .all_dependencies()
            .map(|d| {
                let current = d
                    .range
                    .satisfying_version()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| d.range.to_string());
                (
                    d.name.as_str().to_string(),
                    current,
                    d.dev,
                    d.optional,
                    d.peer,
                )
            })
            .collect()
    } else {
        let mut out = Vec::new();
        for name in &packages {
            let Some(dep) = manifest.find_dep(name) else {
                return Err(crate::error::update_unknown_package(name));
            };
            let current = dep
                .range
                .satisfying_version()
                .map(|v| v.to_string())
                .unwrap_or_else(|| dep.range.to_string());
            out.push((
                dep.name.as_str().to_string(),
                current,
                dep.dev,
                dep.optional,
                dep.peer,
            ));
        }
        out
    };
    if targets.is_empty() {
        info("No dependencies to update.");
        return Ok(());
    }
    let mut updated: Vec<mgc_types::adapter::UpdatedPackage> = Vec::new();
    for (name, from, dev, optional, peer) in &targets {
        let pkg_name = PackageName::new(name)?;
        let spinner = create_spinner(&format!("  Updating {}...", name));
        let prepared = adapter
            .prepare_add(
                root,
                &pkg_name,
                None,
                AddOptions {
                    dev: *dev,
                    optional: *optional,
                    peer: *peer,
                    ..Default::default()
                },
            )
            .await?;
        spinner.finish_and_clear();
        let to = prepared.id.version().to_string();
        if to == *from {
            info(&format!("  {name} already latest ({from})"));
            continue;
        }
        let mut spec = DependencySpec::new(pkg_name, prepared.range);
        spec.dev = *dev;
        spec.optional = *optional;
        spec.peer = *peer;
        manifest.add_dep(spec, *dev, *optional, *peer);
        updated.push(mgc_types::adapter::UpdatedPackage {
            name: name.clone(),
            from_version: from.clone(),
            to_version: to,
        });
    }
    if updated.is_empty() {
        info("All packages are up to date");
        return Ok(());
    }
    let update_journaled = if adapter.manifest_owned() {
        stage_mutation_journal(root, adapter, &packages, &update_snapshot, write_lock)?;
        true
    } else {
        false
    };
    if update_journaled {
        journaled_step(
            adapter,
            root,
            &update_snapshot,
            write_lock,
            "before-manifest-write",
            adapter.write_manifest(root, &manifest),
            "after-manifest-write",
        )
        .await?;
        journaled_step(
            adapter,
            root,
            &update_snapshot,
            write_lock,
            "before-post-image",
            async { record_post_image(root, &manifest, write_lock) },
            "after-post-image",
        )
        .await?;
    } else {
        adapter.write_manifest(root, &manifest).await?;
    }
    for pkg in &updated {
        info(&format!(
            "  {}: {} → {}",
            pkg.name, pkg.from_version, pkg.to_version
        ));
    }
    success(&format!("Updated {} package(s)", updated.len()));
    if install {
        info("Installing updated packages...");
        match install_with_adapter_locked(
            adapter,
            root,
            install_command_for_adapter(adapter),
            false,
            mgc_types::adapter::InstallOptions {
                incremental: true,
                ..Default::default()
            },
            write_lock,
        )
        .await
        {
            Ok(()) => {
                if update_journaled && let Err(e) = finish_mutation_journal(root, write_lock) {
                    return rollback_mutation(adapter, root, &update_snapshot, e, write_lock).await;
                }
            }
            Err(e) => {
                if update_journaled {
                    return rollback_mutation(adapter, root, &update_snapshot, e, write_lock).await;
                }
                return Err(e);
            }
        }
    } else {
        if update_journaled && let Err(e) = finish_mutation_journal(root, write_lock) {
            return rollback_mutation(adapter, root, &update_snapshot, e, write_lock).await;
        }
        info(&format!(
            "Run '{}' to install updates",
            style_cmd(install_command_for_adapter(adapter))
        ));
    }
    Ok(())
}

pub async fn update(
    adapter: &dyn PackageAdapter,
    root: &Path,
    packages: Vec<String>,
    install: bool,
) -> Result<()> {
    // Mutation gateway (P0): same contract as add/remove — the manifest
    // writes below (native or toolchain-spawned) and the install tails
    // run under one writer lock, AFTER stale-journal recovery.
    // (Update qua gateway — lock + recovery trước mọi mutation.)
    let write_lock = begin_dependency_mutation(adapter, root, MutationOperation::Update).await?;
    // Native update (resolve-latest + mgc-side manifest edit + native
    // install tail, zero spawn) for adapters that own the whole lane.
    // Legacy adapter.update (toolchain spawn) below stays for the rest.
    // (Update native cho adapter sở hữu lane.)
    if adapter.supports_native_update() {
        return native_update_locked(adapter, root, packages, install, &write_lock).await;
    }
    if packages.is_empty() {
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
                install_with_adapter_locked(
                    adapter,
                    root,
                    install_command_for_adapter(adapter),
                    false,
                    mgc_types::adapter::InstallOptions {
                        incremental: true,
                        ..Default::default()
                    },
                    &write_lock,
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
            install_with_adapter_locked(
                adapter,
                root,
                install_command_for_adapter(adapter),
                false,
                mgc_types::adapter::InstallOptions {
                    incremental: true,
                    ..Default::default()
                },
                &write_lock,
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
    // Every standalone install runs under the writer lock (P0): the
    // crash-journal recovery below must never race a concurrent remove.
    // Callers already holding the guard (mutation tails) use the _locked
    // variant — the OS lock is not same-process re-entrant. Recovery runs
    // inside, so installs never build on unrestored state.
    // (Install độc lập qua gateway — lock + recovery.)
    let write_lock = begin_dependency_mutation(adapter, root, MutationOperation::Install).await?;
    install_with_adapter_locked(adapter, root, add_cmd, frozen, opts, &write_lock).await
}

async fn install_with_adapter_locked(
    adapter: &dyn PackageAdapter,
    root: &Path,
    add_cmd: &str,
    frozen: bool,
    opts: mgc_types::adapter::InstallOptions,
    // Held alive by the caller across the whole tail (the borrow keeps
    // the guard — and the OS lock — from releasing mid-flight).
    // (Giữ guard sống xuyên tail.)
    _write_lock: &ProjectWriteLock,
) -> Result<()> {
    // NO recovery here by design: every caller either entered through
    // begin_dependency_mutation (recovery already ran) or IS the tail of
    // an op with a live staged journal — recovering now would "restore"
    // the outer op's own in-progress state mid-flight (the exact
    // self-recovery corruption). Stale journals are only ever handled at
    // gateway entries holding a fresh lock with nothing staged.
    // (Tail KHÔNG recovery — journal đang mở là của op ngoài.)
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
    // Value-based like the audit command's StrictMode (an explicitly set
    // "0" means open — presence alone must never arm blocking, or test
    // and operator opt-outs become lies).
    // (Theo giá trị như audit command — "0" là mở, presence không đủ.)
    let strict_armed = std::env::var("MGC_AUDIT_STRICT").ok().is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    });
    if !strict_armed || graph.packages.is_empty() {
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

/// Single lock-vs-manifest matcher (any-match): multi-version locks are
/// legitimate (a peer edge may resolve another version), so the manifest
/// range passes when ANY same-named instance satisfies it — first-match
/// order must never decide. All install/remove/frozen paths share this
/// one function so the predicate cannot drift between copies.
/// (Matcher duy nhất — mọi đường install/remove/frozen dùng chung.)
pub(crate) fn lock_matches_manifest(lock: &Lockfile, manifest: &Manifest) -> bool {
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
    game_hook_optimizer_dep(root).await
}

/// game: thêm dep path `mgc-optimizer = { path = "./optimizer" }` vào root Cargo.toml (bevy only).
#[cfg(feature = "game")]
async fn game_hook_optimizer_dep(root: &Path) -> Result<()> {
    // Serialized idempotent edit: the mgc-optimizer path dep is
    // insert-if-missing with VALUE verification (an identical key with a
    // different value is a user conflict, not a skip), written atomically
    // (tmp + rename, no torn file). Enters the mutation gateway FIRST so
    // a stale journal from an earlier crashed op is recovered-or-refused
    // before touching Cargo.toml (P0-3-adjacent). No journal of its own:
    // both crash outcomes (pre-write / post-write) are valid states and
    // re-running converges, so there is nothing to roll back to.
    // (Sửa idempotent qua gateway + verify giá trị + atomic.)
    let manifest = root.join("Cargo.toml");
    if !manifest.exists() {
        return Ok(());
    }
    let Some(adapter) = mgc_game_adapter::adapter_for(root) else {
        return Err(anyhow::anyhow!(
            "game optimizer hook refused: '{}' is not a detected game project",
            root.display()
        ));
    };
    let _guard = begin_dependency_mutation(&adapter, root, MutationOperation::Optimizer).await?;
    refuse_project_link(&manifest)?;
    let content = std::fs::read_to_string(&manifest)?;
    let mut v: toml::Value = toml::from_str(&content)?;
    let deps = v["dependencies"]
        .as_table_mut()
        .ok_or_else(crate::error::cargo_toml_no_deps)?;
    match deps.get("mgc-optimizer") {
        Some(existing)
            if existing.get("path").and_then(|path| path.as_str()) == Some("./optimizer") =>
        {
            return Ok(());
        }
        Some(existing) => {
            let path = existing
                .get("path")
                .and_then(|path| path.as_str())
                .unwrap_or("");
            return Err(anyhow::anyhow!(
                "game optimizer hook refused: Cargo.toml already has 'mgc-optimizer' with a different value ('{path}' != './optimizer') — resolve manually, never overwrite blindly"
            ));
        }
        None => {}
    }
    deps.insert(
        "mgc-optimizer".to_string(),
        toml::Value::Table(toml::map::Map::from_iter([(
            "path".to_string(),
            toml::Value::String("./optimizer".to_string()),
        )])),
    );
    atomic_write_file_checked(
        root,
        "Cargo.toml",
        toml::to_string_pretty(&v)?.as_bytes(),
        false,
    )?;
    // Read-back verify: the file must now carry our exact value.
    // (Đọc lại verify — file phải mang đúng giá trị.)
    let reread: toml::Value = toml::from_str(&std::fs::read_to_string(&manifest)?)?;
    let confirmed = reread
        .get("dependencies")
        .and_then(|deps| deps.get("mgc-optimizer"))
        .and_then(|dep| dep.get("path"))
        .and_then(|path| path.as_str())
        == Some("./optimizer");
    if !confirmed {
        return Err(anyhow::anyhow!(
            "game optimizer hook write did not round-trip (re-read lacks mgc-optimizer = './optimizer') — refusing to report success"
        ));
    }
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
