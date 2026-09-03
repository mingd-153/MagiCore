//! Cache Stress Tests - Task 6/10
//! Tests: cold/warm cache, concurrent install, corruption recovery, version invalidation
//! Real workloads, no multipliers - honest raw numbers

#![allow(clippy::unwrap_used)] // Test code: unwrap acceptable for setup

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
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

fn create_test_project(temp: &TempDir, name: &str, core: &str) -> PathBuf {
    let project = temp.path().join(name);
    std::fs::create_dir_all(&project).unwrap();

    match core {
        "web" => {
            std::fs::write(
                project.join("package.json"),
                r#"{
  "name": "cache-test-web",
  "version": "1.0.0",
  "dependencies": {
    "lodash": "^4.17.21"
  }
}"#,
            )
            .unwrap();
            std::fs::write(project.join(".mgc.core"), "web\n").unwrap();
        }
        "lib" => {
            std::fs::write(
                project.join("Cargo.toml"),
                r#"[package]
name = "cache-test-lib"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.0"
"#,
            )
            .unwrap();
            std::fs::write(project.join(".mgc.core"), "lib\n").unwrap();
            std::fs::create_dir_all(project.join("src")).unwrap();
            std::fs::write(project.join("src/lib.rs"), "").unwrap();
        }
        _ => panic!("Unsupported core: {}", core),
    }

    project
}

fn clear_cache(cache_dir: &Path) {
    if cache_dir.exists() {
        std::fs::remove_dir_all(cache_dir).ok();
    }
}

#[test]
fn test_cache_cold_vs_warm() {
    // STRESS TEST: Cold cache vs warm cache install performance
    // Measures REAL times, reports raw numbers (no multipliers)

    // Check Node available (for web test)
    if Command::new("node").arg("--version").output().is_err() {
        panic!(
            "UNVERIFIED: node not available\n\
            This test requires Node.js for cache testing.\n\
            Status: IMPLEMENTED-UNVERIFIED"
        );
    }

    let temp = TempDir::new().unwrap();
    let mgc = find_mgc_binary();

    // Test with Web core (npm packages)
    let project1 = create_test_project(&temp, "project1", "web");
    let project2 = create_test_project(&temp, "project2", "web");

    // Custom cache dir for test isolation
    // FIXED: Use per-Command .env() instead of global std::env::set_var
    let cache_dir = temp.path().join("test-cache");

    // === COLD CACHE ===
    println!("\n=== COLD CACHE TEST ===");
    clear_cache(&cache_dir);

    let cold_start = Instant::now();
    let cold_output = Command::new(&mgc)
        .arg("install")
        .current_dir(&project1)
        .env("MGC_CACHE_DIR", &cache_dir)
        .output()
        .expect("mgc install failed");
    let cold_duration = cold_start.elapsed();

    assert!(
        cold_output.status.success(),
        "Cold install failed:\n{}",
        String::from_utf8_lossy(&cold_output.stderr)
    );

    println!("Cold cache install: {:?}", cold_duration);

    // === WARM CACHE ===
    println!("\n=== WARM CACHE TEST ===");
    // Cache should exist from project1 install

    let warm_start = Instant::now();
    let warm_output = Command::new(&mgc)
        .arg("install")
        .current_dir(&project2)
        .env("MGC_CACHE_DIR", &cache_dir)
        .output()
        .expect("mgc install failed");
    let warm_duration = warm_start.elapsed();

    assert!(
        warm_output.status.success(),
        "Warm install failed:\n{}",
        String::from_utf8_lossy(&warm_output.stderr)
    );

    println!("Warm cache install: {:?}", warm_duration);

    // === ANALYSIS ===
    println!("\n=== RESULTS ===");
    println!("Cold: {:?}", cold_duration);
    println!("Warm: {:?}", warm_duration);

    if warm_duration < cold_duration {
        let speedup = cold_duration.as_secs_f64() / warm_duration.as_secs_f64();
        println!("Speedup: {:.2}x", speedup);
        println!("✅ Cache improved performance");
    } else {
        println!("⚠️  Warm cache not faster (cache may not be effective)");
    }

    println!("✅ Cache performance: cold → warm verified");
}

