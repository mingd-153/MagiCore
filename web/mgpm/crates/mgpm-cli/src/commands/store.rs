use std::path::PathBuf;

use colored::Colorize;

use mgpm_core::config::MgpmConfig;
use mgpm_store::{GlobalVirtualStore, SqliteStore, StoreVerifier};
use mgpm_store::store::CasContentStore;

use super::super::{cpath, format_size};

#[derive(clap::Subcommand)]
pub enum StoreCommand {
    /// Verify store integrity (re-hash all CAS files)
    Verify {
        /// Auto-fix corrupted files by removing them
        #[arg(long)]
        fix: bool,
    },
    /// Show store status (packages, projects, size)
    Status,
    /// Prune unreferenced packages
    Prune {
        /// Show what would be deleted without actually deleting
        #[arg(long)]
        dry_run: bool,
    },
    /// Manage completion cache
    CompletionsCache {
        #[command(subcommand)]
        command: CompletionsCacheAction,
    },
    /// Global Virtual Store operations
    Gvs {
        #[command(subcommand)]
        command: GvsCommand,
    },
}

#[derive(clap::Subcommand)]
pub enum GvsCommand {
    /// Register current project in the global virtual store
    Register {
        /// Dependency graph hash to register with
        #[arg(long)]
        dep_graph_hash: String,
    },
    /// Unregister current project from the global virtual store
    Unregister,
    /// List all registered projects
    List,
    /// Show GVS status
    Status,
    /// Garbage collect orphaned GVS directories
    Gc,
}

#[derive(clap::Subcommand)]
pub enum CompletionsCacheAction {
    /// Warm the completion cache
    Warm,
    /// Clear the completion cache
    Clear,
}

pub fn cmd_store_verify(config: &MgpmConfig, fix: bool) -> Result<(), String> {
    let store_path = config.store.store_path();
    if !store_path.exists() {
        return Err(format!("store not found at {}", cpath(&store_path)));
    }

    let index_path = store_path.join("v2").join("index.db");
    let cas_path = store_path.join("v2").join("CAS");

    if !index_path.exists() {
        return Err(format!("store not initialized at {}", cpath(&store_path)));
    }

    let index = SqliteStore::open(&index_path, false)
        .map_err(|e| format!("failed to open store: {}", e))?;
    let store = CasContentStore::new(cas_path, Box::new(index))
        .map_err(|e| format!("failed to open content store: {}", e))?;

    let verifier = StoreVerifier::new(&store, store.index());
    let report = verifier.verify(fix).map_err(|e| format!("verify failed: {}", e))?;

    println!("{} Store verification complete", "[DONE]".green().bold());
    println!("  Packages: {}", report.total_packages);
    println!("  Verified: {}", report.verified);
    println!("  Corrupted: {}", report.corrupted_files.len());
    println!("  Missing: {}", report.missing_files.len());
    println!("  Duration: {}ms", report.duration_ms);

    if !report.is_healthy() {
        for f in &report.corrupted_files {
            println!("  {} Corrupted: {}", "[ERR]".red(), f);
        }
        for f in &report.missing_files {
            println!("  {} Missing: {}", "[WARN]".yellow(), f);
        }
        if fix {
            println!(
                "  Attempted auto-fix for {} file(s)",
                report.corrupted_files.len()
            );
        }
        return Err("store has integrity issues".to_string());
    }

    Ok(())
}

pub fn cmd_store_status(config: &MgpmConfig) -> Result<(), String> {
    let store_path = config.store.store_path();
    println!("{} Store status", "[INFO]".cyan().bold());
    println!("  Path: {}", cpath(&store_path));

    if !store_path.exists() {
        println!("  {} Store directory does not exist", "[WARN]".yellow().bold());
        println!("  Packages: 0");
        println!("  Used: 0 B");
        return Ok(());
    }

    let index_path = store_path.join("v2").join("index.db");
    let cas_path = store_path.join("v2").join("CAS");

    if !index_path.exists() {
        println!("  Store not initialized");
        println!("  Packages: 0");
        println!("  Used: 0 B");
        return Ok(());
    }

    let index = SqliteStore::open(&index_path, true)
        .map_err(|e| format!("failed to open store: {}", e))?;
    let store = CasContentStore::new(cas_path, Box::new(index))
        .map_err(|e| format!("failed to open content store: {}", e))?;

    let verifier = StoreVerifier::new(&store, store.index());
    let report = verifier.status().map_err(|e| format!("failed to get store status: {}", e))?;

    println!("  Packages: {}", report.total_packages);
    println!("  Projects: {}", report.total_projects);
    println!("  Used: {}", format_size(report.total_size_bytes));
    println!("  Unreferenced: {}", report.unreferenced_packages.len());
    println!("  Reclaimable: {}", format_size(report.reclaimable_bytes));

    Ok(())
}

