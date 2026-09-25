//! Dedupe command — scan lockfile/layout, merge duplicate instances (02 §2-3)
//! (Lệnh dedupe: gộp instance trùng lặp, verify build, rollback khi fail)

use anyhow::{Context, Result, bail};
use clap::Args;
use mgc_config::project::ProjectConfig;
use std::fs;
use std::path::Path;

#[derive(Args, Debug, Clone)]
pub struct DedupeArgs {
    #[arg(long, help = "report only, do not apply changes")]
    pub dry_run: bool,
    #[arg(long, help = "prefer latest version over existing instances")]
    pub prefer_latest: bool,
    #[arg(long, help = "output JSON")]
    pub json: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct DedupeReport {
    pub before_instances: usize,
    pub after_instances: usize,
    pub merged: usize,
    pub disk_saved_bytes: u64,
    pub entries: Vec<DedupeEntry>,
    /// GC paths that failed deletion (bytes NOT counted above).
    /// Empty means the GC pass was complete.
    /// (Đường GC lỗi — byte chưa xóa không được cộng.)
    pub gc_failed_paths: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct DedupeEntry {
    pub package: String,
    pub version: String,
    pub instances_before: usize,
    pub instances_after: usize,
    pub action: String,
}

/// Merge duplicate lockfile entries into a merged Lockfile (no-op if none).
fn merged_lockfile(lock: &mgc_lockfile::Lockfile) -> Result<(mgc_lockfile::Lockfile, usize)> {
    // Package name/version alone is not a unified-lock identity: identical
    // names can belong to different cores, ecosystems, registries, sources,
    // or carry distinct graph/target metadata. Deduplicate only exact full
    // serialized records so this GC-oriented command cannot erase ownership.
    // (Chỉ khử bản ghi đầy đủ giống hệt để không làm mất owner/graph/target.)
    let mut seen = std::collections::HashSet::new();
    let mut merged = 0usize;
    let mut new_packages = Vec::new();
    for pkg in &lock.packages {
        let key = toml::to_string(pkg).context("serialize package identity for dedupe")?;
        if seen.insert(key) {
            new_packages.push(pkg.clone());
        } else {
            merged += 1;
        }
    }
    let mut new_lock = lock.clone();
    new_lock.packages = new_packages;
    Ok((new_lock, merged))
}

#[cfg(test)]
#[path = "test/dedupe.rs"]
mod dedupe_tests;

/// Runtime verification (user decision 2026-08-05): build the project after
/// merging; rollback the lockfile if the build fails.
async fn verify_with_build(project_root: &Path, dry_run: bool) -> Result<()> {
    if dry_run {
        return Ok(());
    }
    // Root-bound verify (P1-3): build::run resolves its project from the
    // ambient cwd — verifying a DIFFERENT scope would certify the wrong
    // tree. Fail closed instead of verifying blindly (canonicalized on
    // both sides: /tmp vs /private/tmp must not false-mismatch).
    // (Verify đúng scope — cwd khác thì lỗi chứ không verify mù.)
    let cwd = std::env::current_dir()?;
    let cwd_root = ProjectConfig::find_project_root(&cwd).and_then(|root| root.canonicalize().ok());
    let want_root = project_root.canonicalize().ok();
    if cwd_root.is_none() || cwd_root != want_root {
        return Err(anyhow::anyhow!(
            "dedupe verification refused: process directory '{}' resolves to a different project than '{}' — run dedupe from the project root",
            cwd.display(),
            project_root.display(),
        ));
    }
    crate::commands::build::run(None, None, None)
        .await
        .with_context(|| "build verification failed after dedupe")
}

fn vstore_root(project_root: &Path) -> std::path::PathBuf {
    project_root.join("node_modules").join(".magicore")
}

/// Delete virtual-store package dirs no longer referenced by the lockfile.
fn cleanup_unreferenced_vstore(project_root: &Path, lock: &mgc_lockfile::Lockfile) -> GcReport {
    let vstore = vstore_root(project_root);
    let mut report = GcReport::default();
    if !vstore.exists() {
        return report;
    }
    if let Ok(entries) = fs::read_dir(&vstore) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let in_lock = lock
                .packages
                .iter()
                .any(|pkg| format!("{}@{}", pkg.name.replace('/', "+"), pkg.version) == name);
            if !in_lock {
                // Count bytes ONLY after a successful removal (P1-2):
                // a failed delete must never inflate "freed" telemetry.
                // (Chỉ cộng bytes khi xóa thành công — không bịa số.)
                let bytes = dir_size(&dir);
                match fs::remove_dir_all(&dir) {
                    Ok(()) => {
                        report.deleted += 1;
                        report.bytes_confirmed += bytes;
                    }
                    Err(e) => {
                        report.failed.push(format!("{}: {e}", dir.display()));
                    }
                }
            }
        }
    }
    report
}

