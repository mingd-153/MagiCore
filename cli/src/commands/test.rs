//! `test.rs` — MagiCore Test Command (native orchestration).
//! `test.rs` — Lệnh Test của MagiCore (điều phối native).
//!
//! `mgc test` delegates execution to an explicitly selected compiler or
//! test runner after dependency installation; package managers are blocked
//! by mgc-exec in every scope. Ecosystems without a native install contract
//! must not be reported as native lifecycle support.
//! mgc test chạy trên engine NATIVE — chỉ binary toolchain. Bun/Deno
//! sau khi dependency được cài; mgc-exec chặn package manager ở mọi scope.

use anyhow::Result;
use mgc_ui::info;
use std::path::Path;

use crate::commands::compat::{CompatMode, gate_runtime_spawn};

/// mgc test [args...] — Run tests in project (auto-detect test runner)
/// Auto-detect test runner based on project type:
/// - package.json → npm test / pnpm test / yarn test
/// - Cargo.toml → cargo test
/// - pyproject.toml / setup.py → pytest
/// - go.mod → go test
/// - pubspec.yaml → flutter test
/// - mgc.toml [scripts] test → custom test command
pub async fn test(
    args: Vec<String>,
    core: Option<&str>,
    compat_runtime: Option<&str>,
) -> Result<()> {
    let ctx = crate::context::ProjectContext::load_with_core(core)?;
    test_at(ctx.root(), args, core, compat_runtime).await
}

