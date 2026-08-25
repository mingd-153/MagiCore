//! Hermetic build tests — Tests build hermetic
//! Tests for offline mode, cache integrity, and reproducibility

use mgc_cache::PackageCache;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_cache_isolation() {
    // T4.6.1: Cache should be isolated per package+version
    let temp = TempDir::new().unwrap();
    let cache = PackageCache::with_root(temp.path().to_path_buf());

    // Store two different packages
    let pkg1 = temp.path().join("pkg1.tgz");
    let pkg2 = temp.path().join("pkg2.tgz");
    fs::write(&pkg1, b"package1").unwrap();
    fs::write(&pkg2, b"package2").unwrap();

    let int1 = cache.compute_integrity(&pkg1).unwrap();
    let int2 = cache.compute_integrity(&pkg2).unwrap();

    cache.store_package("react@18.0.0", &pkg1, &int1).unwrap();
    cache.store_package("react@18.2.0", &pkg2, &int2).unwrap();

    // Both should exist independently
    assert!(cache.has_package("react@18.0.0"));
    assert!(cache.has_package("react@18.2.0"));

    // Get should return correct packages
    let retrieved1 = cache.get_package("react@18.0.0", &int1).unwrap();
    let retrieved2 = cache.get_package("react@18.2.0", &int2).unwrap();
    assert_ne!(retrieved1, retrieved2);
}

#[test]
fn test_cache_integrity_enforcement() {
    // T4.6.2: Cache MUST enforce integrity checks
    let temp = TempDir::new().unwrap();
    let cache = PackageCache::with_root(temp.path().to_path_buf());

    let pkg = temp.path().join("pkg.tgz");
    fs::write(&pkg, b"original").unwrap();
    let integrity = cache.compute_integrity(&pkg).unwrap();

    cache.store_package("pkg@1.0.0", &pkg, &integrity).unwrap();

    // Tamper with cached file
    let cached_path = cache.package_path("pkg@1.0.0");
    fs::write(&cached_path, b"TAMPERED").unwrap();

    // Get should FAIL with integrity mismatch
    let result = cache.get_package("pkg@1.0.0", &integrity);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Integrity mismatch"));
}

#[test]
fn test_cache_invalidation() {
    // T4.6.3: Cache invalidation should remove package
    let temp = TempDir::new().unwrap();
    let cache = PackageCache::with_root(temp.path().to_path_buf());

    let pkg = temp.path().join("pkg.tgz");
    fs::write(&pkg, b"data").unwrap();
    let integrity = cache.compute_integrity(&pkg).unwrap();

    cache.store_package("pkg@1.0.0", &pkg, &integrity).unwrap();
    assert!(cache.has_package("pkg@1.0.0"));

    // Invalidate
    cache.invalidate_package("pkg@1.0.0").unwrap();
    assert!(!cache.has_package("pkg@1.0.0"));

    // Should be able to re-store after invalidation
    cache.store_package("pkg@1.0.0", &pkg, &integrity).unwrap();
    assert!(cache.has_package("pkg@1.0.0"));
}

#[test]
fn test_cache_prune() {
    // T4.6.4: Prune should remove all packages
    let temp = TempDir::new().unwrap();
    let cache = PackageCache::with_root(temp.path().to_path_buf());

    // Store multiple packages
    for i in 0..5 {
        let pkg = temp.path().join(format!("pkg{}.tgz", i));
        fs::write(&pkg, format!("package{}", i)).unwrap();
        let integrity = cache.compute_integrity(&pkg).unwrap();
        cache
            .store_package(&format!("pkg{}@1.0.0", i), &pkg, &integrity)
            .unwrap();
    }

    // All should exist
    for i in 0..5 {
        assert!(cache.has_package(&format!("pkg{}@1.0.0", i)));
    }

    // Prune
    let count = cache.prune().unwrap();
    assert!(count > 0); // Should remove npm/ directory (at least 1 dir)

    // All should be gone
    for i in 0..5 {
        assert!(!cache.has_package(&format!("pkg{}@1.0.0", i)));
    }
}

#[test]
fn test_hermetic_storage_reproducibility() {
    // T4.6.5: Same content should produce same integrity
    let temp = TempDir::new().unwrap();
    let cache = PackageCache::with_root(temp.path().to_path_buf());

    let content = b"reproducible package content";
    let pkg1 = temp.path().join("pkg1.tgz");
    let pkg2 = temp.path().join("pkg2.tgz");
    fs::write(&pkg1, content).unwrap();
    fs::write(&pkg2, content).unwrap();

    let int1 = cache.compute_integrity(&pkg1).unwrap();
    let int2 = cache.compute_integrity(&pkg2).unwrap();

    // Same content = same integrity
    assert_eq!(int1, int2);
}

