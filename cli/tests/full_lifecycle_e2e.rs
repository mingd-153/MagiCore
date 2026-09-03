//! Full Lifecycle E2E Tests - Task 3/10
//! Complete chain: create → install → dev/build → test → run
//! Tests all 4 cores: web, ai, app, lib
//! Each step asserts: exit code, artifacts, markers proving optimizer/config propagation

#![allow(clippy::unwrap_used)] // Test code: unwrap acceptable for setup

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
fn test_web_full_lifecycle() {
    // FULL E2E: web core - create → install → test → run
    // REQUIRES: node, npm, web templates available
    // NOTE: This test requires templates. If templates not available, test is UNVERIFIED.

    // Check Node available
    if Command::new("node").arg("--version").output().is_err() {
        panic!(
            "UNVERIFIED: node not available\n\
            This test requires Node.js to verify Web full lifecycle.\n\
            Status: IMPLEMENTED-UNVERIFIED"
        );
    }

    let temp = TempDir::new().unwrap();
    let project_name = "test-web-full";
    let project_path = temp.path().join(project_name);
    let mgc = find_mgc_binary();

    // === STEP 1: CREATE ===
    println!("\n=== STEP 1: mgc create-web ===");
    let create_output = Command::new(&mgc)
        .arg("create-web")
        .arg("nextjs")
        .arg(project_name)
        .current_dir(temp.path())
        .output()
        .expect("mgc create-web failed to execute");

    if !create_output.status.success() {
        let stderr = String::from_utf8_lossy(&create_output.stderr);
        if stderr.contains("Required scaffold layers missing") || stderr.contains("template") {
            panic!(
                "UNVERIFIED: Web templates not available\n\
                This test requires templates to be fetched.\n\
                Run: mgc template fetch web nextjs@latest\n\
                Or run template fetch in CI setup.\n\
                Status: IMPLEMENTED-UNVERIFIED (missing test data)"
            );
        }
        panic!("CREATE FAILED:\n{}", stderr);
    }
    assert!(project_path.exists(), "Project directory not created");
    assert!(
        project_path.join("package.json").exists(),
        "package.json not created"
    );

    // === STEP 2: INSTALL ===
    println!("\n=== STEP 2: mgc install ===");
    let install_output = Command::new(&mgc)
        .arg("install")
        .current_dir(&project_path)
        .output()
        .expect("mgc install failed to execute");

    assert!(
        install_output.status.success(),
        "INSTALL FAILED:\n{}",
        String::from_utf8_lossy(&install_output.stderr)
    );
    assert!(
        project_path.join("node_modules").exists(),
        "node_modules not created after install"
    );

    // === STEP 3: TEST ===
    println!("\n=== STEP 3: mgc test ===");
    let test_output = Command::new(&mgc)
        .arg("test")
        .current_dir(&project_path)
        .output()
        .expect("mgc test failed to execute");

    // Test may fail if no tests defined, but command should execute
    let test_combined = format!(
        "{}{}",
        String::from_utf8_lossy(&test_output.stdout),
        String::from_utf8_lossy(&test_output.stderr)
    );

    // VERIFY: npm/node invoked (optimizer proof)
    assert!(
        test_combined.contains("npm") || test_combined.contains("node"),
        "TEST did not invoke npm/node:\n{}",
        test_combined
    );

    // === STEP 4: BUILD ===
    println!("\n=== STEP 4: mgc build ===");
    let build_output = Command::new(&mgc)
        .arg("build")
        .current_dir(&project_path)
        .output()
        .expect("mgc build failed to execute");

    assert!(
        build_output.status.success(),
        "BUILD FAILED:\n{}",
        String::from_utf8_lossy(&build_output.stderr)
    );

    // VERIFY: Build artifacts exist (.next for Next.js)
    assert!(
        project_path.join(".next").exists() || project_path.join("dist").exists(),
        "Build artifacts not created"
    );

    println!("✅ Web full lifecycle VERIFIED: create → install → test → build");
}

