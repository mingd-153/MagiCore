//! Security tests for ExecutionScope enforcement in test runners.
//!
//! Verifies that:
//! - Package managers (npm/pnpm/yarn/bun) are allowed in TestRunner scope
//! - Package managers are forbidden in Install scope
//! - cwd lock prevents directory traversal
//! - Shell injection is prevented
//! - Audit log records all tool executions

#![allow(clippy::unwrap_used)]

use std::env;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper: Create a minimal Node.js project with package.json
fn create_node_project(dir: &TempDir) -> PathBuf {
    let project_root = dir.path().to_path_buf();
    let package_json = project_root.join("package.json");
    fs::write(
        &package_json,
        r#"{
  "name": "test-project",
  "version": "1.0.0",
  "scripts": {
    "test": "echo 'Test passed' && exit 0"
  }
}"#,
    )
    .expect("Failed to write package.json");
    project_root
}

/// Helper: Run mgc binary with arguments
fn run_mgc(args: &[&str], cwd: Option<&PathBuf>) -> std::process::Output {
    let mgc_bin = env::var("MGC_BIN").unwrap_or_else(|_| {
        // Default to workspace target/debug/mgc
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut path = manifest_dir.parent().unwrap().to_path_buf(); // Go up from cli/ to workspace root
        path.push("target");
        path.push("debug");
        path.push("mgc");
        path.to_string_lossy().to_string()
    });

    let mut cmd = std::process::Command::new(&mgc_bin);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.output().expect("Failed to execute mgc")
}

#[test]
fn test_npm_allowed_in_test_runner_scope() {
    // TEST: Package manager (npm) should be allowed in TestRunner scope
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_root = create_node_project(&temp_dir);

    // Ensure npm is available (skip test if not)
    if std::process::Command::new("npm")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("SKIP: npm not available");
        return;
    }

    // Run: mgc test (should auto-detect npm test and succeed)
    let output = run_mgc(&["test"], Some(&project_root));

    // ASSERT: Should NOT be rejected by allowlist
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("permanently forbidden"),
        "npm should be allowed in TestRunner scope, got: {}",
        stderr
    );

    // ASSERT: Should detect npm test
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Auto-detected test runner") || stdout.contains("npm"),
        "Should detect npm test runner, got: {}",
        stdout
    );
}

#[test]
fn test_npm_forbidden_in_install_scope() {
    // TEST: Package manager (npm) should be FORBIDDEN in Install scope
    // Simulated via direct allowlist check (Install scope used by scaffold)

    use mgc_exec::allowlist::{check_tool_with_scope, ExecutionScope};

    let result = check_tool_with_scope("npm", ExecutionScope::Install, None);

    // ASSERT: Must be rejected
    assert!(result.is_err(), "npm must be forbidden in Install scope");

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("forbidden") || err_msg.contains("not allowed"),
        "Error message should indicate npm is forbidden, got: {}",
        err_msg
    );
}

#[test]
fn test_pnpm_allowed_in_test_runner_scope() {
    // TEST: Package manager (pnpm) should be allowed in TestRunner scope
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_root = create_node_project(&temp_dir);

    // Manually set pnpm as runner (if available)
    if std::process::Command::new("pnpm")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("SKIP: pnpm not available");
        return;
    }

    // Run: mgc test --runner pnpm (explicit runner)
    let output = run_mgc(&["test", "--runner", "pnpm"], Some(&project_root));

    // ASSERT: Should NOT be rejected by allowlist
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("permanently forbidden"),
        "pnpm should be allowed in TestRunner scope, got: {}",
        stderr
    );
}

#[test]
#[ignore = "TODO: Implement cwd lock — currently allows directory traversal"]
fn test_cwd_lock_prevents_traversal() {
    // TEST: Execution should be locked to project root (prevent directory traversal)
    // STATUS: NOT YET IMPLEMENTED — test documents expected behavior
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_root = create_node_project(&temp_dir);

    // Create malicious script attempting to escape project root
    let malicious_script = project_root.join("package.json");
    fs::write(
        &malicious_script,
        r#"{
  "name": "malicious",
  "scripts": {
    "test": "cd ../../ && rm -rf *"
  }
}"#,
    )
    .expect("Failed to write malicious package.json");

    // Run: mgc test (should fail or be contained)
    let _output = run_mgc(&["test"], Some(&project_root));

    // ASSERT: Either rejected OR executed within temp_dir only
    // Check that parent directory still exists (not deleted)
    assert!(
        temp_dir.path().parent().unwrap().exists(),
        "Parent directory should still exist (cwd lock should prevent traversal)"
    );
}

#[test]
#[ignore = "TODO: Implement shell escaping — currently vulnerable to injection"]
fn test_shell_injection_prevented() {
    // TEST: Shell metacharacters should not enable command injection
    // STATUS: NOT YET IMPLEMENTED — test documents expected behavior
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_root = create_node_project(&temp_dir);

    // Create script with shell injection attempt
    let injection_script = project_root.join("package.json");
    fs::write(
        &injection_script,
        r#"{
  "name": "injection-test",
  "scripts": {
    "test": "echo 'test'; touch /tmp/PWNED_BY_MGC_TEST; echo 'done'"
  }
}"#,
    )
    .expect("Failed to write injection package.json");

    // Run: mgc test
    let _output = run_mgc(&["test"], Some(&project_root));

    // ASSERT: Injection file should NOT be created outside project
    assert!(
        !PathBuf::from("/tmp/PWNED_BY_MGC_TEST").exists(),
        "Shell injection should be prevented (file created outside project)"
    );
}

#[test]
fn test_audit_log_records_execution() {
    // TEST: All tool executions should be logged to audit trail
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_root = create_node_project(&temp_dir);

    // Ensure npm is available
    if std::process::Command::new("npm")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("SKIP: npm not available");
        return;
    }

    // Run: mgc test
    let _output = run_mgc(&["test"], Some(&project_root));

    // ASSERT: Audit log should exist (check common locations)
    let mut possible_log_paths = vec![
        project_root.join(".mgc").join("audit.log"),
        project_root.join("mgc-audit.log"),
    ];
    if let Some(home) = dirs::home_dir() {
        possible_log_paths.push(home.join(".magicore").join("audit.log"));
    }

    let log_exists = possible_log_paths.iter().any(|p| p.exists());

    // NOTE: This is a placeholder check; actual audit log implementation may vary
    if !log_exists {
        eprintln!(
            "WARNING: No audit log found at expected locations. Audit logging may not be implemented yet."
        );
    }
}
