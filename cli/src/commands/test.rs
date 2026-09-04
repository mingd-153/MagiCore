//! `test.rs` — MagiCore Test Command (Auto-detect Test Runner)
//! `test.rs` — Lệnh Test của MagiCore (Tự động phát hiện test runner)

use anyhow::Result;
use mgc_ui::info;
use std::path::Path;

/// mgc test [args...] — Run tests in project (auto-detect test runner)
/// Auto-detect test runner based on project type:
/// - package.json → npm test / pnpm test / yarn test
/// - Cargo.toml → cargo test
/// - pyproject.toml / setup.py → pytest
/// - go.mod → go test
/// - pubspec.yaml → flutter test
/// - mgc.toml [scripts] test → custom test command
pub async fn test(args: Vec<String>, core: Option<&str>) -> Result<()> {
    let ctx = crate::context::ProjectContext::load_with_core(core)?;
    let project_root = ctx.root();

    // 1. Priority: mgc.toml [scripts] test — ưu tiên: mgc.toml [scripts] test
    let mgc_toml_path = project_root.join("mgc.toml");
    if mgc_toml_path.exists() {
        if let Some(cmd) = resolve_mgc_toml_script(&mgc_toml_path, "test")? {
            info(&format!("Running test from mgc.toml: {}", cmd));
            return crate::commands::run::run("test".to_string(), args, core).await;
        }
    }

    // 2. Auto-detect test runner based on project files — tự động phát hiện test runner
    if let Some((runner, runner_args)) = detect_test_runner(project_root)? {
        info(&format!(
            "Auto-detected test runner: {} {}",
            runner,
            runner_args.join(" ")
        ));

        let mut full_args = runner_args;
        full_args.extend(args);

        // Load optimizer env for test runtime
        // Tải env optimizer cho runtime test
        let runtime = detect_test_runtime(project_root);
        let optimizer_envs =
            crate::commands::optimizer::env_loader::load_optimizer_env(project_root, &runtime)
                .map_err(|e| {
                    mgc_ui::warning(&format!("Failed to load optimizer config: {}", e));
                    e
                })
                .unwrap_or_default();
        let env: Vec<(String, String)> = optimizer_envs.into_iter().collect();

        let opts = mgc_exec::prelude::ExecOptions {
            cwd: Some(project_root.to_path_buf()),
            timeout: Some(std::time::Duration::from_secs(600)), // 10min test timeout
            execution_scope: Some(mgc_exec::allowlist::ExecutionScope::TestRunner), // TestRunner scope allows PM tools
            env,
            clean_env: false, // Preserve existing env
            log_path: Some(project_root.join(".magicore").join("exec.log")), // P0.7 FIX: Enable audit logging
            ..Default::default()
        };

        return mgc_exec::prelude::run_inherited(&runner, &full_args, &opts)
            .map(|_report| ()) // Discard ExecReport, return () — bỏ ExecReport, trả về ()
            .map_err(|e| anyhow::anyhow!("Test runner failed: {}", e));
    }

    // 3. No test runner detected — không phát hiện test runner
    Err(anyhow::anyhow!(
        "No test runner detected. Add 'test' script to mgc.toml or package.json"
    ))
}

