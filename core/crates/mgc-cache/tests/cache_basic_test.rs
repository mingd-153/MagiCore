#![cfg(test)]
#![allow(clippy::unwrap_used)]

use mgc_cache::PackageCache;
use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn setup_test_cache() -> (TempDir, PackageCache) {
    let tmp = tempfile::tempdir().unwrap();
    let cache = PackageCache::with_root(tmp.path().to_path_buf());
    (tmp, cache)
}

fn create_test_package(dir: &std::path::Path, content: &[u8]) -> std::path::PathBuf {
    let pkg_path = dir.join("test.tgz");
    let mut file = fs::File::create(&pkg_path).unwrap();
    file.write_all(content).unwrap();
    pkg_path
}

#[test]
fn test_cache_store_and_retrieve() {
    let (_tmp, cache) = setup_test_cache();

    // Create test package
    let test_dir = tempfile::tempdir().unwrap();
    let content = b"test package content";
    let pkg_file = create_test_package(test_dir.path(), content);

    // Compute integrity
    let integrity = cache.compute_integrity(&pkg_file).unwrap();

    // Store package
    let stored = cache.store_package("lodash@4.17.21", &pkg_file, &integrity);
    assert!(stored.is_ok(), "Failed to store package: {:?}", stored);

    // Retrieve package
    let retrieved = cache.get_package("lodash@4.17.21", &integrity);
    assert!(
        retrieved.is_ok(),
        "Failed to retrieve package: {:?}",
        retrieved
    );

    // Verify content
    let retrieved_path = retrieved.unwrap();
    let retrieved_content = fs::read(&retrieved_path).unwrap();
    assert_eq!(retrieved_content, content);
}

#[test]
fn test_cache_integrity_mismatch_fails() {
    let (_tmp, cache) = setup_test_cache();

    let test_dir = tempfile::tempdir().unwrap();
    let pkg_file = create_test_package(test_dir.path(), b"original content");
    let integrity = cache.compute_integrity(&pkg_file).unwrap();

    // Store package
    cache
        .store_package("axios@1.0.0", &pkg_file, &integrity)
        .unwrap();

    // Try retrieve with wrong integrity
    let wrong_integrity = "blake3:0000000000000000";
    let result = cache.get_package("axios@1.0.0", wrong_integrity);

    assert!(result.is_err(), "Should fail with wrong integrity");
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Integrity mismatch"));
}

#[test]
fn test_cache_store_with_wrong_integrity_fails() {
    let (_tmp, cache) = setup_test_cache();

    let test_dir = tempfile::tempdir().unwrap();
    let pkg_file = create_test_package(test_dir.path(), b"test content");

    // Try store with wrong integrity
    let wrong_integrity = "blake3:wronghash";
    let result = cache.store_package("express@4.0.0", &pkg_file, wrong_integrity);

    assert!(result.is_err(), "Should fail storing with wrong integrity");
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Integrity mismatch"));
}

#[test]
fn test_cache_has_package() {
    let (_tmp, cache) = setup_test_cache();

    assert!(!cache.has_package("nonexistent@1.0.0"));

    // Store package
    let test_dir = tempfile::tempdir().unwrap();
    let pkg_file = create_test_package(test_dir.path(), b"content");
    let integrity = cache.compute_integrity(&pkg_file).unwrap();
    cache
        .store_package("react@18.0.0", &pkg_file, &integrity)
        .unwrap();

    assert!(cache.has_package("react@18.0.0"));
}

#[test]
fn test_cache_invalidate_package() {
    let (_tmp, cache) = setup_test_cache();

    // Store package
    let test_dir = tempfile::tempdir().unwrap();
    let pkg_file = create_test_package(test_dir.path(), b"content");
    let integrity = cache.compute_integrity(&pkg_file).unwrap();
    cache
        .store_package("vue@3.0.0", &pkg_file, &integrity)
        .unwrap();

    assert!(cache.has_package("vue@3.0.0"));

    // Invalidate
    cache.invalidate_package("vue@3.0.0").unwrap();

    assert!(!cache.has_package("vue@3.0.0"));
}

#[test]
fn test_cache_prune() {
    let (_tmp, cache) = setup_test_cache();

    // Store multiple packages
    let test_dir = tempfile::tempdir().unwrap();
    let pkg_file = create_test_package(test_dir.path(), b"content");
    let integrity = cache.compute_integrity(&pkg_file).unwrap();

    cache
        .store_package("pkg1@1.0.0", &pkg_file, &integrity)
        .unwrap();
    cache
        .store_package("pkg2@1.0.0", &pkg_file, &integrity)
        .unwrap();
    cache
        .store_package("pkg3@1.0.0", &pkg_file, &integrity)
        .unwrap();

    // Prune (current impl removes all packages in npm directory)
    let pruned = cache.prune().unwrap();
    assert!(pruned > 0, "Should prune at least some packages");

    // Verify packages gone
    assert!(!cache.has_package("pkg1@1.0.0"));
    assert!(!cache.has_package("pkg2@1.0.0"));
    assert!(!cache.has_package("pkg3@1.0.0"));
}
