//! mg hooks — user-defined pre/post scripts (P2, 21 §9)
//! (Chạy script trước/sau command: pre-install, post-publish...
//!  hook fail → command fail; hook ghi audit.)

use anyhow::{bail, Result};
use clap::Subcommand;
use mg_config::hooks;
use std::path::Path;

#[derive(Subcommand, Debug, Clone)]
pub enum HooksCmd {
    /// Run hooks for an event (e.g. pre-install)
    Run { event: String },
    /// List configured hook events
    Ls,
    /// Show hooks file paths (project + user)
    Paths,
}

/// Project root — từ cwd (project-local hooks) hoặc -p
pub fn project_root() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf())
}

pub fn handle(cmd: HooksCmd) -> Result<()> {
    let root = project_root();
    match cmd {
        HooksCmd::Run { event } => {
            hooks::run_hooks(&root, &event)?;
            println!("hooks: {} ok", event);
            Ok(())
        }
        HooksCmd::Ls => {
            let all = hooks::list_hooks(&root)?;
            if all.is_empty() {
                println!("no hooks configured");
            } else {
                for (event, cmds) in all {
                    println!("{event}: {} cmd(s)", cmds.len());
                    for c in cmds {
                        println!("  - {c}");
                    }
                }
            }
            Ok(())
        }
        HooksCmd::Paths => {
            for p in hooks::hooks_paths(&root) {
                println!("{}", p.display());
            }
            Ok(())
        }
    }
}

/// Hook wrapper cho command có event (install/publish/remove...):
/// chạy pre-* trước, post-* sau — fail → báo lỗi chặn luôn
pub fn run_event(root: &Path, event: &str) -> Result<()> {
    hooks::run_hooks(root, event).map_err(|e| {
        // fail-closed: hook fail → command fail (21 §9)
        crate::error::hook_failed(event, &e)
    })
}

#[allow(dead_code)]
fn _unsupported(_c: HooksCmd) -> Result<()> {
    bail!("not supported")
}
