//! mg store — manage the local package store (02 §2.2)
//! (Quản lý store: prune package không còn project nào tham chiếu)

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use mg_config::project::ProjectConfig;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
    /// Sao lưu store về một thư mục mục tiêu
    Backup {
        /// Đường dẫn thư mục đích (mặc định: ~/.megagate/store_backup_<timestamp>)
        #[arg(long)]
        path: Option<String>,
    },
    /// Phục hồi store từ thư mục sao lưu
    Restore {
        /// Đường dẫn thư mục sao lưu
        #[arg(long)]
        path: String,
    },
}

#[derive(Debug, serde::Serialize)]
pub struct PruneReport {
    pub unreferenced: Vec<String>,
    pub removed: Vec<String>,
    pub removed_bytes: u64,
    pub dry_run: bool,
}

fn store_db_for(project_root: &Path) -> Result<PathBuf> {
    let store_root = project_root.join(".megagate").join("cache").join("web");
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

/// Copy toàn bộ cây thư mục (per-file — không dùng `cp` ngoài, portable).
/// Nguồn không tồn tại → trả Ok (no-op) để backup thiếu vstore vẫn chạy được.
fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dst).with_context(|| format!("create {}", dst.display()))?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_dir(&from, &to)?;
        } else if ft.is_file() || ft.is_symlink() {
            std::fs::copy(&from, &to)
                .with_context(|| format!("copy {} → {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

/// Xoá nội dung thư mục đích (giữ chính thư mục) trước khi restore — fail nếu không xoá được.
fn clear_dir(dst: &Path) -> Result<()> {
    if !dst.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dst)? {
        let entry = entry?;
        let p = entry.path();
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(&p).with_context(|| format!("remove {}", p.display()))?;
        } else {
            std::fs::remove_file(&p).with_context(|| format!("remove {}", p.display()))?;
        }
    }
    Ok(())
}

fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}

fn default_backup_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let base = Path::new(&home).join(".megagate");
    std::fs::create_dir_all(&base)?;
    Ok(base.join(format!("store_backup_{}", timestamp())))
}

fn prune_unreferenced(project_root: &Path, dry_run: bool) -> Result<PruneReport> {
    use mg_store::database::Database;

    let db = Database::open(&store_db_for(project_root)?)?;
    let unreferenced = db.list_unreferenced()?;
    let mut report = PruneReport {
        unreferenced: unreferenced.iter().map(|id| id.to_string()).collect(),
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
        .ok_or_else(crate::error::project_root_missing)?;

    match cmd {
        StoreCmd::Prune { dry_run, json } => {
            let report = prune_unreferenced(&project_root, dry_run)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Store prune {}: {} unreferenced package(s)",
                    if dry_run { "(dry-run)" } else { "(pruned)" },
                    report.unreferenced.len()
                );
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
        StoreCmd::Backup { path } => {
            let backup_dir = match path {
                Some(p) => PathBuf::from(p),
                None => default_backup_dir()?,
            };
            // Backup 2 phần: store.db (index) + vstore (packages thật)
            let db_src = store_db_for(&project_root)?;
            let db_dst = backup_dir.join("store.db");
            let vstore_dst = backup_dir.join("vstore");
            if db_src.exists() {
                if let Some(parent) = db_dst.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&db_src, &db_dst)
                    .with_context(|| format!("copy {} → {}", db_src.display(), db_dst.display()))?;
            }
            copy_dir(&vstore_root_for(&project_root), &vstore_dst)?;
            println!("backup store → {}", backup_dir.display());
            println!(
                "  store.db: {} bytes",
                std::fs::metadata(&db_dst).map(|m| m.len()).unwrap_or(0)
            );
            println!("  vstore: {} bytes", dir_size(&vstore_dst));
            println!(
                "  restore: mg store restore --path {}",
                backup_dir.display()
            );
            Ok(())
        }
        StoreCmd::Restore { path } => {
            let backup_dir = PathBuf::from(path);
            let db_src = backup_dir.join("store.db");
            let vstore_src = backup_dir.join("vstore");
            if !db_src.exists() && !vstore_src.exists() {
                bail!(
                    "invalid backup: {} (missing store.db and vstore)",
                    backup_dir.display()
                );
            }
            // Fail-closed: điểm yếu ghi đè — tự backup trước khi khôi phục
            let safety_backup = default_backup_dir()?;
            let db_dst = store_db_for(&project_root)?;
            copy_dir(
                &vstore_root_for(&project_root),
                &safety_backup.join("vstore"),
            )?;
            if db_dst.exists() {
                std::fs::copy(&db_dst, safety_backup.join("store.db"))?;
            }
            if db_src.exists() {
                if let Some(parent) = db_dst.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&db_src, &db_dst)?;
            }
            clear_dir(&vstore_root_for(&project_root))?;
            copy_dir(&vstore_src, &vstore_root_for(&project_root))?;
            println!("restore store ← {}", backup_dir.display());
            println!(
                "  safe backup (before restore): {}",
                safety_backup.display()
            );
            Ok(())
        }
    }
}
