use anyhow::Result;
use mg_ui::info;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_RUN_SCRIPT_TIMEOUT_SECS: u64 = 300;
const RUN_SCRIPT_TIMEOUT_ENV: &str = "MG_RUN_SCRIPT_TIMEOUT_SECS";

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

fn reject_external_package_manager_script(cmd: &str, manifest_path: &Path) -> Result<()> {
    if let Some(pm) = mg_exec::allowlist::find_forbidden_tool_in_script(cmd) {
        anyhow::bail!(
            "Script '{}' in '{}' delegates to forbidden package manager '{}'. Core-web task execution must not bounce through another package manager.",
            cmd,
            manifest_path.display(),
            pm
        );
    }

    Ok(())
}

fn resolve_mg_toml_script(path: &Path, script: &str) -> Result<Option<String>> {
    let content = std::fs::read_to_string(path)?;
    let toml: toml::Value = toml::from_str(&content)?;
    Ok(toml
        .get("scripts")
        .and_then(|s| s.get(script))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

fn resolve_package_json_script(path: &Path, script: &str) -> Result<Option<String>> {
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
    cwd: &Path,
    script_name: &str,
    bin_path: Option<PathBuf>,
) -> Result<()> {
    let invocation = mg_exec::allowlist::parse_script_invocation(cmd)
        .map_err(|e| anyhow::anyhow!("Unsupported script '{}': {}", script_name, e))?;
    let program = invocation.program;
    let mut script_args = invocation.args;
    script_args.extend(args.iter().cloned());
    let full_cmd = std::iter::once(program.as_str())
        .chain(script_args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");

    info(&format!("$ {}", full_cmd));

    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    if let Some(bin) = bin_path {
        if bin.exists() {
            paths.insert(0, bin);
        }
    }
    let path_env = std::env::join_paths(paths)?;

    let mut env = vec![
        ("PATH".to_string(), path_env.to_string_lossy().to_string()),
        ("MG_LIFECYCLE_EVENT".to_string(), script_name.to_string()),
    ];
    env.extend(invocation.env);

    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(cwd.to_path_buf()),
        timeout: Some(run_script_timeout()),
        env,
        clean_env: true,
        ..Default::default()
    };
    mg_exec::prelude::run_inherited(&program, &script_args, &opts)?;
    Ok(())
}

fn run_script_timeout() -> Duration {
    std::env::var(RUN_SCRIPT_TIMEOUT_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(DEFAULT_RUN_SCRIPT_TIMEOUT_SECS))
}
