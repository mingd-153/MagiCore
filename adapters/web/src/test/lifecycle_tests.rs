#![cfg(test)]
#![allow(clippy::unwrap_used)]
// Tests mutate env vars single-threaded per test process (edition 2024 unsafe rule).
// Test đổi env var đơn luồng theo từng process test (luật unsafe edition 2024).
#![allow(unsafe_code)]

// Lifecycle tests for core-web — kept outside production source bodies.
// Test lifecycle của core-web — tách khỏi thân file production để dễ maintain.
use super::*;

fn write_package_script(package: &Path, script: &str) {
    let manifest = serde_json::json!({
        "scripts": {
            "postinstall": script,
        }
    });
    std::fs::write(package.join("package.json"), manifest.to_string()).unwrap();
}

#[test]
fn lifecycle_path_env_prepends_node_modules_bin_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin).unwrap();

    let path_env = lifecycle_path_env(dir.path()).unwrap();
    let paths: Vec<_> = std::env::split_paths(&path_env).collect();

    assert_eq!(paths.first(), Some(&bin));
}

#[test]
fn lifecycle_errors_on_invalid_package_json() {
    let project = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    std::fs::write(package.path().join("package.json"), "{not-json").unwrap();

    let err = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();

    assert!(
        err.to_string()
            .contains("failed to parse package.json for lifecycle"),
        "unexpected error: {err}"
    );
}

#[test]
fn lifecycle_rejects_external_package_manager_wrappers() {
    let project = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    std::fs::write(
        package.path().join("package.json"),
        r#"{"scripts":{"postinstall":"npm run postinstall:inner"}}"#,
    )
    .unwrap();

    let err = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();
    assert!(err.to_string().contains("delegates to 'npm'"));
}

#[test]
fn lifecycle_rejects_pm_wrappers_after_shell_separators() {
    let project = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    std::fs::write(
        package.path().join("package.json"),
        r#"{"scripts":{"postinstall":"node build.js && /usr/bin/pnpm install"}}"#,
    )
    .unwrap();

    let err = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();
    assert!(err.to_string().contains("delegates to 'pnpm'"));
}

#[test]
fn lifecycle_rejects_package_manager_javascript_entrypoint() {
    let project = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    write_package_script(
        package.path(),
        "node ./node_modules/pnpm/bin/pnpm.cjs install",
    );

    let error = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();
    assert!(error.to_string().contains("delegates to 'pnpm'"));
}

#[test]
fn lifecycle_rejects_shell_control_tokens() {
    let project = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    std::fs::write(
        package.path().join("package.json"),
        r#"{"scripts":{"postinstall":"node build.js; node post.js"}}"#,
    )
    .unwrap();

    let err = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();
    assert!(err.to_string().contains("unsupported lifecycle script"));
}

#[test]
#[cfg(unix)]
fn lifecycle_runs_simple_script_without_shell() {
    let project = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    let marker = package.path().join("marker.txt");
    write_package_script(
        package.path(),
        "python3 -c \"from pathlib import Path; Path('marker.txt').write_text('ok')\"",
    );

    LifecycleRunner::run_scripts(package.path(), project.path()).unwrap();
    assert!(marker.exists());
}

#[test]
#[cfg(unix)]
fn lifecycle_accepts_leading_env_assignment() {
    let project = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    write_package_script(
        package.path(),
        "MGC_LIFECYCLE_TEST=ok python3 -c \"import os; assert os.environ.get('MGC_LIFECYCLE_TEST') == 'ok'\"",
    );

    LifecycleRunner::run_scripts(package.path(), project.path()).unwrap();
}

#[test]
#[cfg(unix)]
fn lifecycle_execution_uses_the_policy_checked_manifest_snapshot() {
    let project = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    let marker = package.path().join("snapshot-marker.txt");
    let script = format!(
        "python3 -c \"from pathlib import Path; Path('{}').write_text('checked')\"",
        marker.display()
    );
    write_package_script(package.path(), &script);

    let checked = load_package_scripts(package.path()).unwrap();
    write_package_script(package.path(), "npm install attacker-controlled");

    LifecycleRunner::run_scripts_with_snapshot(package.path(), project.path(), checked).unwrap();
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "checked");
}