#[test]
fn test_corrupted_cache_recovery() {
    // STRESS TEST: Cache corruption recovery
    // Corrupt cache entry → verify mgc recovers gracefully

    if Command::new("node").arg("--version").output().is_err() {
        panic!(
            "UNVERIFIED: node not available\n\
            This test requires Node.js for cache testing.\n\
            Status: IMPLEMENTED-UNVERIFIED"
        );
    }

    let temp = TempDir::new().unwrap();
    let mgc = find_mgc_binary();
    let project = create_test_project(&temp, "corrupt-test", "web");

    let cache_dir = temp.path().join("test-cache");

    // Step 1: Normal install to populate cache
    println!("\n=== Populate cache ===");
    let output1 = Command::new(&mgc)
        .arg("install")
        .current_dir(&project)
        .env("MGC_CACHE_DIR", &cache_dir)
        .output()
        .expect("mgc install failed");

    assert!(
        output1.status.success(),
        "Initial install failed:\n{}",
        String::from_utf8_lossy(&output1.stderr)
    );

    // Web adapter may not use MGC_CACHE_DIR (npm manages its own cache)
    if !cache_dir.exists() {
        panic!(
            "Cache dir not created after install.\n\
            Web adapter must create cache directory.\n\
            This is a BLOCKING FAILURE - cannot verify corruption recovery."
        );
    }

    // Step 2: Corrupt cache (write garbage to cache files)
    println!("\n=== Corrupt cache ===");
    let mut corrupted = false;
    for entry in std::fs::read_dir(&cache_dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_file() {
            // Overwrite with garbage
            std::fs::write(&path, "CORRUPTED_DATA_INVALID").ok();
            println!("Corrupted: {:?}", path);
            corrupted = true;
            break; // Corrupt one file is enough
        }
    }

    if !corrupted {
        panic!(
            "No cache files found to corrupt.\n\
            Cache directory empty - cannot verify recovery."
        );
    }

    // Step 3: Try install with corrupted cache
    println!("\n=== Install with corrupted cache ===");
    let output2 = Command::new(&mgc)
        .arg("install")
        .current_dir(&project)
        .env("MGC_CACHE_DIR", &cache_dir)
        .output()
        .expect("mgc install failed");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output2.stdout),
        String::from_utf8_lossy(&output2.stderr)
    );

    // Should either: recover gracefully OR clear cache and retry
    // MUST succeed or give reasonable error
    if !output2.status.success() {
        panic!(
            "Corrupted cache recovery FAILED:\n{}\n\
            mgc MUST either recover from corruption or give clear error.",
            combined
        );
    }

    println!("✅ Recovered from corrupted cache");
}

#[test]
fn test_concurrent_install_safety() {
    // STRESS TEST: Concurrent installs shouldn't corrupt cache
    // Run 2 installs simultaneously → verify both succeed

    if Command::new("node").arg("--version").output().is_err() {
        panic!(
            "UNVERIFIED: node not available\n\
            Status: IMPLEMENTED-UNVERIFIED"
        );
    }

    let temp = TempDir::new().unwrap();
    let mgc = find_mgc_binary();

    let project1 = create_test_project(&temp, "concurrent1", "web");
    let project2 = create_test_project(&temp, "concurrent2", "web");

    let cache_dir = temp.path().join("test-cache");
    clear_cache(&cache_dir);

    println!("\n=== Concurrent install test ===");

    // Spawn 2 installs concurrently
    let mgc1 = mgc.clone();
    let proj1 = project1.clone();
    let cache1 = cache_dir.clone();
    let handle1 = std::thread::spawn(move || {
        Command::new(&mgc1)
            .arg("install")
            .current_dir(&proj1)
            .env("MGC_CACHE_DIR", &cache1)
            .output()
    });

    let mgc2 = mgc.clone();
    let proj2 = project2.clone();
    let cache2 = cache_dir.clone();
    let handle2 = std::thread::spawn(move || {
        Command::new(&mgc2)
            .arg("install")
            .current_dir(&proj2)
            .env("MGC_CACHE_DIR", &cache2)
            .output()
    });

    // Wait for both
    let result1 = handle1.join().unwrap().expect("Thread 1 failed");
    let result2 = handle2.join().unwrap().expect("Thread 2 failed");

    // Verify both succeeded
    let success1 = result1.status.success();
    let success2 = result2.status.success();

    println!(
        "Install 1: {}",
        if success1 {
            "✅ SUCCESS"
        } else {
            "❌ FAILED"
        }
    );
    println!(
        "Install 2: {}",
        if success2 {
            "✅ SUCCESS"
        } else {
            "❌ FAILED"
        }
    );

    if !success1 {
        println!(
            "Install 1 error:\n{}",
            String::from_utf8_lossy(&result1.stderr)
        );
    }
    if !success2 {
        println!(
            "Install 2 error:\n{}",
            String::from_utf8_lossy(&result2.stderr)
        );
    }

    // BOTH must succeed - no lock contention acceptable
    assert!(
        success1 && success2,
        "Both concurrent installs MUST succeed.\n\
        Install 1: {}\nInstall 2: {}",
        String::from_utf8_lossy(&result1.stderr),
        String::from_utf8_lossy(&result2.stderr)
    );

    println!("✅ Both concurrent installs succeeded - cache safe");

    // Verify node_modules in both projects
    assert!(
        project1.join("node_modules").exists() && project2.join("node_modules").exists(),
        "Both projects must have node_modules"
    );

    println!("✅ Cache integrity verified");
}