/// Run the same test discovery and execution policy against an explicit project root.
/// Chạy cùng chính sách phát hiện và thực thi test trên project root tường minh.
pub(crate) async fn test_at(
    project_root: &Path,
    args: Vec<String>,
    core: Option<&str>,
    compat_runtime: Option<&str>,
) -> Result<()> {
    let compat = CompatMode::from_flag(compat_runtime)?;

    // 1. Priority: mgc.toml [scripts] test — ưu tiên: mgc.toml [scripts] test
    let mgc_toml_path = project_root.join("mgc.toml");
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if mgc_toml_path.exists()
        && let Some(cmd) = resolve_mgc_toml_script(&mgc_toml_path, "test")?
    {
        info(&format!("Running test from mgc.toml: {}", cmd));
        return crate::commands::run::run("test".to_string(), args, core, None).await;
    }

    // 2. Auto-detect test runner based on project files — tự động phát hiện test runner
    if let Some((runner, runner_args)) = detect_test_runner(project_root)? {
        // Project-defined test SCRIPT (package.json "test") routes
        // through the native task runner — run.rs re-parses and gates
        // rival runtimes/PMs exactly like `mgc run test`. Handled FIRST
        // because it consumes `args` by value.
        // Script test DO PROJECT ĐỊNH NGHĨA đi qua task runner native —
        // run.rs parse + chặn runtime đối thủ/PM y như `mgc run test`.
        // Xử lý TRƯỚC vì nó tiêu thụ `args` theo giá trị.
        if runner == "mgc-internal-run-script" {
            return crate::commands::run::run("test".to_string(), args, core, compat_runtime).await;
        }

        info(&format!(
            "Auto-detected test runner: {} {}",
            runner,
            runner_args.join(" ")
        ));

        let mut full_args = runner_args;
        full_args.extend(args);

        // Package managers are rejected by the execution boundary for every
        // scope. Runtime compatibility does not grant package-manager use.
        gate_runtime_spawn(&compat, &runner)?;

        // Project-defined test SCRIPT (package.json "test") routes
        // through the native task runner — run.rs re-parses and gates
        // rival runtimes/PMs exactly like `mgc run test`.
        // Script test DO PROJECT ĐỊNH NGHĨA đi qua task runner native —
        // run.rs parse + chặn runtime đối thủ/PM y như `mgc run test`.
        // Security gate: JS runtimes must pass launcher policy before exec
        // (block --eval / --allow-all style injection through test args).
        // Cổng bảo mật: runtime JS phải qua launcher policy trước khi exec
        // (chặn --eval / --allow-all tiêm qua test args).
        if let Some(policy) = test_runner_policy(&runner) {
            let arg_refs: Vec<&str> = full_args.iter().map(|s| s.as_str()).collect();
            policy.validate_args(&arg_refs)?;
        }

        // Load optimizer env for test runtime
        // Tải env optimizer cho runtime test
        let runtime = detect_test_runtime(project_root)?;
        let optimizer_envs =
            crate::commands::optimizer::env_loader::load_optimizer_env(project_root, &runtime)
                .map_err(|e| {
                    mgc_ui::warning(&format!("Failed to load optimizer config: {}", e));
                    e
                })
                .unwrap_or_default();
        let env: Vec<(String, String)> = optimizer_envs.into_iter().collect();
        let mut env = env;
        if runner == "go" {
            env.push(("GOPROXY".to_string(), "off".to_string()));
            env.push(("GOSUMDB".to_string(), "off".to_string()));
            env.push(("GOTOOLCHAIN".to_string(), "local".to_string()));
        }

        // Native MGC-installed Python packages must be visible to Python
        // test runners. Invalid/missing materializations fail closed.
        if matches!(runner.as_str(), "pytest" | "python" | "python3") {
            crate::commands::python_runtime::extend_native_python_env(
                &mut env,
                project_root,
                true,
            )?;
        }

        // P0-1: compat lane truyền runtime đã chọn (gate ở trên đã kiểm)
        // xuống mgc-exec để exemption shadow-path áp đúng runtime.
        let compat_runtime = match &compat {
            CompatMode::Native => None,
            CompatMode::Explicit(runtime) => Some(runtime.clone()),
        };
        let opts = mgc_exec::prelude::ExecOptions {
            cwd: Some(project_root.to_path_buf()),
            timeout: Some(std::time::Duration::from_secs(600)), // 10min test timeout
            execution_scope: Some(mgc_exec::allowlist::ExecutionScope::TestRunner),
            env,
            clean_env: false, // Preserve existing env
            log_path: Some(project_root.join(".magicore").join("exec.log")), // P0.7 FIX: Enable audit logging
            compat_runtime,
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

/// Map runner name → launcher policy for JS runtimes (None for non-JS runners).
/// Ánh xạ runner → launcher policy cho runtime JS (None với runner không phải JS).
/// cargo/go/pytest/flutter là toolchain hệ thống — mgc-exec allowlist đã kiểm soát.
fn test_runner_policy(runner: &str) -> Option<crate::commands::launcher_policy::LauncherPolicy> {
    use crate::commands::launcher_policy::{LauncherPolicy, Runtime};
    let runtime = match runner {
        "bun" => Runtime::Bun,
        "deno" => Runtime::Deno,
        "node" => Runtime::Node,
        _ => return None,
    };
    Some(LauncherPolicy::test_runner(runtime))
}

/// Detect test runner based on project manifest files — phát hiện test runner dựa trên file manifest
fn detect_test_runner(project_root: &Path) -> Result<Option<(String, Vec<String>)>> {
    // Check Cargo.toml (Rust) — kiểm tra Cargo.toml
    if read_project_manifest(project_root, "Cargo.toml")?.is_some() {
        return Ok(Some((
            "cargo".to_string(),
            vec![
                "test".to_string(),
                "--locked".to_string(),
                "--offline".to_string(),
            ],
        )));
    }

    // Check go.mod (Go) — kiểm tra go.mod
    if read_project_manifest(project_root, "go.mod")?.is_some() {
        return Ok(Some((
            "go".to_string(),
            vec![
                "test".to_string(),
                "-mod=readonly".to_string(),
                "./...".to_string(),
            ],
        )));
    }

    // Check pyproject.toml or setup.py (Python) — kiểm tra pyproject.toml hoặc setup.py
    if read_project_manifest(project_root, "pyproject.toml")?.is_some()
        || read_project_manifest(project_root, "setup.py")?.is_some()
    {
        // Try pytest first, fall back to python -m unittest — thử pytest trước
        // Note: pytest auto-discovers test_*.py and *_test.py in current directory
        // -s: no output capture, -v: verbose
        return Ok(Some((
            "pytest".to_string(),
            vec!["-s".to_string(), "-v".to_string()],
        )));
    }

    // Check pubspec.yaml (Flutter/Dart) — kiểm tra pubspec.yaml
    if read_project_manifest(project_root, "pubspec.yaml")?.is_some() {
        return Ok(Some((
            "flutter".to_string(),
            vec!["test".to_string(), "--no-pub".to_string()],
        )));
    }

    // SwiftPM runs tests only; MGC owns dependency resolution and installation.
    // (SwiftPM chỉ chạy test; MGC sở hữu resolve và cài dependency.)
    if read_project_manifest(project_root, "Package.swift")?.is_some() {
        return Ok(Some((
            "swift".to_string(),
            vec![
                "test".to_string(),
                "--skip-update".to_string(),
                "--disable-automatic-resolution".to_string(),
            ],
        )));
    }

    // Deno REMOVED from auto-detect (native-engine ruling 2026-09-10):
    // a deno.json project gets the honest migration error, not a silent
    // `deno test` spawn. Same for package.json scripts that used to be
    // forwarded to npm/pnpm/yarn/bun — the native web test lane runs
    // via project-local binaries (node_modules/.bin), never a PM.
    // Deno BỎ khỏi auto-detect: project deno.json nhận lỗi migration
    // trung thực, không spawn `deno test` âm thầm. Script package.json
    // cũng vậy — lane test web native chạy qua binary local của project,
    // không qua PM.
    let package_json_path = project_root.join("package.json");
    if let Some(test_script) = resolve_package_json_script(&package_json_path, "test")? {
        // Run the project's OWN test script through the native task
        // runner (run.rs gates rival runtimes/PMs the same way).
        // Chạy script test CỦA CHÍNH project qua task runner native
        // (run.rs chặn runtime đối thủ/PM y hệt).
        return Ok(Some((
            "mgc-internal-run-script".to_string(),
            vec![test_script],
        )));
    }

    // No test runner detected — không phát hiện test runner
    Ok(None)
}

/// Detect runtime for optimizer env loading based on test runner
/// Phát hiện runtime để load env optimizer dựa trên test runner
fn detect_test_runtime(
    project_root: &Path,
) -> Result<crate::commands::optimizer::runtime_detect::DetectedRuntime> {
    use crate::commands::optimizer::runtime_detect::{DetectedRuntime, detect_runtimes};

    let core = detect_test_core(project_root)?;

    // Use detect_runtimes from optimizer (core-aware)
    let runtimes = detect_runtimes(project_root, &core);
    Ok(runtimes
        .first()
        .cloned()
        .unwrap_or(DetectedRuntime::Unknown))
}

/// Use the shared fail-closed marker reader before applying legacy manifest inference.
/// Dùng chung marker reader fail-closed trước khi suy luận từ manifest cũ.
fn detect_test_core(project_root: &Path) -> Result<String> {
    if let Some(core) = mgc_config::project::ProjectConfig::read_core_marker(project_root)? {
        return Ok(core);
    }

    // Fallback for unmarked legacy projects. — Dự phòng cho project cũ chưa có marker.
    let read_manifest =
        |name: &str| mgc_config::project::read_regular_project_text(&project_root.join(name), name);
    let core = if read_manifest("Cargo.toml")?.is_some() {
        "lib"
    } else if let Some(content) = read_manifest("pyproject.toml")? {
        if content.contains("torch")
            || content.contains("pytorch")
            || content.contains("[tool.magicore]")
        {
            "ai"
        } else {
            "lib"
        }
    } else if read_manifest("pubspec.yaml")?.is_some() {
        "app"
    } else if read_manifest("package.json")?.is_some() {
        "web"
    } else {
        "lib"
    };
    Ok(core.to_string())
}

/// Resolve test script from mgc.toml — lấy test script từ mgc.toml
fn resolve_mgc_toml_script(path: &Path, script: &str) -> Result<Option<String>> {
    let Some(content) = mgc_config::project::read_regular_project_text(path, "project config")?
    else {
        return Ok(None);
    };
    let toml: toml::Value = toml::from_str(&content)?;
    Ok(toml
        .get("scripts")
        .and_then(|s| s.get(script))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

fn read_project_manifest(project_root: &Path, name: &str) -> Result<Option<String>> {
    mgc_config::project::read_regular_project_text(&project_root.join(name), name)
}

/// Resolve test script from package.json — lấy test script từ package.json
fn resolve_package_json_script(path: &Path, script: &str) -> Result<Option<String>> {
    let Some(content) = mgc_config::project::read_regular_project_text(path, "package manifest")?
    else {
        return Ok(None);
    };
    let manifest: serde_json::Value = serde_json::from_str(&content)?;
    Ok(manifest
        .get("scripts")
        .and_then(|s| s.get(script))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

#[cfg(test)]
#[path = "test/test_runner.rs"]
mod tests;
