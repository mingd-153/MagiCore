//! E2E lifecycle tests for optimizer consumption across web/ai/app/lib
//! REAL TESTS: Call mgc commands (dev/build/test/run) and verify child process output

#![allow(clippy::unwrap_used)] // Test code: unwrap acceptable for setup

use std::process::Command;
use tempfile::TempDir;

/// Helper: Find mgc binary
fn find_mgc_binary() -> std::path::PathBuf {
    std::env::var("CARGO_BIN_EXE_mgc")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .map(|p| p.join("mgc"))
        })
        .expect("mgc binary not found. Run: cargo build -p mgc")
}

#[test]
fn test_web_node_mgc_test_with_optimizer() {
    // REAL E2E: mgc test (web/node) → verify child npm/node receives optimizer env

    let temp = TempDir::new().unwrap();
    let project = temp.path();

    // Check npm available
    if Command::new("npm").arg("--version").output().is_err() {
        eprintln!("SKIP: npm not available");
        return;
    }

    // Create Node.js test project
    std::fs::write(
        project.join("package.json"),
        r#"{
  "name": "test-optimizer-web",
  "version": "1.0.0",
  "scripts": {
    "test": "node test.js"
  }
}"#,
    )
    .unwrap();

    std::fs::write(
        project.join("test.js"),
        r#"
// Test script that echoes optimizer env var
const marker = process.env.NODE_OPTIMIZER_MARKER || 'NOT_SET';
console.log('OPTIMIZER_STATUS:', marker);
process.exit(marker === 'NODE_OPTIMIZED' ? 0 : 1);
"#,
    )
    .unwrap();

    // Create .mgc.core marker
    std::fs::write(project.join(".mgc.core"), "web\n").unwrap();

    // Create optimizer config
    let optimizer_dir = project.join(".mgc-optimizer");
    std::fs::create_dir(&optimizer_dir).unwrap();
    std::fs::write(
        optimizer_dir.join("node_env.env"),
        "NODE_OPTIMIZER_MARKER=NODE_OPTIMIZED\n",
    )
    .unwrap();

    // Run mgc test (should load optimizer env and pass to npm test → node)
    let mgc = find_mgc_binary();
    let output = Command::new(&mgc)
        .arg("test")
        .current_dir(project)
        .output()
        .expect("mgc test failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    println!("=== mgc test output ===\n{}", combined);

    // VERIFY: Child node process received optimizer env var
    assert!(
        combined.contains("NODE_OPTIMIZED"),
        "INTEGRATION FAILED: mgc test did not pass optimizer env to child node process.\n\
        Expected: OPTIMIZER_STATUS: NODE_OPTIMIZED\n\
        Got: {}\n\
        This proves mgc → npm → node chain is BROKEN for optimizer env passing.",
        combined
    );

    println!("✅ Web (Node) mgc test → npm → node optimizer env verified");
}

