#![allow(clippy::unwrap_used)]
//! Run tests — dry-run không spawn, audit log ghi args đã REDACTED, exit ≠ 0 bail
//! (00-index §5.5 dry-run, §5.4 audit, §5.8 fail → bail)

use mg_exec::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

fn tmp_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let d = std::env::temp_dir().join(format!("mg-exec-test-{}-{nanos}", std::process::id()));
    let _ = fs::create_dir_all(&d);
    d
}

#[test]
fn dry_run_does_not_spawn_and_prints() {
    // cargo --version sẽ chạy thật nếu không dry-run — dry_run=true không spawn
    let opts = ExecOptions {
        dry_run: true,
        log_path: None,
        cwd: None,
        ..Default::default()
    };
    let report = run("cargo", &["--version".to_string()], &opts).unwrap();
    assert!(report.dry_run);
    assert_eq!(report.exit_code, 0);
}

#[test]
fn inherited_dry_run_uses_same_report_contract() {
    let opts = ExecOptions {
        dry_run: true,
        log_path: None,
        cwd: None,
        ..Default::default()
    };
    let report = run_inherited("git", &["--version".to_string()], &opts).unwrap();
    assert!(report.dry_run);
    assert_eq!(report.cmd, "git");
    assert_eq!(report.exit_code, 0);
}

#[test]
fn inherited_run_rejects_forbidden_pm_before_spawn() {
    let opts = ExecOptions {
        clean_env: true,
        ..Default::default()
    };
    let err = run_inherited("pnpm", &["install".to_string()], &opts).unwrap_err();
    assert!(err.to_string().contains("forbidden"), "{err}");
}

#[test]
fn reports_process_tree_guard_capability_truthfully() {
    assert_eq!(process_tree_guard_available(), cfg!(unix));
}

#[test]
fn forbidden_tool_rejected_before_spawn() {
    let opts = ExecOptions {
        dry_run: false,
        log_path: None,
        cwd: None,
        ..Default::default()
    };
    let err = run("npm", &["install".to_string()], &opts).unwrap_err();
    assert!(err.to_string().contains("forbidden"));
}

#[test]
fn unknown_tool_rejected() {
    let opts = ExecOptions::default();
    assert!(run("definitely-not-a-real-tool-xyz", &[], &opts).is_err());
}