/// Honest GC telemetry: confirmed bytes only; failures listed, never
/// counted, never swallowed. A failed GC does not roll back the lock
/// (the lock is already verified) but the report says Partial.
/// (Báo cáo GC trung thực — lỗi liệt kê, không cộng byte chưa xóa.)
#[derive(Debug, Default)]
struct GcReport {
    deleted: usize,
    bytes_confirmed: u64,
    failed: Vec<String>,
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        stack.push(entry.path());
                    } else if let Ok(meta) = entry.metadata() {
                        total += meta.len();
                    }
                }
            }
        }
    }
    total
}

pub async fn run(args: DedupeArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let project_root =
        ProjectConfig::find_project_root(&cwd).ok_or_else(crate::error::project_root_missing)?;

    let mgc_lock = project_root.join("mgc.lock");
    if !mgc_lock.exists() {
        bail!("mgc.lock not found — run mgc install first");
    }

    // Writer lock FIRST (P0-2/P0-3): every authoritative read below
    // happens inside the critical section — no stale-read race, no
    // interleave with concurrent mutations, and no pending journal is
    // paved over (fail closed instead).
    // (Lock trước, đọc sau — không stale-read, không đè journal.)
    let guard = mgc_lockfile::project_lock::ProjectWriteLock::acquire(
        &project_root,
        crate::commands::core::shared::writer_lock_timeout(&project_root),
    )
    .map_err(|e| anyhow::anyhow!("dedupe cannot acquire the project writer lock: {e}"))?;
    // Pending-journal protocol (P0-3): never rewrite the lock over an
    // unrestored mutation — fail closed and point at recovery. (Dedupe
    // has no adapter to restore with, so it refuses instead of paving.)
    // (Không đè lên journal chưa phục hồi — lỗi rõ.)
    crate::commands::core::shared::ensure_no_pending_remove_journal(&project_root, &guard)?;

    let lock_content = fs::read_to_string(&mgc_lock)?;
    let lock: mgc_lockfile::Lockfile = mgc_lockfile::serialization::from_toml(&lock_content)?;
    let before = lock.packages.len();

    let (new_lock, merged) = merged_lockfile(&lock)?;
    let after = new_lock.packages.len();

    // Dry-run performs ZERO mutations: no cleanup, no write, no verify
    // spawn — pure report. (P0-2: dry-run tuyệt đối không sửa gì.)
    // (Dry-run: báo cáo thuần — không cleanup/write/verify.)
    if args.dry_run {
        let report = DedupeReport {
            before_instances: before,
            after_instances: after,
            merged,
            disk_saved_bytes: 0,
            entries: Vec::new(),
            gc_failed_paths: Vec::new(),
        };
        print_report(&args, &report)?;
        return Ok(());
    }

    let mut gc = GcReport::default();
    if merged > 0 {
        // Atomic commit (P0-2): the merged lock swaps in via tmp+rename
        // (never a torn write), verified by a runtime build, and rolled
        // back ATOMICALLY on verification failure. GC of the now
        // unreferenced store runs ONLY after the commit verifies — a
        // failed merge never deletes materialization it might need back.
        // (Commit nguyên tử → verify → mới GC; fail thì rollback nguyên tử.)
        let new_toml = mgc_lockfile::serialization::to_toml(&new_lock)?;
        mgc_lockfile::ensure_lockfile_mutation_allowed(&mgc_lock)?;
        mgc_lockfile::atomic::atomic_write_locked(
            &guard,
            &mgc_lock,
            new_toml.as_bytes(),
            std::time::Duration::from_secs(60),
        )
        .map_err(|e| anyhow::anyhow!("dedupe lock commit failed: {e}"))?;
        if let Err(err) = verify_with_build(&project_root, false).await {
            mgc_lockfile::atomic::atomic_write_locked(
                &guard,
                &mgc_lock,
                lock_content.as_bytes(),
                std::time::Duration::from_secs(60),
            )
            .map_err(|e| anyhow::anyhow!("dedupe lock rollback failed: {e}"))?;
            bail!("merge rolled back — build verification failed: {err}");
        }
        gc = cleanup_unreferenced_vstore(&project_root, &new_lock);
        if !gc.failed.is_empty() {
            mgc_ui::warning(&format!(
                "dedupe GC partial: {} path(s) failed (lock commit already verified — no rollback needed): {}",
                gc.failed.len(),
                gc.failed.join("; "),
            ));
        }
    }

    let report = DedupeReport {
        before_instances: before,
        after_instances: after,
        merged,
        disk_saved_bytes: gc.bytes_confirmed,
        entries: Vec::new(),
        gc_failed_paths: gc.failed,
    };
    print_report(&args, &report)?;
    Ok(())
}

/// Render the dedupe report (shared by dry-run and real runs).
/// (In báo cáo dedupe — chung cho dry-run và chạy thật.)
fn print_report(args: &DedupeArgs, report: &DedupeReport) -> Result<()> {
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Dedupe report:");
        println!("  Before: {} instances", report.before_instances);
        println!("  After:  {} instances", report.after_instances);
        println!("  Merged: {} duplicates", report.merged);
        if report.disk_saved_bytes > 0 {
            println!("  Vstore freed: {} bytes", report.disk_saved_bytes);
        }
        if args.dry_run {
            println!("(dry-run — no changes applied)");
        }
    }

    Ok(())
}