#[test]
fn test_lib_rust_mgc_build_with_optimizer() {
    // REAL E2E: mgc build (lib/rust) → verify rustc compilation with optimizer flags
    // This is the BLOCKER 1 test (already verified), included here for completeness

    let temp = TempDir::new().unwrap();
    let project = temp.path();

    // Create Rust lib project
    std::fs::write(
        project.join("Cargo.toml"),
        r#"
[package]
name = "test-optimizer-lib"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "test_optimizer_lib"
path = "src/main.rs"
"#,
    )
    .unwrap();

    std::fs::create_dir(project.join("src")).unwrap();
    std::fs::write(
        project.join("src/main.rs"),
        r#"
fn main() {
    #[cfg(mgc_lib_optimized)]
    {
        println!("LIB_OPTIMIZER_ACTIVE");
    }
    #[cfg(not(mgc_lib_optimized))]
    {
        println!("LIB_OPTIMIZER_INACTIVE");
    }
}
"#,
    )
    .unwrap();

    // Create .mgc.core marker
    std::fs::write(project.join(".mgc.core"), "lib\n").unwrap();

    // Create optimizer config
    let optimizer_dir = project.join(".mgc-optimizer");
    std::fs::create_dir(&optimizer_dir).unwrap();
    std::fs::write(
        optimizer_dir.join("rust_cargo_profile.toml"),
        r#"
[build]
rustflags = ["-C", "opt-level=2", "--cfg", "mgc_lib_optimized"]
"#,
    )
    .unwrap();

    // Run mgc build
    let mgc = find_mgc_binary();
    let output = Command::new(&mgc)
        .arg("build")
        .current_dir(project)
        .output()
        .expect("mgc build failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("=== mgc build output ===\n{}{}", stdout, stderr);

    assert!(
        output.status.success(),
        "mgc build failed:\nSTDOUT: {}\nSTDERR: {}",
        stdout,
        stderr
    );

    // Run compiled binary
    let bin_path = project.join("target/debug/test_optimizer_lib");
    #[cfg(target_os = "windows")]
    let bin_path = project.join("target/debug/test_optimizer_lib.exe");

    assert!(bin_path.exists(), "Binary not found after mgc build");

    let bin_output = Command::new(&bin_path)
        .output()
        .expect("Failed to run binary");
    let bin_stdout = String::from_utf8_lossy(&bin_output.stdout);

    // VERIFY: Binary behavior reflects optimizer cfg
    assert!(
        bin_stdout.contains("LIB_OPTIMIZER_ACTIVE"),
        "INTEGRATION FAILED: Compiled lib does not reflect optimizer cfg.\n\
        Expected: LIB_OPTIMIZER_ACTIVE\n\
        Got: {}\n\
        This proves mgc build → cargo → rustc chain is BROKEN.",
        bin_stdout
    );

    println!("✅ Lib (Rust) mgc build → cargo → rustc optimizer verified");
}

#[test]
fn test_ai_python_mgc_test_with_optimizer() {
    // REAL E2E: mgc test (ai/python) → verify child pytest receives optimizer env
    // REQUIRES: python3 + pytest installed

    let temp = TempDir::new().unwrap();
    let project = temp.path();

    // Check python available
    // Check python3 available - skip test if missing
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("⚠️  SKIPPED: python3 not available");
        eprintln!("   This test requires python3 to verify AI optimizer.");
        eprintln!("   Status: SKIPPED (not FAIL)");
        return;
    }

    // Check pytest available - skip test if missing (CI should have it)
    if Command::new("pytest").arg("--version").output().is_err() {
        eprintln!("⚠️  SKIPPED: pytest not available");
        eprintln!("   This test requires pytest to verify AI optimizer.");
        eprintln!("   Install: pip install pytest, or provision in CI matrix.");
        eprintln!("   Status: SKIPPED (not FAIL)");
        return; // Skip test gracefully
    }

    // Create Python project with pyproject.toml
    std::fs::write(
        project.join("pyproject.toml"),
        r#"
[tool.magicore]
framework = "python-agent"

[project]
name = "test-optimizer-ai"
version = "0.1.0"
"#,
    )
    .unwrap();

    std::fs::write(
        project.join("test_optimizer.py"),
        r#"
import os
import sys

def test_optimizer_env():
    marker = os.environ.get('PYTHON_OPTIMIZER_MARKER', 'NOT_SET')
    print(f'OPTIMIZER_STATUS: {marker}')
    assert marker == 'PYTHON_OPTIMIZED', f'Expected PYTHON_OPTIMIZED, got {marker}'

if __name__ == '__main__':
    test_optimizer_env()
"#,
    )
    .unwrap();

    // Create .mgc.core marker
    std::fs::write(project.join(".mgc.core"), "ai\n").unwrap();

    // Create optimizer config (PyTorch adapter expects pytorch_runtime.env + pytorch_docker.env)
    let optimizer_dir = project.join(".mgc-optimizer");
    std::fs::create_dir(&optimizer_dir).unwrap();
    std::fs::write(
        optimizer_dir.join("pytorch_runtime.env"),
        "PYTHON_OPTIMIZER_MARKER=PYTHON_OPTIMIZED\n",
    )
    .unwrap();
    std::fs::write(
        optimizer_dir.join("pytorch_docker.env"),
        "# Docker config\n",
    )
    .unwrap();

    // Debug: Print what we created
    println!("=== Test Setup ===");
    println!("Project: {:?}", project);
    println!(".mgc.core exists: {}", project.join(".mgc.core").exists());
    println!("pytorch_runtime.env exists: {}", optimizer_dir.join("pytorch_runtime.env").exists());
    println!("test_optimizer.py exists: {}", project.join("test_optimizer.py").exists());

    // Run mgc test
    let mgc = find_mgc_binary();
    let output = Command::new(&mgc)
        .arg("test")
        .current_dir(project)
        .output()
        .expect("mgc test failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    println!("=== mgc test output ===\n{}", combined);

    // VERIFY: Child python/pytest process received optimizer env
    // Success indicators:
    // 1. Command succeeded
    // 2. Pytest test passed (marker assertion passed)
    // 3. Optimizer env reached child process (OPTIMIZER_STATUS in output)

    if !output.status.success() {
        eprintln!("❌ INTEGRATION FAILED: mgc test exited with error");
        eprintln!("   Exit code: {:?}", output.status.code());
        eprintln!("   Output: {}", combined);
        panic!("mgc test failed - see output above");
    }

    let test_passed = combined.contains("1 passed") && !combined.contains("FAILED");
    let env_marker_present = combined.contains("OPTIMIZER_STATUS: PYTHON_OPTIMIZED");

    println!("=== Verification ===");
    println!("test_passed: {}", test_passed);
    println!("env_marker_present: {}", env_marker_present);

    if !test_passed {
        eprintln!("❌ INTEGRATION FAILED: pytest test did not pass");
        eprintln!("   Expected: '1 passed' in output");
        eprintln!("   Got: {}", combined);
        panic!("pytest test failed");
    }

    if !env_marker_present {
        eprintln!("❌ INTEGRATION FAILED: Optimizer env marker not found");
        eprintln!("   Expected: 'OPTIMIZER_STATUS: PYTHON_OPTIMIZED'");
        eprintln!("   Got: {}", combined);
        panic!("Optimizer env not passed to pytest");
    }

    println!("✅ AI (Python) mgc test → pytest optimizer env verified");
}

#[test]
fn test_app_flutter_mgc_build_with_optimizer() {
    // REAL E2E: mgc build (app/flutter) → verify child flutter receives optimizer env
    // REQUIRES: flutter installed

    let temp = TempDir::new().unwrap();
    let project = temp.path();

    // Check flutter available - skip test if missing
    if Command::new("flutter").arg("--version").output().is_err() {
        eprintln!("⚠️  SKIPPED: flutter not available");
        eprintln!("   This test requires Flutter SDK to verify App optimizer.");
        eprintln!("   Install Flutter or provision in CI matrix.");
        eprintln!("   Status: SKIPPED (not FAIL)");
        return;
    }

    // Create minimal Flutter project
    std::fs::write(
        project.join("pubspec.yaml"),
        r#"
name: test_optimizer_flutter
version: 1.0.0
environment:
  sdk: ">=3.0.0 <4.0.0"
dev_dependencies:
  test: any
"#,
    )
    .unwrap();

    std::fs::create_dir_all(project.join("lib")).unwrap();
    std::fs::write(
        project.join("lib/main.dart"),
        r#"void main() {
  print('Flutter app');
}
"#,
    )
    .unwrap();

    // Create Flutter test that checks env
    std::fs::create_dir_all(project.join("test")).unwrap();
    std::fs::write(
        project.join("test/optimizer_test.dart"),
        r#"
import 'dart:io';
import 'package:test/test.dart';

void main() {
  test('optimizer env propagates to Flutter test', () {
    final marker = Platform.environment['FLUTTER_OPTIMIZER_MARKER'] ?? 'NOT_SET';
    print('OPTIMIZER_STATUS: $marker');
    expect(marker, equals('FLUTTER_OPTIMIZED'), reason: 'Optimizer env should propagate');
  });
}
"#,
    )
    .unwrap();

    // Create .mgc.core marker
    std::fs::write(project.join(".mgc.core"), "app\n").unwrap();

    // Create optimizer config
    let optimizer_dir = project.join(".mgc-optimizer");
    std::fs::create_dir(&optimizer_dir).unwrap();
    std::fs::write(
        optimizer_dir.join("flutter_env.env"),
        "FLUTTER_OPTIMIZER_MARKER=FLUTTER_OPTIMIZED\n",
    )
    .unwrap();

    // Run mgc test (Flutter test)
    let mgc = find_mgc_binary();
    let output = Command::new(&mgc)
        .arg("test")
        .current_dir(project)
        .output()
        .expect("mgc test failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    println!("=== mgc test output (Flutter) ===\n{}", combined);

    // VERIFY: mgc test succeeded AND optimizer env reached Flutter test process

    println!("=== Verification ===");
    println!("Exit success: {}", output.status.success());

    if !output.status.success() {
        eprintln!("❌ INTEGRATION FAILED: mgc test (Flutter) exited with error");
        eprintln!("   Exit code: {:?}", output.status.code());
        eprintln!("   Output: {}", combined);
        panic!("mgc test (Flutter) failed");
    }

    // Check 2: Optimizer env marker in Flutter test output
    let env_marker_present = combined.contains("OPTIMIZER_STATUS: FLUTTER_OPTIMIZED");
    println!("env_marker_present: {}", env_marker_present);

    if !env_marker_present {
        eprintln!("❌ INTEGRATION FAILED: Optimizer env marker not found in Flutter test");
        eprintln!("   Expected: 'OPTIMIZER_STATUS: FLUTTER_OPTIMIZED'");
        eprintln!("   This means env did not propagate to Flutter test process");
        eprintln!("   Output: {}", combined);
        panic!("Optimizer env not passed to Flutter test");
    }

    println!("✅ App (Flutter) mgc test → flutter test: optimizer env VERIFIED");
}