pub fn cmd_store_prune(config: &MgpmConfig, dry_run: bool) -> Result<(), String> {
    let store_path = config.store.store_path();
    if !store_path.exists() {
        return Err(format!("store not found at {}", cpath(&store_path)));
    }

    let index_path = store_path.join("v2").join("index.db");
    let cas_path = store_path.join("v2").join("CAS");

    if !index_path.exists() {
        return Err(format!("store not initialized at {}", cpath(&store_path)));
    }

    let index = SqliteStore::open(&index_path, false)
        .map_err(|e| format!("failed to open store: {}", e))?;
    let store = CasContentStore::new(cas_path, Box::new(index))
        .map_err(|e| format!("failed to open content store: {}", e))?;

    let verifier = StoreVerifier::new(&store, store.index());

    if dry_run {
        let report = verifier
            .status()
            .map_err(|e| format!("failed to get store status: {}", e))?;
        println!("{} Dry run — nothing deleted", "[INFO]".cyan().bold());
        println!("  Would remove: {} packages", report.unreferenced_packages.len());
        println!("  Would reclaim: {}", format_size(report.reclaimable_bytes));
    } else {
        let report = verifier
            .prune(false)
            .map_err(|e| format!("prune failed: {}", e))?;
        println!("{} Prune complete", "[OK]".green().bold());
        println!("  Removed: {} packages", report.unreferenced_packages.len());
        println!("  Reclaimed: {}", format_size(report.reclaimable_bytes));
    }

    Ok(())
}

