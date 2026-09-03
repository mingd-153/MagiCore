//! CLI Surface and Error Handling Tests - Task 7/10
//! Tests: aliases, typos, offline, existing dir, error clarity
//! English-only errors, no warning spam

use std::process::Command;
use tempfile::TempDir;

fn find_mgc_binary() -> String {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let cli_dir = std::path::PathBuf::from(manifest_dir);
    let workspace_root = cli_dir.parent().expect("No parent dir");

    let debug = workspace_root.join("target/debug/mgc");
    let release = workspace_root.join("target/release/mgc");

    if debug.exists() {
        debug.to_str().unwrap().to_string()
    } else if release.exists() {
        release.to_str().unwrap().to_string()
    } else {
        panic!("mgc binary not found. Run: cargo build -p mgc");
    }
}

#[test]
fn test_cli_aliases_work() {
    // TEST: Short aliases should work (cre-w, cre-ai, cre-a, cre-l)
    let mgc = find_mgc_binary();

    // Test --help on aliases (should not fail)
    let aliases = vec![
        ("cre-w", "create-web alias"),
        ("cre-ai", "create-ai alias"),
        ("cre-a", "create-app alias"),
        ("cre-l", "create-lib alias"),
    ];

    for (alias, desc) in aliases {
        println!("\nTesting alias: {} ({})", alias, desc);

        let output = Command::new(&mgc)
            .arg(alias)
            .arg("--help")
            .output()
            .expect("mgc alias --help failed");

        assert!(
            output.status.success(),
            "Alias {} failed:\n{}",
            alias,
            String::from_utf8_lossy(&output.stderr)
        );

        println!("✅ {} works", alias);
    }
}

#[test]
fn test_typo_in_framework_name() {
    // TEST: Typo in framework name (nextjs@laster) should give clear error
    let temp = TempDir::new().unwrap();
    let mgc = find_mgc_binary();

    println!("\n=== Test typo: nextjs@laster ===");

    let output = Command::new(&mgc)
        .arg("create-web")
        .arg("nextjs@laster") // typo: latest → laster
        .arg("test-typo")
        .current_dir(temp.path())
        .output()
        .expect("mgc create-web failed to execute");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stdout, stderr);

    // Should fail with error
    assert!(
        !output.status.success(),
        "Should fail with typo, but succeeded"
    );

    // Error should be in English and clear
    assert!(
        combined.contains("Error")
            || combined.contains("error")
            || combined.contains("not found")
            || combined.contains("invalid"),
        "Error message unclear or not in English:\n{}",
        combined
    );

    // Should NOT have warning spam
    let warning_count = combined.matches("warning").count() + combined.matches("Warning").count();
    assert!(
        warning_count < 5,
        "Too many warnings ({}). Errors should be focused:\n{}",
        warning_count,
        combined
    );

    println!("✅ Typo error is clear and English-only");
}

#[test]
fn test_directory_already_exists() {
    // TEST: Creating project in existing directory should fail gracefully
    let temp = TempDir::new().unwrap();
    let mgc = find_mgc_binary();
    let project_name = "existing-dir";
    let project_path = temp.path().join(project_name);

    // Create directory first
    std::fs::create_dir(&project_path).unwrap();
    std::fs::write(project_path.join("README.md"), "existing content").unwrap();

    println!("\n=== Test create in existing dir ===");

    let output = Command::new(&mgc)
        .arg("create-web")
        .arg("nextjs")
        .arg(project_name)
        .current_dir(temp.path())
        .output()
        .expect("mgc create-web failed to execute");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should fail OR warn about existing directory
    if output.status.success() {
        // If it succeeds, should preserve existing files
        assert!(
            project_path.join("README.md").exists(),
            "Existing files should not be deleted"
        );
        println!("⚠️  Succeeded despite existing dir (acceptable if files preserved)");
    } else {
        // If it fails, error should mention "exists" or "already"
        assert!(
            combined.contains("exist") || combined.contains("already"),
            "Error should mention directory exists:\n{}",
            combined
        );
        println!("✅ Failed gracefully with clear error");
    }
}

