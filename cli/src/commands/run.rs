use anyhow::Result;
use std::process::Command;

/// mg run <script> [args...] — runs a script defined in package.json
pub async fn run(script: String, args: Vec<String>, core: Option<&str>) -> Result<()> {
    let ctx = crate::context::ProjectContext::load_with_core(core)?;
    let project_root = ctx.root();

    let package_json_path = project_root.join("package.json");
    if !package_json_path.exists() {
        anyhow::bail!("No package.json found in {}", project_root.display());
    }

    let content = std::fs::read_to_string(&package_json_path)?;
    let manifest: serde_json::Value = serde_json::from_str(&content)?;

    let script_cmd = manifest
        .get("scripts")
        .and_then(|s| s.get(&script))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Script '{}' not found in package.json", script))?
        .to_string();

    // Build PATH with node_modules/.bin prepended
    let node_modules_bin = project_root.join("node_modules").join(".bin");
    let mut path_env = std::env::var("PATH").unwrap_or_default();
    if node_modules_bin.exists() {
        path_env = format!("{}:{}", node_modules_bin.display(), path_env);
    }

    // Run the script through shell
    let full_cmd = if args.is_empty() {
        script_cmd
    } else {
        format!("{} {}", script_cmd, args.join(" "))
    };

    mg_ui::info(&format!("$ {}", full_cmd));

    let status = Command::new("sh")
        .arg("-c")
        .arg(&full_cmd)
        .current_dir(project_root)
        .env("PATH", &path_env)
        .env("INIT_CWD", project_root.display().to_string())
        .env("npm_lifecycle_event", &script)
        .status()?;

    if !status.success() {
        let code = status.code().unwrap_or(1);
        std::process::exit(code);
    }

    Ok(())
}