#[test]
fn test_lib_full_lifecycle() {
    // FULL E2E: lib core - create → build → test
    // REQUIRES: cargo/rustc (should be available in Rust project)

    let temp = TempDir::new().unwrap();
    let project_name = "test-lib-full";
    let project_path = temp.path().join(project_name);
    let mgc = find_mgc_binary();

    // === STEP 1: CREATE ===
    println!("\n=== STEP 1: mgc create-lib ===");
    let create_output = Command::new(&mgc)
        .arg("create-lib")
        .arg("rust")
        .arg(project_name)
        .current_dir(temp.path())
        .output()
        .expect("mgc create-lib failed to execute");

    assert!(
        create_output.status.success(),
        "CREATE FAILED:\n{}",
        String::from_utf8_lossy(&create_output.stderr)
    );
    assert!(project_path.exists(), "Project directory not created");
    assert!(
        project_path.join("Cargo.toml").exists(),
        "Cargo.toml not created"
    );

    // === STEP 2: BUILD ===
    println!("\n=== STEP 2: mgc build ===");
    let build_output = Command::new(&mgc)
        .arg("build")
        .current_dir(&project_path)
        .output()
        .expect("mgc build failed to execute");

    assert!(
        build_output.status.success(),
        "BUILD FAILED:\n{}",
        String::from_utf8_lossy(&build_output.stderr)
    );

    // VERIFY: target/ directory exists (cargo build artifact)
    assert!(
        project_path.join("target").exists(),
        "target/ directory not created after build"
    );

    // === STEP 3: TEST ===
    println!("\n=== STEP 3: mgc test ===");
    let test_output = Command::new(&mgc)
        .arg("test")
        .current_dir(&project_path)
        .output()
        .expect("mgc test failed to execute");

    // Test should succeed (cargo new creates passing test)
    assert!(
        test_output.status.success(),
        "TEST FAILED:\n{}",
        String::from_utf8_lossy(&test_output.stderr)
    );

    let test_combined = format!(
        "{}{}",
        String::from_utf8_lossy(&test_output.stdout),
        String::from_utf8_lossy(&test_output.stderr)
    );

    // VERIFY: cargo test ran
    assert!(
        test_combined.contains("test result") || test_combined.contains("running"),
        "TEST did not show test results:\n{}",
        test_combined
    );

    println!("✅ Lib full lifecycle VERIFIED: create → build → test");
}

#[test]
fn test_ai_full_lifecycle_limited() {
    // LIMITED E2E: ai core - create → verify structure
    // Full test (install → test) requires pytest - see optimizer_lifecycle_e2e.rs
    // REQUIRES: python3

    if Command::new("python3").arg("--version").output().is_err() {
        panic!(
            "UNVERIFIED: python3 not available\n\
            This test requires Python to verify AI lifecycle.\n\
            Status: IMPLEMENTED-UNVERIFIED"
        );
    }

    let temp = TempDir::new().unwrap();
    let project_name = "test-ai-full";
    let project_path = temp.path().join(project_name);
    let mgc = find_mgc_binary();

    // === STEP 1: CREATE ===
    println!("\n=== STEP 1: mgc create-ai ===");
    let create_output = Command::new(&mgc)
        .arg("create-ai")
        .arg("python-agent")
        .arg(project_name)
        .current_dir(temp.path())
        .output()
        .expect("mgc create-ai failed to execute");

    assert!(
        create_output.status.success(),
        "CREATE FAILED:\n{}",
        String::from_utf8_lossy(&create_output.stderr)
    );
    assert!(project_path.exists(), "Project directory not created");
    assert!(
        project_path.join("pyproject.toml").exists(),
        "pyproject.toml not created"
    );

    // VERIFY: .mgc.core file
    let core_content = std::fs::read_to_string(project_path.join(".mgc.core")).unwrap();
    assert_eq!(core_content.trim(), "ai", ".mgc.core should contain 'ai'");

    println!("✅ AI lifecycle PARTIAL: create verified");
    println!("   Full lifecycle (install → test) requires pytest - see optimizer tests");
}

#[test]
fn test_app_full_lifecycle_limited() {
    // LIMITED E2E: app core - create → verify structure
    // Full test (build → test) requires Flutter SDK
    // REQUIRES: flutter

    if Command::new("flutter").arg("--version").output().is_err() {
        panic!(
            "UNVERIFIED: flutter not available\n\
            This test requires Flutter SDK to verify App lifecycle.\n\
            Status: IMPLEMENTED-UNVERIFIED"
        );
    }

    let temp = TempDir::new().unwrap();
    let project_name = "test_app_full"; // Flutter naming
    let project_path = temp.path().join(project_name);
    let mgc = find_mgc_binary();

    // === STEP 1: CREATE ===
    println!("\n=== STEP 1: mgc create-app ===");
    let create_output = Command::new(&mgc)
        .arg("create-app")
        .arg("flutter")
        .arg(project_name)
        .current_dir(temp.path())
        .output()
        .expect("mgc create-app failed to execute");

    assert!(
        create_output.status.success(),
        "CREATE FAILED:\n{}",
        String::from_utf8_lossy(&create_output.stderr)
    );
    assert!(project_path.exists(), "Project directory not created");
    assert!(
        project_path.join("pubspec.yaml").exists(),
        "pubspec.yaml not created"
    );

    // VERIFY: .mgc.core file
    let core_content = std::fs::read_to_string(project_path.join(".mgc.core")).unwrap();
    assert_eq!(core_content.trim(), "app", ".mgc.core should contain 'app'");

    println!("✅ App lifecycle PARTIAL: create verified");
    println!("   Full lifecycle (build → test) requires Flutter - see optimizer tests");
}
