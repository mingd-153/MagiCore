#![cfg_attr(test, allow(clippy::unwrap_used))]
//! Cache module tests for app adapter.

use mgc_app_adapter::cache::{cache_dir, cache_size, clear_cache};
use mgc_app_adapter::AppLanguage;

#[test]
fn flutter_cache_dir_points_to_pub_cache() {
    let dir = cache_dir(AppLanguage::Flutter).unwrap();
    assert!(dir.to_string_lossy().contains(".pub-cache"));
}

#[test]
fn kotlin_cache_dir_points_to_gradle() {
    let dir = cache_dir(AppLanguage::Kotlin).unwrap();
    assert!(dir.to_string_lossy().contains(".gradle"));
}

#[test]
fn swift_cache_dir_platform_specific() {
    let dir = cache_dir(AppLanguage::Swift).unwrap();
    assert!(dir.to_string_lossy().contains("swiftpm"));
}

#[test]
fn react_native_cache_uses_npm() {
    let dir = cache_dir(AppLanguage::ReactNative).unwrap();
    assert!(dir.to_string_lossy().contains(".npm"));
}

#[test]
fn objc_cache_uses_cocoapods() {
    let dir = cache_dir(AppLanguage::ObjC).unwrap();
    assert!(
        dir.to_string_lossy().contains("CocoaPods") || dir.to_string_lossy().contains("cocoapods")
    );
}

#[test]
fn cache_size_handles_nonexistent() {
    let result = cache_size(AppLanguage::Flutter);
    // size is u64 — any value valid; contract: no panic
    // Gọi được là đạt — Err chấp nhận khi cache không truy cập được
    // (must simply not panic; Err acceptable when cache inaccessible)
    drop(result);
}

#[test]
fn clear_cache_doesnt_panic() {
    let _ = clear_cache(AppLanguage::Multi);
}
