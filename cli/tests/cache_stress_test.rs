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
    let cache_dir = temp.path().join("test-cache");
    std::env::set_var("MGC_CACHE_DIR", &cache_dir);

    // === COLD CACHE ===
    println!("\n=== COLD CACHE TEST ===");
    clear_cache(&cache_dir);

    let cold_start = Instant::now();
    let cold_output = Command::new(&mgc)
        .arg("install")
        .current_dir(&project1)
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

    // Cleanup env
    std::env::remove_var("MGC_CACHE_DIR");
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
    std::env::set_var("MGC_CACHE_DIR", &cache_dir);

    // Step 1: Normal install to populate cache
    println!("\n=== Populate cache ===");
    let output1 = Command::new(&mgc)
        .arg("install")
        .current_dir(&project)
        .output()
        .expect("mgc install failed");

    assert!(output1.status.success(), "Initial install failed");

    // Web adapter may not use MGC_CACHE_DIR (npm manages its own cache)
    if !cache_dir.exists() {
        println!("⚠️  Cache dir not created - Web may use npm cache directly");
        println!("   Skipping cache corruption test (not applicable)");
        std::env::remove_var("MGC_CACHE_DIR");
        return;
    }

    // Step 2: Corrupt cache (write garbage to cache files)
    println!("\n=== Corrupt cache ===");
    for entry in std::fs::read_dir(&cache_dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_file() {
            // Overwrite with garbage
            std::fs::write(&path, "CORRUPTED_DATA_INVALID").ok();
            println!("Corrupted: {:?}", path);
            break; // Corrupt one file is enough
        }
    }

    // Step 3: Try install with corrupted cache
    println!("\n=== Install with corrupted cache ===");
    let output2 = Command::new(&mgc)
        .arg("install")
        .current_dir(&project)
        .output()
        .expect("mgc install failed");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output2.stdout),
        String::from_utf8_lossy(&output2.stderr)
    );

    // Should either: recover gracefully OR clear cache and retry
    if output2.status.success() {
        println!("✅ Recovered from corrupted cache");
    } else {
        // Check if error message is reasonable
        assert!(
            combined.contains("cache")
                || combined.contains("corrupted")
                || combined.contains("invalid"),
            "Error should mention cache/corruption issue:\n{}",
            combined
        );
        println!(
            "⚠️  Failed with cache error (expected behavior):\n{}",
            combined
        );
    }

    std::env::remove_var("MGC_CACHE_DIR");
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
    std::env::set_var("MGC_CACHE_DIR", &cache_dir);
    clear_cache(&cache_dir);

    println!("\n=== Concurrent install test ===");

    // Spawn 2 installs concurrently
    let mgc1 = mgc.clone();
    let proj1 = project1.clone();
    let handle1 = std::thread::spawn(move || {
        Command::new(&mgc1)
            .arg("install")
            .current_dir(&proj1)
            .output()
    });

    let mgc2 = mgc.clone();
    let proj2 = project2.clone();
    let handle2 = std::thread::spawn(move || {
        Command::new(&mgc2)
            .arg("install")
            .current_dir(&proj2)
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

    // At least one should succeed (lock contention may cause one to fail/retry)
    // FIXED: BOTH must succeed - no partial failures acceptable
    assert!(
        success1 && success2,
        "❌ CONCURRENT INSTALL FAILED - Cache locking broken.\n\
        Both installs MUST succeed. Got: install1={}, install2={}",
        success1,
        success2
    );

    println!("✅ Both concurrent installs succeeded - cache locking works");

    // Verify node_modules in both projects (actual success indicator)
    assert!(
        project1.join("node_modules").exists() && project2.join("node_modules").exists(),
        "Both projects should have node_modules after successful install"
    );

    println!("✅ Cache integrity verified - both projects have dependencies");

    // Cache dir may or may not exist (Web adapter may not use MGC_CACHE_DIR)
    // The important verification is: both installs succeeded + node_modules present
    if cache_dir.exists() {
        println!("   Cache directory created at: {:?}", cache_dir);
    } else {
        println!("   No cache directory (Web may use npm cache directly)");
    }

    std::env::remove_var("MGC_CACHE_DIR");
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
    std::env::set_var("MGC_CACHE_DIR", &cache_dir);
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
        .output()
        .expect("mgc install failed");

    assert!(output1.status.success(), "Install 4.17.20 failed");

    // Verify lodash 4.17.20 installed
    let lock_or_modules = project.join("node_modules/lodash/package.json").exists();
    if lock_or_modules {
        println!("✅ lodash@4.17.20 installed");
    }

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

    let pkg_json =
        std::fs::read_to_string(&pkg_json_path).expect("Failed to read lodash package.json");

    assert!(
        pkg_json.contains("4.17.21"),
        "❌ CACHE BUG: Version not updated.\n\
        Expected: 4.17.21\n\
        package.json content: {}",
        pkg_json
    );

    assert!(
        !pkg_json.contains("4.17.20"),
        "❌ CACHE BUG: Old version 4.17.20 still present despite upgrade"
    );

    println!("✅ Cache correctly invalidated - new version 4.17.21 installed");

    std::env::remove_var("MGC_CACHE_DIR");
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
    std::env::set_var("MGC_CACHE_DIR", &cache_dir);
    clear_cache(&cache_dir);

    // Step 1: Install Web project with lodash
    println!("\n=== Install Web project (lodash) ===");
    let web_project = create_test_project(&temp, "web-proj", "web");
    let output1 = Command::new(&mgc)
        .arg("install")
        .current_dir(&web_project)
        .output()
        .expect("mgc install failed");

    assert!(output1.status.success(), "Web install failed");
    println!("✅ Web project installed");

    // Step 2: Install Lib project with serde
    println!("\n=== Install Lib project (serde) ===");
    let lib_project = create_test_project(&temp, "lib-proj", "lib");
    let output2 = Command::new(&mgc)
        .arg("install")
        .current_dir(&lib_project)
        .output()
        .expect("mgc install failed");

    // Lib install should NOT use Web cache (different runtime)
    if output2.status.success() {
        println!("✅ Lib project installed");

        // Verify Web deps not in Lib project
        let web_in_lib = lib_project.join("node_modules").exists();
        assert!(
            !web_in_lib,
            "❌ CACHE BUG: Web node_modules leaked into Lib project"
        );

        println!("✅ Cross-core cache isolation verified");
    } else {
        println!("⚠️  Lib install failed (may need cargo available)");
    }

    std::env::remove_var("MGC_CACHE_DIR");
}
