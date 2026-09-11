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
///
/// Native-engine ruling 2026-09-10: a script whose PROGRAM is a rival
/// JS runtime (bun/deno) or an external package manager (npm/npx/pnpm/
/// yarn/bunx) is REJECTED on the native path — no silent forwarding.
/// `--compat-runtime bun|deno` opts into the temporary compatibility
/// lane explicitly, with a loud warning on the spawn.
/// Script có PROGRAM là runtime đối thủ (bun/deno) hoặc PM ngoài bị TỪ
/// CHỐI trên đường native — không chuyển tiếp âm thầm. Cờ
/// --compat-runtime chọn lane compat tường minh kèm cảnh báo.
pub async fn run(
    script: String,
    args: Vec<String>,
    core: Option<&str>,
    compat_runtime: Option<&str>,
) -> Result<()> {
    let compat = crate::commands::compat::CompatMode::from_flag(compat_runtime)?;
    let ctx = crate::context::ProjectContext::load_with_core(core)?;
    let project_root = ctx.root();

    // 1. Try mgc.toml first (native MagiCore task definition)
    let mgc_toml_path = project_root.join("mgc.toml");
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if mgc_toml_path.exists()
        && let Some(cmd) = resolve_mgc_toml_script(&mgc_toml_path, &script)?
    {
        gate_script_program(&compat, &cmd)?;
        return execute_task_with_bin(&cmd, &args, project_root, &script, None, &compat);
    }

    // 2. Fall back to package.json (web ecosystem compatibility)
    let package_json_path = project_root.join("package.json");
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if package_json_path.exists()
        && let Some(cmd) = resolve_package_json_script(&package_json_path, &script)?
    {
        reject_external_package_manager_script(&cmd, &package_json_path)?;
        gate_script_program(&compat, &cmd)?;
        let bin = project_root.join("node_modules").join(".bin");
        return execute_task_with_bin(&cmd, &args, project_root, &script, Some(bin), &compat);
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

/// Gate the script's PROGRAM against the native-engine contract: rival
/// JS runtimes need an explicit compat opt-in; external PMs are never
/// spawnable. The program token is inspected WITHOUT spawning anything.
/// Chặn PROGRAM của script theo hợp đồng engine native: runtime đối thủ
/// cần compat tường minh; PM ngoài không bao giờ spawn. Token program
/// được soi KHÔNG spawn gì cả.
fn gate_script_program(compat: &crate::commands::compat::CompatMode, cmd: &str) -> Result<()> {
    let invocation = mgc_exec::allowlist::parse_script_invocation(cmd)
        .map_err(|e| crate::error::unsupported_script("run", &e))?;
    crate::commands::compat::gate_runtime_spawn(compat, &invocation.program)
}

fn execute_task_with_bin(
    cmd: &str,
    args: &[String],
    cwd: &Path,
    script_name: &str,
    bin_path: Option<PathBuf>,
    compat: &crate::commands::compat::CompatMode,
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
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Some(bin) = bin_path
        && bin.exists()
    {
        paths.insert(0, bin);
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

    // P0-1: truyền runtime compat đã chọn (nếu có) xuống mgc-exec để
    // exemption blocker shim + allowlist áp cho ĐÚNG runtime đã gate.
    let compat_runtime = match compat {
        crate::commands::compat::CompatMode::Native => None,
        crate::commands::compat::CompatMode::Explicit(runtime) => Some(runtime.clone()),
    };
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(cwd.to_path_buf()),
        timeout: Some(run_script_timeout()),
        log_path: Some(cwd.join(".magicore").join("exec.log")),
        env,
        clean_env: true,
        compat_runtime,
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
