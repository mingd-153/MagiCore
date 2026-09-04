use anyhow::Result;
use mgc_ui::info;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_RUN_SCRIPT_TIMEOUT_SECS: u64 = 300;
const RUN_SCRIPT_TIMEOUT_ENV: &str = "MGC_RUN_SCRIPT_TIMEOUT_SECS";

/// mgc run <script> [args...] — MagiCore Native Task Runner.
/// Priority:
///   1. mgc.toml [scripts] section
///   2. package.json scripts (Web core only)
pub async fn run(script: String, args: Vec<String>, core: Option<&str>) -> Result<()> {
    let ctx = crate::context::ProjectContext::load_with_core(core)?;
    let project_root = ctx.root();

    // 1. Try mgc.toml first (native MagiCore task definition)
    let mgc_toml_path = project_root.join("mgc.toml");
    if mgc_toml_path.exists() {
        if let Some(cmd) = resolve_mgc_toml_script(&mgc_toml_path, &script)? {
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

    Err(crate::error::script_not_found(&script))
}

fn reject_external_package_manager_script(cmd: &str, manifest_path: &Path) -> Result<()> {
    if let Some(pm) = mgc_exec::allowlist::find_forbidden_tool_in_script(cmd) {
        return Err(crate::error::forbidden_pm_script(cmd, manifest_path, pm));
    }

    Ok(())
}

fn resolve_mgc_toml_script(path: &Path, script: &str) -> Result<Option<String>> {
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
    let invocation = mgc_exec::allowlist::parse_script_invocation(cmd)
        .map_err(|e| crate::error::unsupported_script(script_name, &e))?;
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
        ("MGC_LIFECYCLE_EVENT".to_string(), script_name.to_string()),
    ];
    env.extend(invocation.env);

    // Load optimizer env for run command
    // Tải env optimizer cho lệnh run
    let runtime = detect_run_runtime(cwd);
    let optimizer_envs = crate::commands::optimizer::env_loader::load_optimizer_env(cwd, &runtime)
        .map_err(|e| {
            mgc_ui::warning(&format!("Failed to load optimizer config: {}", e));
            e
        })
        .unwrap_or_default();
    env.extend(optimizer_envs);

    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(cwd.to_path_buf()),
        timeout: Some(run_script_timeout()),
        env,
        clean_env: true,
        ..Default::default()
    };
    mgc_exec::prelude::run_inherited(&program, &script_args, &opts)?;
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

/// Detect runtime for optimizer env loading based on project files
/// Phát hiện runtime để load env optimizer dựa trên file project
fn detect_run_runtime(cwd: &Path) -> crate::commands::optimizer::runtime_detect::DetectedRuntime {
    use crate::commands::optimizer::runtime_detect::{DetectedRuntime, PackageManager};

    if cwd.join("Cargo.toml").exists() {
        DetectedRuntime::RustLib
    } else if cwd.join("go.mod").exists() {
        DetectedRuntime::GoLib
    } else if cwd.join("pyproject.toml").exists() || cwd.join("setup.py").exists() {
        DetectedRuntime::PythonLib
    } else if cwd.join("pubspec.yaml").exists() {
        DetectedRuntime::Flutter
    } else if cwd.join("package.json").exists() {
        // Detect package manager for web runtime
        if cwd.join("bun.lockb").exists() {
            DetectedRuntime::Bun
        } else if cwd.join("deno.json").exists() || cwd.join("deno.jsonc").exists() {
            DetectedRuntime::Deno
        } else {
            DetectedRuntime::NodeJs {
                package_manager: PackageManager::Npm,
            }
        }
    } else {
        DetectedRuntime::Unknown
    }
}

#[cfg(test)]
#[path = "test/run.rs"]
mod tests;
