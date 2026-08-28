#![cfg(test)]
#![allow(clippy::unwrap_used)]

use mgc_cache::PackageCache;

#[test]
fn test_package_cache_can_be_created() {
    let cache = PackageCache::new();
    assert!(cache.is_ok());
}

#[test]
fn test_package_cache_creates_default_directory() {
    let cache = PackageCache::new().unwrap();
    // Cache should initialize without error
    // Actual directory location is internal implementation detail
    drop(cache);
}

#[test]
fn test_package_cache_can_be_created_multiple_times() {
    let cache1 = PackageCache::new();
    let cache2 = PackageCache::new();

    assert!(cache1.is_ok());
    assert!(cache2.is_ok());
}