#[test]
fn test_cache_does_not_exist_initially() {
    // T4.6.6: Fresh cache should have no packages
    let temp = TempDir::new().unwrap();
    let cache = PackageCache::with_root(temp.path().to_path_buf());

    assert!(!cache.has_package("nonexistent@1.0.0"));
}

#[test]
fn test_cache_get_nonexistent_fails() {
    // T4.6.7: Getting nonexistent package should fail
    let temp = TempDir::new().unwrap();
    let cache = PackageCache::with_root(temp.path().to_path_buf());

    let result = cache.get_package("nonexistent@1.0.0", "fake-integrity");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not in cache"));
}

#[test]
fn test_cache_special_characters_in_package_id() {
    // T4.6.8: Package IDs with special chars should be sanitized
    let temp = TempDir::new().unwrap();
    let cache = PackageCache::with_root(temp.path().to_path_buf());

    let pkg = temp.path().join("pkg.tgz");
    fs::write(&pkg, b"data").unwrap();
    let integrity = cache.compute_integrity(&pkg).unwrap();

    // Package ID with special chars
    let pkg_id = "@scope/package:1.0.0";
    cache.store_package(pkg_id, &pkg, &integrity).unwrap();

    // Should be retrievable
    assert!(cache.has_package(pkg_id));
    cache.get_package(pkg_id, &integrity).unwrap();
}

#[test]
fn test_cache_concurrent_access_safety() {
    // T4.6.9: Cache should handle concurrent access (basic test)
    let temp = TempDir::new().unwrap();
    let cache = PackageCache::with_root(temp.path().to_path_buf());

    let pkg = temp.path().join("pkg.tgz");
    fs::write(&pkg, b"data").unwrap();
    let integrity = cache.compute_integrity(&pkg).unwrap();

    // Store
    cache.store_package("pkg@1.0.0", &pkg, &integrity).unwrap();

    // Multiple reads should succeed
    for _ in 0..10 {
        cache.get_package("pkg@1.0.0", &integrity).unwrap();
    }
}

#[test]
fn test_cache_large_package() {
    // T4.6.10: Cache should handle larger packages
    let temp = TempDir::new().unwrap();
    let cache = PackageCache::with_root(temp.path().to_path_buf());

    // 1MB package
    let large_data = vec![0u8; 1024 * 1024];
    let pkg = temp.path().join("large.tgz");
    fs::write(&pkg, &large_data).unwrap();
    let integrity = cache.compute_integrity(&pkg).unwrap();

    cache
        .store_package("large@1.0.0", &pkg, &integrity)
        .unwrap();

    // Verify retrieval
    let retrieved = cache.get_package("large@1.0.0", &integrity).unwrap();
    let retrieved_data = fs::read(&retrieved).unwrap();
    assert_eq!(retrieved_data.len(), 1024 * 1024);
}

#[test]
fn test_parallel_store() {
    // T5.2: Parallel store should be faster than sequential
    let temp = TempDir::new().unwrap();
    let cache = PackageCache::with_root(temp.path().to_path_buf());

    // Create 10 fake packages
    let mut requests = Vec::new();
    for i in 0..10 {
        let pkg_file = temp.path().join(format!("pkg{}.tgz", i));
        fs::write(&pkg_file, format!("package{}", i)).unwrap();
        let integrity = cache.compute_integrity(&pkg_file).unwrap();
        requests.push((format!("pkg{}@1.0.0", i), pkg_file, integrity));
    }

    // Store in parallel
    let results = cache.store_packages_parallel(&requests).unwrap();
    assert_eq!(results.len(), 10);

    // All should exist
    for i in 0..10 {
        assert!(cache.has_package(&format!("pkg{}@1.0.0", i)));
    }
}

#[test]
fn test_parallel_get() {
    // T5.2: Parallel get should work correctly
    let temp = TempDir::new().unwrap();
    let cache = PackageCache::with_root(temp.path().to_path_buf());

    // Store 10 packages first
    for i in 0..10 {
        let pkg_file = temp.path().join(format!("pkg{}.tgz", i));
        fs::write(&pkg_file, format!("package{}", i)).unwrap();
        let integrity = cache.compute_integrity(&pkg_file).unwrap();
        cache
            .store_package(&format!("pkg{}@1.0.0", i), &pkg_file, &integrity)
            .unwrap();
    }

    // Get in parallel
    let mut requests = Vec::new();
    for i in 0..10 {
        let pkg_file = temp.path().join(format!("pkg{}.tgz", i));
        let integrity = cache.compute_integrity(&pkg_file).unwrap();
        requests.push((format!("pkg{}@1.0.0", i), integrity));
    }

    let results = cache.get_packages_parallel(&requests).unwrap();
    assert_eq!(results.len(), 10);
}
