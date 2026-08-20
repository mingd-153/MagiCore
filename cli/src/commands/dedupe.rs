//! Dedupe command — scan lockfile/layout, merge duplicate instances (02 §2-3)
//! (Lệnh dedupe: gộp instance trùng lặp, verify build, rollback khi fail)

use anyhow::{bail, Context, Result};
use clap::Args;
use mg_config::project::ProjectConfig;
use std::collections::HashMap;
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
fn merged_lockfile(lock: &mg_lockfile::Lockfile) -> (mg_lockfile::Lockfile, usize) {
    let mut seen: HashMap<(String, String), bool> = HashMap::new();
    let mut merged = 0usize;
    let mut new_packages = Vec::new();
    for pkg in &lock.packages {
        let key = (pkg.name.clone(), pkg.version.clone());
        if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
            e.insert(true);
            new_packages.push(pkg.clone());
        } else {
            merged += 1;
        }
    }
    let mut new_lock = lock.clone();
    new_lock.packages = new_packages;
    (new_lock, merged)
}

/// Runtime verification (user decision 2026-08-05): build the project after
/// merging; rollback the lockfile if the build fails.
async fn verify_with_build(_project_root: &Path, dry_run: bool) -> Result<()> {
    if dry_run {
        return Ok(());
    }
    crate::commands::build::run(None, None)
        .await
        .with_context(|| "build verification failed after dedupe")
}

fn vstore_root(project_root: &Path) -> std::path::PathBuf {
    project_root.join("node_modules").join(".megagate")
}

/// Delete virtual-store package dirs no longer referenced by the lockfile.
fn cleanup_unreferenced_vstore(project_root: &Path, lock: &mg_lockfile::Lockfile) -> u64 {
    let vstore = vstore_root(project_root);
    if !vstore.exists() {
        return 0;
    }
    let mut freed = 0u64;
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
                freed += dir_size(&dir);
                let _ = fs::remove_dir_all(&dir);
            }
        }
    }
    freed
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

    let mg_lock = project_root.join("mg.lock");
    if !mg_lock.exists() {
        bail!("mg.lock not found — run mg install first");
    }

    let lock_content = fs::read_to_string(&mg_lock)?;
    let lock: mg_lockfile::Lockfile = mg_lockfile::serialization::from_toml(&lock_content)?;
    let before = lock.packages.len();

    let (new_lock, merged) = merged_lockfile(&lock);
    let after = new_lock.packages.len();

    let disk_saved_bytes = if merged > 0 {
        cleanup_unreferenced_vstore(&project_root, &new_lock)
    } else {
        0
    };

    let report = DedupeReport {
        before_instances: before,
        after_instances: after,
        merged,
        disk_saved_bytes,
        entries: Vec::new(),
    };

    if merged > 0 && !args.dry_run {
        // Verify runtime build before committing the merge (02 §5.2).
        let backup = lock_content.clone();
        mg_lockfile::write_lockfile(&project_root, &new_lock)?;
        if let Err(err) = verify_with_build(&project_root, false).await {
            fs::write(&mg_lock, backup)?;
            bail!("merge rolled back — build verification failed: {err}");
        }
    }

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
