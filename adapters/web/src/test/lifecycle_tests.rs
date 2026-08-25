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
fn lifecycle_timeout_kills_hung_process() {
    let project = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    write_package_script(package.path(), "python3 -c \"import time; time.sleep(2)\"");

    std::env::set_var("MGC_LIFECYCLE_TIMEOUT_SECS", "1");
    let err = LifecycleRunner::run_scripts(package.path(), project.path()).unwrap_err();
    std::env::remove_var("MGC_LIFECYCLE_TIMEOUT_SECS");
    assert!(err.to_string().contains("timed out"));
}
