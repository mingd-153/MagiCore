use anyhow::Result;
use mg_ui::info;

use crate::context::ProjectContext;

/// mg exec <cmd> [args...] — runs a shell command inside the project environment.
/// Prepends node_modules/.bin to PATH (and equivalent for other core ecosystems).
pub fn run(core: Option<&str>, command: String, args: Vec<String>) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let project_root = ctx.root();

    // Build environment PATH depending on core
    let mut path_env = std::env::var("PATH").unwrap_or_default();
    if ctx.adapter().name() == "web" {
        let bin = project_root.join("node_modules").join(".bin");
        if bin.exists() {
            path_env = format!("{}:{}", bin.display(), path_env);
        }
    }

    let full_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    info(&format!("$ {} {}", command, full_args.join(" ")));

    let status = std::process::Command::new(&command)
        .args(&args)
        .current_dir(project_root)
        .env("PATH", &path_env)
        .status()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