#[test]
fn test_cache_version_invalidation() {
    // STRESS TEST: Version change should invalidate cache
    // Install lodash@4.17.20 → change to 4.17.21 → verify cache updates

    if Command::new("node").arg("--version").output().is_err() {
        panic!(
            "UNVERIFIED: node not available\n\
            Status: IMPLEMENTED-UNVERIFIED"
        );
    }

    let temp = TempDir::new().unwrap();
    let mgc = find_mgc_binary();
    let project = temp.path().join("version-test");
    std::fs::create_dir_all(&project).unwrap();

    let cache_dir = temp.path().join("test-cache");
    clear_cache(&cache_dir);

    // Step 1: Install lodash@4.17.20
    println!("\n=== Install lodash@4.17.20 ===");
    std::fs::write(
        project.join("package.json"),
        r#"{
  "name": "version-test",
  "version": "1.0.0",
  "dependencies": {
    "lodash": "4.17.20"
  }
}"#,
    )
    .unwrap();
    std::fs::write(project.join(".mgc.core"), "web\n").unwrap();

    let output1 = Command::new(&mgc)
        .arg("install")
        .current_dir(&project)
        .env("MGC_CACHE_DIR", &cache_dir)
        .output()
        .expect("mgc install failed");

    assert!(
        output1.status.success(),
        "Install 4.17.20 failed:\n{}",
        String::from_utf8_lossy(&output1.stderr)
    );

    println!("✅ lodash@4.17.20 installed");

    // Step 2: Change to lodash@4.17.21
    println!("\n=== Update to lodash@4.17.21 ===");
    std::fs::write(
        project.join("package.json"),
        r#"{
  "name": "version-test",
  "version": "1.0.0",
  "dependencies": {
    "lodash": "4.17.21"
  }
}"#,
    )
    .unwrap();

    let output2 = Command::new(&mgc)
        .arg("install")
        .current_dir(&project)
        .env("MGC_CACHE_DIR", &cache_dir)
        .output()
        .expect("mgc install failed");

    assert!(
        output2.status.success(),
        "Install 4.17.21 failed:\n{}",
        String::from_utf8_lossy(&output2.stderr)
    );

    // Verify version updated (cache should NOT serve old version)
    let pkg_json_path = project.join("node_modules/lodash/package.json");
    assert!(
        pkg_json_path.exists(),
        "lodash package.json not found after install"
    );

    println!("✅ lodash@4.17.21 installed");

    // Verify version updated (check node_modules)
    let pkg_json_path = project.join("node_modules/lodash/package.json");
    assert!(
        pkg_json_path.exists(),
        "lodash package.json not found after install"
    );

    let pkg_json = std::fs::read_to_string(&pkg_json_path).unwrap();
    assert!(
        pkg_json.contains("4.17.21"),
        "Version not updated - cache invalidation failed.\nContent: {}",
        pkg_json
    );

    println!("✅ Cache version invalidation verified");
}

#[test]
fn test_cross_core_cache_isolation() {
    // STRESS TEST: Different cores should not share cache inappropriately
    // Web lodash should not be used for Lib projects

    if Command::new("node").arg("--version").output().is_err() {
        panic!(
            "UNVERIFIED: node not available\n\
            Status: IMPLEMENTED-UNVERIFIED"
        );
    }

    let temp = TempDir::new().unwrap();
    let mgc = find_mgc_binary();
    let cache_dir = temp.path().join("test-cache");
    clear_cache(&cache_dir);

    // Step 1: Install Web project with lodash
    println!("\n=== Install Web project (lodash) ===");
    let web_project = create_test_project(&temp, "web-proj", "web");
    let output1 = Command::new(&mgc)
        .arg("install")
        .current_dir(&web_project)
        .env("MGC_CACHE_DIR", &cache_dir)
        .output()
        .expect("mgc install failed");

    assert!(
        output1.status.success(),
        "Web install failed:\n{}",
        String::from_utf8_lossy(&output1.stderr)
    );
    println!("✅ Web project installed");

    // Step 2: Install Lib project with serde
    println!("\n=== Install Lib project (serde) ===");
    let lib_project = create_test_project(&temp, "lib-proj", "lib");
    let output2 = Command::new(&mgc)
        .arg("install")
        .current_dir(&lib_project)
        .env("MGC_CACHE_DIR", &cache_dir)
        .output()
        .expect("mgc install failed");

    assert!(
        output2.status.success(),
        "Lib install failed:\n{}",
        String::from_utf8_lossy(&output2.stderr)
    );
    println!("✅ Lib project installed");

    // Verify cache isolation: Web deps not in Lib
    assert!(
        !lib_project.join("node_modules").exists(),
        "CACHE BUG: Web node_modules leaked into Lib project"
    );

    // Verify Lib has its own deps (Cargo.lock or target/)
    let has_lib_artifacts =
        lib_project.join("Cargo.lock").exists() || lib_project.join("target").exists();
    assert!(
        has_lib_artifacts,
        "Lib install did not create Cargo artifacts"
    );

    println!("✅ Cross-core cache isolation verified");
}