pub fn cmd_store_gvs(config: &MgpmConfig, cmd: GvsCommand) -> Result<(), String> {
    let gvs_path = config.store.gvs_path();
    let gvs = GlobalVirtualStore::new(gvs_path);

    let store_path = config.store.store_path();
    let index_path = store_path.join("v2").join("index.db");

    match cmd {
        GvsCommand::Register { dep_graph_hash } => {
            let index = SqliteStore::open(&index_path, false)
                .map_err(|e| format!("failed to open store: {}", e))?;
            let project_path = std::env::current_dir()
                .map_err(|e| format!("failed to get current dir: {}", e))?;

            gvs.ensure_dirs()
                .map_err(|e| format!("failed to create GVS dirs: {}", e))?;
            gvs.register(&project_path, &dep_graph_hash, &index)
                .map_err(|e| format!("failed to register project: {}", e))?;

            println!("{} Project registered in GVS", "[OK]".green().bold());
            println!("  Path: {}", cpath(&project_path));
            println!("  Dep graph hash: {}", dep_graph_hash);
            println!("  GVS root: {}", cpath(gvs.root()));
        }
        GvsCommand::Unregister => {
            let index = SqliteStore::open(&index_path, false)
                .map_err(|e| format!("failed to open store: {}", e))?;
            let project_path = std::env::current_dir()
                .map_err(|e| format!("failed to get current dir: {}", e))?;

            gvs.unregister(&project_path, &index)
                .map_err(|e| format!("failed to unregister project: {}", e))?;

            println!("{} Project unregistered from GVS", "[OK]".green().bold());
            println!("  Path: {}", cpath(&project_path));
        }
        GvsCommand::List => {
            if !index_path.exists() {
                println!("{} No projects registered in GVS", "[INFO]".cyan().bold());
                return Ok(());
            }

            let index = SqliteStore::open(&index_path, true)
                .map_err(|e| format!("failed to open store: {}", e))?;

            let projects = gvs
                .list_projects(&index)
                .map_err(|e| format!("failed to list projects: {}", e))?;

            if projects.is_empty() {
                println!("{} No projects registered in GVS", "[INFO]".cyan().bold());
                return Ok(());
            }

            println!("{} Registered projects:", "[LIST]".cyan().bold());
            for p in &projects {
                let hash = p.dep_graph_hash().unwrap_or_else(|| "N/A".to_string());
                println!(
                    "  {} (hash: {})",
                    cpath(&PathBuf::from(&p.path)),
                    hash
                );
            }
            println!("  Total: {} project(s)", projects.len());
        }
        GvsCommand::Status => {
            if !index_path.exists() {
                println!("{} GVS not initialized", "[INFO]".cyan().bold());
                return Ok(());
            }

            let index = SqliteStore::open(&index_path, true)
                .map_err(|e| format!("failed to open store: {}", e))?;

            let stats = gvs
                .status(&index)
                .map_err(|e| format!("failed to get GVS status: {}", e))?;

            println!("{} GVS status", "[INFO]".cyan().bold());
            println!("  Root: {}", cpath(&stats.gvs_root));
            println!("  Projects: {}", stats.total_projects);
            println!("  Packages: {}", stats.total_packages);
            println!("  Symlinks: {}", stats.total_symlinks);
            println!("  Total size: {}", format_size(stats.total_size_bytes));
            if stats.reclaimable_dirs > 0 {
                println!(
                    "  {} Reclaimable: {} dir(s), {} symlink(s)",
                    "[WARN]".yellow().bold(),
                    stats.reclaimable_dirs,
                    stats.reclaimable_symlinks,
                );
            }
        }
        GvsCommand::Gc => {
            if !index_path.exists() {
                println!("{} No orphaned GVS directories found", "[OK]".green().bold());
                return Ok(());
            }

            let index = SqliteStore::open(&index_path, false)
                .map_err(|e| format!("failed to open store: {}", e))?;

            let report = gvs
                .gc(&index)
                .map_err(|e| format!("GVS GC failed: {}", e))?;

            if report.removed_dirs.is_empty() {
                println!("{} No orphaned GVS directories found", "[OK]".green().bold());
            } else {
                println!("{} GVS GC complete", "[OK]".green().bold());
                for dir in &report.removed_dirs {
                    println!("  Removed: {}", cpath(dir));
                }
                println!("  Symlinks removed: {}", report.removed_symlinks);
                println!("  Reclaimed: {}", format_size(report.reclaimed_bytes));
            }
        }
    }

    Ok(())
}

pub fn cmd_completions_cache(action: CompletionsCacheAction) -> Result<(), String> {
    let cache_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mgpm")
        .join("completions");

    match action {
        CompletionsCacheAction::Warm => {
            std::fs::create_dir_all(&cache_dir)
                .map_err(|e| format!("failed to create completions cache dir: {e}"))?;

            let project_root = std::env::current_dir().map_err(|e| e.to_string())?;
            match mgpm_workspace::Workspace::discover(&project_root) {
                Ok(workspace) => {
                    let members: Vec<String> =
                        workspace.members().iter().map(|m| m.name.clone()).collect();

                    let cache_file = cache_dir.join("workspace_members.json");
                    let json = serde_json::to_string_pretty(&members)
                        .map_err(|e| format!("failed to serialize cache: {e}"))?;
                    std::fs::write(&cache_file, json)
                        .map_err(|e| format!("failed to write cache: {e}"))?;

                    println!(
                        "{} Cached {} workspace member names",
                        "[OK]".green().bold(),
                        members.len()
                    );
                }
                Err(_) => {
                    println!(
                        "  {} No workspace found, cache will be empty",
                        "[WARN]".yellow().bold()
                    );
                    let cache_file = cache_dir.join("workspace_members.json");
                    std::fs::write(&cache_file, "[]")
                        .map_err(|e| format!("failed to write cache: {e}"))?;
                }
            }

            Ok(())
        }
        CompletionsCacheAction::Clear => {
            if cache_dir.exists() {
                std::fs::remove_dir_all(&cache_dir)
                    .map_err(|e| format!("failed to clear completions cache: {e}"))?;
                println!("{} Cleared completion cache", "[OK]".green().bold());
            } else {
                println!("  {} No cache to clear", "[WARN]".yellow().bold());
            }
            Ok(())
        }
    }
}