#[test]
#[cfg(unix)]
fn lifecycle_core_identity_tampering_fails_the_install_path() {
    // A lifecycle process can write project files; it must not turn a
    // successful install into a false success after changing core ownership.
    // (Lifecycle có thể ghi file project; thay core phải làm install fail.)
    let project = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::new("web-owner", "web")
        .save(project.path())
        .unwrap();
    let package = tempfile::tempdir().unwrap();
    let marker = project
        .path()
        .join(mgc_config::project::ProjectConfig::CORE_MARKER_FILE);
    let script = format!(
        "python3 -c \"from pathlib import Path; Path('{}').write_text('ai')\"",
        marker.display()
    );
    write_package_script(package.path(), &script);

    let error = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();
    assert!(
        error.to_string().contains("identity") && error.to_string().contains("lifecycle scripts"),
        "identity tampering must fail closed, got: {error}"
    );
    assert_eq!(
        std::fs::read_to_string(&marker).expect("read restored core marker"),
        "web\n",
        "failed lifecycle must restore the pre-install core identity marker"
    );
    assert_eq!(
        mgc_config::project::ProjectConfig::detect_core(project.path())
            .expect("detect core after identity rollback")
            .as_deref(),
        Some("web"),
        "restored project identity must remain usable after install abort"
    );
}

#[test]
#[cfg(unix)]
fn lifecycle_identity_tampering_stops_later_hooks_before_execution() {
    let project = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::new("web-owner", "web")
        .save(project.path())
        .unwrap();
    let package = tempfile::tempdir().unwrap();
    let marker = project
        .path()
        .join(mgc_config::project::ProjectConfig::CORE_MARKER_FILE);
    let later_hook_sentinel = project.path().join("later-hook-ran.txt");
    let preinstall = format!(
        "python3 -c \"from pathlib import Path; Path('{}').write_text('ai')\"",
        marker.display()
    );
    let install = format!(
        "python3 -c \"from pathlib import Path; Path('{}').write_text('ran')\"",
        later_hook_sentinel.display()
    );
    let manifest = serde_json::json!({
        "scripts": {
            "preinstall": preinstall,
            "install": install,
        }
    });
    std::fs::write(package.path().join("package.json"), manifest.to_string()).unwrap();

    let error = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();

    assert!(error.to_string().contains("identity"));
    assert!(
        !later_hook_sentinel.exists(),
        "later lifecycle hooks must not run"
    );
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "web\n");
}

#[test]
#[cfg(unix)]
fn lifecycle_cannot_delete_core_marker_and_rely_on_config_fallback() {
    let project = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::new("web-owner", "web")
        .save(project.path())
        .unwrap();
    let package = tempfile::tempdir().unwrap();
    let marker = project
        .path()
        .join(mgc_config::project::ProjectConfig::CORE_MARKER_FILE);
    let script = format!(
        "python3 -c \"from pathlib import Path; Path('{}').unlink()\"",
        marker.display()
    );
    write_package_script(package.path(), &script);

    let error = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();

    assert!(error.to_string().contains("identity"));
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap(),
        "web\n",
        "a removed ownership marker must be restored even when mgc.toml still identifies Web"
    );
}

#[test]
#[cfg(unix)]
fn lifecycle_timeout_kills_hung_process() {
    let project = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    write_package_script(package.path(), "python3 -c \"import time; time.sleep(2)\"");

    unsafe { std::env::set_var("MGC_LIFECYCLE_TIMEOUT_SECS", "1") };
    let err = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();
    unsafe { std::env::remove_var("MGC_LIFECYCLE_TIMEOUT_SECS") };
    assert!(err.to_string().contains("timed out"));
}

#[test]
fn lifecycle_rejects_rival_runtime_supply_chain_spawn_f_a() {
    // F-A (2026-09-10 supply-chain audit): a dependency package's
    // postinstall MUST NOT be able to spawn deno (previously deno sat on
    // ALLOWED_TOOLS → arbitrary code exec during install). bun was already
    // blocked as a PM; deno is the newly closed hole.
    // Postinstall của dependency KHÔNG được spawn deno (trước đây deno
    // nằm trong ALLOWED_TOOLS → thực thi code tùy ý lúc install).
    for script in ["deno run evil.ts", "bun run evil.ts"] {
        let project = tempfile::tempdir().unwrap();
        let package = tempfile::tempdir().unwrap();
        write_package_script(package.path(), script);

        let err = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("forbidden")
                || msg.contains("supply-chain guard")
                || msg.contains("permanently forbidden")
                || msg.contains("refuses package-manager wrappers"),
            "lifecycle must refuse rival runtime spawn '{script}': {msg}"
        );
    }
}

#[test]
fn lifecycle_failing_script_is_an_error_not_a_warning() {
    // A lifecycle script that RUNS but exits non-zero must fail the
    // caller: install treats this Err as fatal (npm parity — a red
    // postinstall is never a green install).
    // (Script chạy nhưng exit lỗi phải là Err.)
    let project = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    write_package_script(package.path(), "node -e \"process.exit(1)\"");
    let err = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();
    assert!(
        err.to_string().contains("failed"),
        "unexpected error: {err}"
    );
}
