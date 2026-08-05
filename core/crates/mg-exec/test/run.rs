//! Run tests — dry-run không spawn, audit log ghi args đã REDACTED, exit ≠ 0 bail
//! (00-index §5.5 dry-run, §5.4 audit, §5.8 fail → bail)

use mg_exec::prelude::*;
use std::fs;
use std::path::PathBuf;

fn tmp_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("mg-exec-test-{}", std::process::id()));
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
    };
    let report = run("cargo", &["--version".to_string()], &opts).unwrap();
    assert!(report.dry_run);
    assert_eq!(report.exit_code, 0);
}

#[test]
fn forbidden_tool_rejected_before_spawn() {
    let opts = ExecOptions {
        dry_run: false,
        log_path: None,
        cwd: None,
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
    };
    run("cargo", &["--token=leakme".to_string(), "--version".to_string()], &opts).unwrap();
    let content = fs::read_to_string(&log).unwrap();
    assert!(content.contains("cargo"));
    assert!(
        !content.contains("leakme"),
        "secret must never reach the audit log: {content}"
    );
    assert!(content.contains("[REDACTED]"));
    let _ = fs::remove_dir_all(tmp_dir());
}

#[test]
fn missing_tool_fails_with_clear_error() {
    // tool thuộc allowlist nhưng không tồn tại trên máy — spawn fail rõ ràng
    let opts = ExecOptions::default();
    let err = run("pio", &[], &opts).unwrap_err();
    assert!(err.to_string().contains("spawn") || err.to_string().contains("No such"));
}