#[test]
fn test_offline_mode_behavior() {
    // TEST: Offline/network error should give clear message
    // Simulate by using invalid registry URL
    let temp = TempDir::new().unwrap();
    let mgc = find_mgc_binary();
    let project = temp.path().join("offline-test");
    std::fs::create_dir(&project).unwrap();

    std::fs::write(
        project.join("package.json"),
        r#"{
  "name": "offline-test",
  "version": "1.0.0",
  "dependencies": {
    "nonexistent-package-12345": "1.0.0"
  }
}"#,
    )
    .unwrap();
    std::fs::write(project.join(".mgc.core"), "web\n").unwrap();

    println!("\n=== Test network/registry error ===");

    let output = Command::new(&mgc)
        .arg("install")
        .current_dir(&project)
        .output()
        .expect("mgc install failed to execute");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should fail
    assert!(
        !output.status.success(),
        "Should fail with nonexistent package"
    );

    // Error should mention network/registry/not found
    assert!(
        combined.contains("not found") || combined.contains("404") || combined.contains("failed"),
        "Error should clearly indicate package not found:\n{}",
        combined
    );

    println!("✅ Network/registry error is clear");
}

#[test]
fn test_invalid_command() {
    // TEST: Invalid command should show helpful error + available commands
    let mgc = find_mgc_binary();

    println!("\n=== Test invalid command ===");

    let output = Command::new(&mgc)
        .arg("invalid-command-xyz")
        .output()
        .expect("mgc invalid command failed to execute");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should fail
    assert!(!output.status.success(), "Invalid command should fail");

    // Error should be helpful
    assert!(
        combined.contains("invalid")
            || combined.contains("not found")
            || combined.contains("unknown"),
        "Error should indicate invalid command:\n{}",
        combined
    );

    // Should suggest help or list commands
    assert!(
        combined.contains("help") || combined.contains("--help") || combined.contains("available"),
        "Error should suggest help:\n{}",
        combined
    );

    println!("✅ Invalid command error is helpful");
}

#[test]
fn test_missing_required_argument() {
    // TEST: Missing required arg should show clear usage
    let mgc = find_mgc_binary();

    println!("\n=== Test missing required argument ===");

    // create-web without framework or project name
    let output = Command::new(&mgc)
        .arg("create-web")
        .output()
        .expect("mgc create-web failed to execute");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should fail
    assert!(
        !output.status.success(),
        "Should fail without required args"
    );

    // Error should show usage or mention missing argument
    assert!(
        combined.contains("Usage")
            || combined.contains("required")
            || combined.contains("argument"),
        "Error should indicate missing argument:\n{}",
        combined
    );

    println!("✅ Missing argument error shows usage");
}

#[test]
fn test_error_message_format() {
    // TEST: Errors should be focused (English-only, no warning spam, clear format)
    let mgc = find_mgc_binary();

    println!("\n=== Test error format quality ===");

    // Trigger error: invalid --core flag
    let output = Command::new(&mgc)
        .arg("install")
        .arg("--core")
        .arg("invalid-core-xyz")
        .output()
        .expect("mgc install failed to execute");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should fail
    assert!(!output.status.success(), "Should fail with invalid --core");

    // Check English-only (no mixed language)
    // This is a heuristic - just verify no obvious non-English characters
    let has_ascii =
        combined.is_ascii() || combined.chars().all(|c| c.is_ascii() || c.is_whitespace());
    assert!(
        has_ascii,
        "Error message should be English/ASCII:\n{}",
        combined
    );

    // Check for focused errors (not too verbose)
    let line_count = combined.lines().count();
    assert!(
        line_count < 50,
        "Error output too verbose ({} lines). Should be focused:\n{}",
        line_count,
        combined
    );

    // Check no excessive warnings
    let warning_count = combined.matches("warning").count() + combined.matches("Warning").count();
    assert!(
        warning_count < 5,
        "Too many warnings ({}). Errors should be clear, not spammy:\n{}",
        warning_count,
        combined
    );

    println!("✅ Error format is focused and English-only");
}
