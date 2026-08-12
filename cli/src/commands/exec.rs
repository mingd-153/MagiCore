use anyhow::Result;
use mg_ui::info;
use std::path::PathBuf;

use crate::context::ProjectContext;

/// mg exec <cmd> [args...] — runs an allowlisted command inside the project environment.
/// Prepends core-local bins through mg-exec clean env, without shell/PM wrappers.
pub fn run(core: Option<&str>, command: String, args: Vec<String>) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let project_root = ctx.root();

    let mut path_entries: Vec<PathBuf> = Vec::new();
    if ctx.adapter().name() == "web" {
        let bin = project_root.join("node_modules").join(".bin");
        if bin.exists() {
            path_entries.push(bin);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        path_entries.extend(std::env::split_paths(&path));
    }
    let path_env = std::env::join_paths(path_entries)?;

    let full_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    info(&format!("$ {} {}", command, full_args.join(" ")));

    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        log_path: Some(project_root.join(".megagate").join("exec.log")),
        clean_env: true,
        env: vec![("PATH".to_string(), path_env.to_string_lossy().to_string())],
        ..Default::default()
    };
    mg_exec::prelude::run(&command, &args, &opts)?;
    Ok(())
}