/// Detect test runner based on project manifest files — phát hiện test runner dựa trên file manifest
fn detect_test_runner(project_root: &Path) -> Result<Option<(String, Vec<String>)>> {
    // Check Cargo.toml (Rust) — kiểm tra Cargo.toml
    if project_root.join("Cargo.toml").exists() {
        return Ok(Some(("cargo".to_string(), vec!["test".to_string()])));
    }

    // Check go.mod (Go) — kiểm tra go.mod
    if project_root.join("go.mod").exists() {
        return Ok(Some((
            "go".to_string(),
            vec!["test".to_string(), "./...".to_string()],
        )));
    }

    // Check pyproject.toml or setup.py (Python) — kiểm tra pyproject.toml hoặc setup.py
    if project_root.join("pyproject.toml").exists() || project_root.join("setup.py").exists() {
        // Try pytest first, fall back to python -m unittest — thử pytest trước
        // Note: pytest auto-discovers test_*.py and *_test.py in current directory
        // -s: no output capture, -v: verbose
        return Ok(Some((
            "pytest".to_string(),
            vec!["-s".to_string(), "-v".to_string()],
        )));
    }

    // Check pubspec.yaml (Flutter/Dart) — kiểm tra pubspec.yaml
    if project_root.join("pubspec.yaml").exists() {
        return Ok(Some(("flutter".to_string(), vec!["test".to_string()])));
    }

    // Check deno.json/deno.jsonc (Deno) — kiểm tra deno.json
    if project_root.join("deno.json").exists() || project_root.join("deno.jsonc").exists() {
        return Ok(Some(("deno".to_string(), vec!["test".to_string()])));
    }

    // Check package.json (Node.js/Web) — kiểm tra package.json
    let package_json_path = project_root.join("package.json");
    if package_json_path.exists() {
        // Check if "test" script exists in package.json — kiểm tra script "test"
        if let Some(_test_script) = resolve_package_json_script(&package_json_path, "test")? {
            // Detect package manager — phát hiện package manager
            let pm = detect_package_manager(project_root);
            return Ok(Some((pm, vec!["test".to_string()])));
        }
    }

    // No test runner detected — không phát hiện test runner
    Ok(None)
}

/// Detect runtime for optimizer env loading based on test runner
/// Phát hiện runtime để load env optimizer dựa trên test runner
fn detect_test_runtime(
    project_root: &Path,
) -> crate::commands::optimizer::runtime_detect::DetectedRuntime {
    use crate::commands::optimizer::runtime_detect::{detect_runtimes, DetectedRuntime};

    // Detect core type first
    let core = if project_root.join(".mgc.core").exists() {
        std::fs::read_to_string(project_root.join(".mgc.core"))
            .unwrap_or_default()
            .trim()
            .to_string()
    } else {
        // Fallback: infer from files
        if project_root.join("Cargo.toml").exists() {
            "lib".to_string()
        } else if project_root.join("pyproject.toml").exists() {
            // Check if AI project (has torch/pytorch)
            if let Ok(content) = std::fs::read_to_string(project_root.join("pyproject.toml")) {
                if content.contains("torch")
                    || content.contains("pytorch")
                    || content.contains("[tool.magicore]")
                {
                    "ai".to_string()
                } else {
                    "lib".to_string()
                }
            } else {
                "lib".to_string()
            }
        } else if project_root.join("pubspec.yaml").exists() {
            "app".to_string()
        } else if project_root.join("package.json").exists() {
            "web".to_string()
        } else {
            "lib".to_string()
        }
    };

    // Use detect_runtimes from optimizer (core-aware)
    let runtimes = detect_runtimes(project_root, &core);
    runtimes
        .first()
        .cloned()
        .unwrap_or(DetectedRuntime::Unknown)
}

/// Detect package manager for Node.js projects — phát hiện package manager cho project Node.js
fn detect_package_manager(project_root: &Path) -> String {
    // Check for Deno first (deno.json/deno.jsonc)
    // Kiểm tra Deno trước (deno.json/deno.jsonc)
    if project_root.join("deno.json").exists() || project_root.join("deno.jsonc").exists() {
        return "deno".to_string();
    }

    if project_root.join("pnpm-lock.yaml").exists() {
        "pnpm".to_string()
    } else if project_root.join("yarn.lock").exists() {
        "yarn".to_string()
    } else if project_root.join("bun.lockb").exists() {
        "bun".to_string()
    } else if project_root.join("package-lock.json").exists() {
        "npm".to_string()
    } else {
        // Default to npm if no lockfile — mặc định npm nếu không có lockfile
        "npm".to_string()
    }
}

/// Resolve test script from mgc.toml — lấy test script từ mgc.toml
fn resolve_mgc_toml_script(path: &Path, script: &str) -> Result<Option<String>> {
    let content = std::fs::read_to_string(path)?;
    let toml: toml::Value = toml::from_str(&content)?;
    Ok(toml
        .get("scripts")
        .and_then(|s| s.get(script))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

/// Resolve test script from package.json — lấy test script từ package.json
fn resolve_package_json_script(path: &Path, script: &str) -> Result<Option<String>> {
    let content = std::fs::read_to_string(path)?;
    let manifest: serde_json::Value = serde_json::from_str(&content)?;
    Ok(manifest
        .get("scripts")
        .and_then(|s| s.get(script))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}
