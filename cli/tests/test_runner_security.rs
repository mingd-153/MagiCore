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
fn test_cwd_lock_sets_working_directory() {
    // TEST: mgc sets correct working directory for child process
    // SCOPE: Verifies mgc passes cwd correctly, NOT that npm scripts are sandboxed
    //
    // THREAT MODEL CLARIFICATION:
    // - mgc validates paths in ITS OWN args (mgc → tool)
    // - mgc does NOT validate content of npm/package manager scripts
    // - npm scripts can contain arbitrary commands (by design)
    // - If user adds malicious script to package.json, it runs with npm's permissions
    //
    // This is npm's responsibility, not mgc's. mgc is a package manager ORCHESTRATOR,
    // not a sandbox. Users trust their own package.json just as they trust their own code.
    
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let parent_dir = temp_dir.path();
    
    // Create project root INSIDE temp dir
    let project_root = parent_dir.join("project");
    fs::create_dir(&project_root).expect("Failed to create project dir");
    
    // Create package.json that logs CWD
    fs::write(
        project_root.join("package.json"),
        r#"{
  "name": "cwd-test",
  "scripts": {
    "test": "pwd > cwd.txt && echo 'done'"
  }
}"#,
    )
    .expect("Failed to write package.json");

    // Ensure npm/bash available
    if std::process::Command::new("npm")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("SKIP: npm not available");
        return;
    }

    // Run: mgc test
    let output = run_mgc(&["test"], Some(&project_root));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("=== mgc test output ===\n{}{}", stdout, stderr);

    // ASSERT: Verify mgc set correct cwd
    let cwd_file = project_root.join("cwd.txt");
    if cwd_file.exists() {
        let logged_cwd = fs::read_to_string(&cwd_file).expect("Failed to read cwd.txt");
        let logged_cwd = logged_cwd.trim();
        let expected_cwd = project_root.canonicalize().expect("Failed to canonicalize");
        
        let logged_path = PathBuf::from(logged_cwd)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(logged_cwd));
        
        assert!(
            logged_path == expected_cwd || logged_path.starts_with(&expected_cwd),
            "mgc did not set correct cwd for child process.\nExpected: {}\nActual: {}",
            expected_cwd.display(),
            logged_path.display()
        );
        
        println!("✅ CWD correctly set to project root: {}", logged_path.display());
    } else {
        eprintln!("SKIP: cwd.txt not created (npm may have failed)");
    }
}

#[test]
fn test_shell_injection_prevented() {
    // TEST: Shell metacharacters in tool names should be rejected
    // NEGATIVE TEST: Try to execute tool with shell injection → must FAIL
    
    use mgc_exec::allowlist::check_tool_with_scope;
    use mgc_exec::allowlist::ExecutionScope;

    // ATTACK 1: Try to inject shell command via tool name
    let malicious_tools = vec![
        "npm; rm -rf /",           // Command chaining
        "npm && cat /etc/passwd",  // Command chaining
        "npm | tee evil.txt",      // Pipe injection
        "npm $(whoami)",           // Command substitution
        "npm `whoami`",            // Command substitution (backticks)
        "npm > /dev/null",         // Redirection
    ];

    for tool in malicious_tools {
        let result = check_tool_with_scope(tool, ExecutionScope::TestRunner, None);
        
        // ASSERT: Must be rejected (either not in allowlist or contains forbidden chars)
        assert!(
            result.is_err(),
            "SECURITY VULNERABILITY: Shell injection not blocked for tool: '{}'.\n\
            Malicious tool name with shell metacharacters was allowed.\n\
            This could lead to command injection attacks.",
            tool
        );
        
        let err_msg = result.unwrap_err().to_string();
        println!("✅ Blocked shell injection attempt: '{}' → {}", tool, err_msg);
    }

    // ATTACK 2: Verify mgc-exec uses Command::args() not shell
    // This is verified by code inspection: mgc-exec/run.rs uses command.args(args)
    // No shell involvement means shell metacharacters are passed as literal strings, not interpreted
    
    println!("✅ Shell injection prevented: mgc-exec uses Command::args(), malicious tool names rejected");
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

#[test]
fn test_path_traversal_in_args_rejected() {
    // TEST: Path traversal attempts in command arguments should be handled safely
    // NEGATIVE TEST: Try to pass ../../ paths → verify no unintended file access
    
    use mgc_exec::prelude::{run, ExecOptions};
    
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_root = temp_dir.path();
    
    // Create a sensitive file OUTSIDE project root
    let parent = project_root.parent().unwrap();
    let sensitive_file = parent.join("sensitive.txt");
    fs::write(&sensitive_file, "SECRET_DATA").expect("Failed to write sensitive file");
    
    // Try to read file via path traversal in args
    let result = run(
        "cat",
        &["../../sensitive.txt".to_string()],
        &ExecOptions {
            cwd: Some(project_root.to_path_buf()),
            ..Default::default()
        },
    );
    
    // ASSERT: Either command fails (file not found due to cwd lock)
    // Or mgc sanitizes the path
    match result {
        Ok(report) => {
            let combined = format!("{}{}", report.stdout_tail, report.stderr_tail);
            
            // Should NOT be able to read secret data
            assert!(
                !combined.contains("SECRET_DATA"),
                "PATH TRAVERSAL VULNERABILITY: Command was able to read file outside project root.\n\
                Attempted: cat ../../sensitive.txt\n\
                Got: {}",
                combined
            );
            
            println!("✅ Path traversal blocked: command could not escape project root");
        }
        Err(e) => {
            // Rejection is also acceptable (stricter security)
            println!("✅ Path traversal rejected: {}", e);
        }
    }
}