#[test]
fn audit_log_written_with_redacted_args() {
    let log = tmp_dir().join("exec.log");
    let _ = fs::remove_file(&log);
    let opts = ExecOptions {
        dry_run: true,
        log_path: Some(log.clone()),
        cwd: None,
        ..Default::default()
    };
    run(
        "cargo",
        &["--token=leakme".to_string(), "--version".to_string()],
        &opts,
    )
    .unwrap();
    let content = fs::read_to_string(&log).unwrap();
    assert!(content.contains("cargo"));
    assert!(
        !content.contains("leakme"),
        "secret must never reach the audit log: {content}"
    );
    assert!(content.contains("[REDACTED]"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_tool_fails_with_clear_error() {
    // tool thuộc allowlist nhưng không tồn tại trên máy — spawn fail rõ ràng
    let opts = ExecOptions::default();
    let err = run("pio", &[], &opts).unwrap_err();
    assert!(err.to_string().contains("spawn") || err.to_string().contains("No such"));
}

#[test]
#[cfg(unix)]
fn command_timeout_kills_hung_tool() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("timeout-bin");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let fake_cargo = dir.join("cargo");
    fs::write(&fake_cargo, "#!/bin/sh\n/bin/sleep 2\n").unwrap();
    let mut permissions = fs::metadata(&fake_cargo).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_cargo, permissions).unwrap();

    let opts = ExecOptions {
        clean_env: true,
        timeout: Some(Duration::from_millis(50)),
        ..Default::default()
    };

    let err = run(
        fake_cargo.to_str().unwrap(),
        &["--version".to_string()],
        &opts,
    )
    .unwrap_err();
    assert!(err.to_string().contains("timed out"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[cfg(unix)]
fn clean_env_blocks_forbidden_pm_spawned_by_child_path_lookup() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("child-pm-bin");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let fake_cargo = dir.join("cargo");
    fs::write(&fake_cargo, "#!/bin/sh\nnpm --version\n").unwrap();
    let mut permissions = fs::metadata(&fake_cargo).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_cargo, permissions).unwrap();

    let opts = ExecOptions {
        clean_env: true,
        env: vec![("PATH".to_string(), dir.display().to_string())],
        timeout: Some(Duration::from_secs(2)),
        ..Default::default()
    };

    let err = run(
        fake_cargo.to_str().unwrap(),
        &["--version".to_string()],
        &opts,
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("MegaGate blocked forbidden package manager: npm")
            || err
                .to_string()
                .contains("references forbidden package manager 'npm'"),
        "unexpected error: {err}"
    );
}

#[test]
#[cfg(unix)]
fn clean_env_kills_forbidden_pm_spawned_by_absolute_child_path() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("absolute-child-pm-bin");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let fake_cargo = dir.join("cargo");
    let fake_npm = dir.join("npm");
    fs::write(&fake_npm, "#!/bin/sh\n/bin/sleep 2\n").unwrap();
    fs::write(
        &fake_cargo,
        format!("#!/bin/sh\n\"{}\"\n", fake_npm.display()),
    )
    .unwrap();
    for path in [&fake_cargo, &fake_npm] {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    let opts = ExecOptions {
        clean_env: true,
        timeout: Some(Duration::from_secs(2)),
        ..Default::default()
    };

    let err = run(
        fake_cargo.to_str().unwrap(),
        &["--version".to_string()],
        &opts,
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("references forbidden package manager 'npm'")
            || err
                .to_string()
                .contains("forbidden package manager 'npm' spawned"),
        "unexpected error: {err}"
    );
}

#[test]
#[cfg(unix)]
fn project_binary_rejects_forbidden_binary_name() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("project-bin-forbidden-name");
    fs::create_dir_all(&dir).unwrap();
    let fake_npm = dir.join("npm");
    fs::write(&fake_npm, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = fs::metadata(&fake_npm).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_npm, permissions).unwrap();

    let opts = ExecOptions {
        clean_env: true,
        ..Default::default()
    };
    let err = run_project_binary(&fake_npm, &[], &opts).unwrap_err();
    assert!(err.to_string().contains("forbidden package manager"));
}

#[test]
#[cfg(unix)]
fn project_binary_rejects_script_that_references_forbidden_pm() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("project-bin-script-pm");
    fs::create_dir_all(&dir).unwrap();
    let fake_bin = dir.join("tool");
    fs::write(&fake_bin, "#!/bin/sh\n/usr/bin/npm --version\n").unwrap();
    let mut permissions = fs::metadata(&fake_bin).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_bin, permissions).unwrap();

    let opts = ExecOptions {
        clean_env: true,
        ..Default::default()
    };
    let err = run_project_binary(&fake_bin, &[], &opts).unwrap_err();
    assert!(err
        .to_string()
        .contains("references forbidden package manager"));
}

#[test]
#[cfg(unix)]
fn inherited_project_binary_rejects_script_that_references_forbidden_pm() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("project-bin-inherited-script-pm");
    fs::create_dir_all(&dir).unwrap();
    let fake_bin = dir.join("tool");
    fs::write(&fake_bin, "#!/bin/sh\n/usr/bin/yarn --version\n").unwrap();
    let mut permissions = fs::metadata(&fake_bin).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_bin, permissions).unwrap();

    let opts = ExecOptions {
        clean_env: true,
        ..Default::default()
    };
    let err = run_project_binary_inherited(&fake_bin, &[], &opts).unwrap_err();
    assert!(err
        .to_string()
        .contains("references forbidden package manager"));
}

#[test]
fn npm_is_blocked_even_inside_react_native_subdir() {
    let base = std::env::temp_dir().join(format!("mgexec-rn-{}", std::process::id()));
    let rn = base.join("react-native");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&rn).unwrap();
    fs::write(base.join("mg.toml"), "[app]\nlanguage = \"multi\"\n").unwrap();
    fs::write(
        rn.join("package.json"),
        "{\"dependencies\": {\"react-native\": \"0.7x\"}}",
    )
    .unwrap();

    let opts = ExecOptions {
        cwd: Some(rn.clone()),
        clean_env: true,
        ..Default::default()
    };
    let err = run_inherited("npm", &["install".to_string()], &opts).unwrap_err();
    assert!(
        err.to_string().contains("permanently forbidden"),
        "npm inside react-native subdir must stay blocked, got: {err}"
    );

    let outside = ExecOptions {
        clean_env: true,
        ..Default::default()
    };
    let err = run_inherited("npm", &["install".to_string()], &outside).unwrap_err();
    assert!(
        err.to_string().contains("permanently forbidden"),
        "npm outside react-native subdir must be rejected, got: {err}"
    );

    let _ = fs::remove_dir_all(&base);
}
