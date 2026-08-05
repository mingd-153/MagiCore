//! mg store — manage the local package store (02 §2.2)
//! (Quản lý store: prune package không còn project nào tham chiếu)

use anyhow::Result;
use clap::Subcommand;
use mg_config::project::ProjectConfig;
use std::path::{Path, PathBuf};

#[derive(Subcommand, Debug, Clone)]
pub enum StoreCmd {
    /// Delete packages not referenced by any project (refcount == 0)
    Prune {
        #[arg(long, help = "report only, do not delete")]
        dry_run: bool,
        #[arg(long, help = "output JSON")]
        json: bool,
    },
    /// Show package store summary
    Status,
}

#[derive(Debug, serde::Serialize)]
pub struct PruneReport {
    pub unreferenced: Vec<String>,
    pub removed: Vec<String>,
    pub removed_bytes: u64,
    pub dry_run: bool,
}

fn store_db_for(project_root: &Path) -> Result<PathBuf> {
    let store_root = project_root
        .join(".megagate")
        .join("cache")
        .join("web");
    Ok(store_root.join("store.db"))
}

fn vstore_root_for(project_root: &Path) -> PathBuf {
    project_root.join("node_modules").join(".megagate")
}

fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
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

fn prune_unreferenced(project_root: &Path, dry_run: bool) -> Result<PruneReport> {
    use mg_store::database::Database;

    let db = Database::open(&store_db_for(project_root)?)?;
    let unreferenced = db.list_unreferenced()?;
    let mut report = PruneReport {
        unreferenced: unreferenced
            .iter()
            .map(|id| id.to_string())
            .collect(),
        removed: Vec::new(),
        removed_bytes: 0,
        dry_run,
    };

    let vstore_root = vstore_root_for(project_root);
    for id in &unreferenced {
        let vstore_dir = vstore_root.join(format!(
            "{}@{}",
            id.name_str().replace('/', "+"),
            id.version()
        ));
        if vstore_dir.exists() {
            report.removed_bytes += dir_size(&vstore_dir);
            if !dry_run {
                std::fs::remove_dir_all(&vstore_dir)?;
            }
        }
        report.removed.push(id.to_string());
        if !dry_run {
            db.remove_package(id)?;
        }
    }
    Ok(report)
}

/// mg store prune — delete unreferenced packages (02 §2.2).
pub async fn run(cmd: StoreCmd) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let project_root = ProjectConfig::find_project_root(&cwd)
        .ok_or_else(|| anyhow::anyhow!("Project root not found — run mg init"))?;

    match cmd {
        StoreCmd::Prune { dry_run, json } => {
            let report = prune_unreferenced(&project_root, dry_run)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Store prune {}: {} unreferenced package(s)",
                    if dry_run { "(dry-run)" } else { "(pruned)" },
                    report.unreferenced.len());
                for removed in &report.removed {
                    println!("  removed {removed}");
                }
                if report.removed_bytes > 0 {
                    println!("  freed {} bytes", report.removed_bytes);
                }
            }
            Ok(())
        }
        StoreCmd::Status => {
            let db = mg_store::database::Database::open(&store_db_for(&project_root)?)?;
            let installed = db.list_installed()?;
            println!("Store status: {} installed package(s)", installed.len());
            println!(
                "virtual store: {}",
                vstore_root_for(&project_root).display()
            );
            Ok(())
        }
    }
}