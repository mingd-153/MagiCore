use anyhow::Result;
use mg_ui::info;

/// mg run <script> [args...] — MegaGate Native Task Runner.
/// Priority:
///   1. mg.toml [scripts] section
///   2. package.json scripts (Web core only)
pub async fn run(script: String, args: Vec<String>, core: Option<&str>) -> Result<()> {
    let ctx = crate::context::ProjectContext::load_with_core(core)?;
    let project_root = ctx.root();

    // 1. Try mg.toml first (native MegaGate task definition)
    let mg_toml_path = project_root.join("mg.toml");
    if mg_toml_path.exists() {
        if let Some(cmd) = resolve_mg_toml_script(&mg_toml_path, &script)? {
            return execute_task_with_bin(&cmd, &args, project_root, &script, None);
        }
    }

    // 2. Fall back to package.json (web ecosystem compatibility)
    let package_json_path = project_root.join("package.json");
    if package_json_path.exists() {
        if let Some(cmd) = resolve_package_json_script(&package_json_path, &script)? {
            reject_external_package_manager_script(&cmd, &package_json_path)?;
            let bin = project_root.join("node_modules").join(".bin");
            return execute_task_with_bin(&cmd, &args, project_root, &script, Some(bin));
        }
    }

    anyhow::bail!(
        "Script '{}' not found. Define it in 'mg.toml' under [scripts] or in 'package.json'.",
        script
    )
}

fn reject_external_package_manager_script(
    cmd: &str,
    manifest_path: &std::path::Path,
) -> Result<()> {
    let first = cmd.split_whitespace().next().unwrap_or_default();
    let delegated_pm = match first {
        "npm" => Some("npm"),
        "pnpm" => Some("pnpm"),
        "bun" => Some("bun"),
        "yarn" => Some("yarn"),
        "npx" => Some("npx"),
        "bunx" => Some("bunx"),
        _ => None,
    };

    if let Some(pm) = delegated_pm {
        anyhow::bail!(
            "Script '{}' in '{}' delegates to '{}'. Core-web task execution must not bounce through another package manager.",
            cmd,
            manifest_path.display(),
            pm
        );
    }

    Ok(())
}

fn resolve_mg_toml_script(path: &std::path::Path, script: &str) -> Result<Option<String>> {
    let content = std::fs::read_to_string(path)?;
    let toml: toml::Value = toml::from_str(&content)?;
    Ok(toml
        .get("scripts")
        .and_then(|s| s.get(script))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

fn resolve_package_json_script(path: &std::path::Path, script: &str) -> Result<Option<String>> {
    let content = std::fs::read_to_string(path)?;
    let manifest: serde_json::Value = serde_json::from_str(&content)?;
    Ok(manifest
        .get("scripts")
        .and_then(|s| s.get(script))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

fn execute_task_with_bin(
    cmd: &str,
    args: &[String],
    cwd: &std::path::Path,
    script_name: &str,
    bin_path: Option<std::path::PathBuf>,
) -> Result<()> {
    let full_cmd = if args.is_empty() {
        cmd.to_string()
    } else {
        format!("{} {}", cmd, args.join(" "))
    };

    info(&format!("$ {}", full_cmd));

    let mut path_env = std::env::var("PATH").unwrap_or_default();
    if let Some(bin) = bin_path {
        if bin.exists() {
            path_env = format!("{}:{}", bin.display(), path_env);
        }
    }

    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(&full_cmd)
        .current_dir(cwd)
        .env("PATH", &path_env)
        .env("MG_LIFECYCLE_EVENT", script_name)
        .status()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
