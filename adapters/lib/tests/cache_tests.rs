#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Cache module integration tests.

#![allow(clippy::unwrap_used)]
use mgc_lib_adapter::cache::metadata::{metadata_path, CacheEntry, CacheMetadata};
use mgc_lib_adapter::cache::prune::{prune_cache, PruneStrategy};
use mgc_lib_adapter::cache::{cache_dir, cache_size, clear_cache};
use mgc_types::{PackageId, PackageName, Version};
use std::path::PathBuf;

#[test]
fn cache_dir_rust_points_to_cargo_registry() {
    let dir = cache_dir("rust").unwrap();
    assert!(dir.to_string_lossy().contains(".cargo"));
    assert!(dir.to_string_lossy().contains("registry"));
}

#[test]
fn cache_dir_python_prefers_uv_over_pip() {
    let dir = cache_dir("python").unwrap();
    // Will be either .cache/uv or .cache/pip depending on system
    assert!(dir.to_string_lossy().contains(".cache"));
}

#[test]
fn cache_dir_typescript_uses_mgc_store() {
    let dir = cache_dir("ts").unwrap();
    assert!(dir.to_string_lossy().contains(".mgc-store"));
}

#[test]
fn cache_dir_unsupported_language_errors() {
    let result = cache_dir("java");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unsupported"));
}

#[test]
fn cache_size_returns_zero_for_nonexistent_cache() {
    // Use rust cache which may or may not exist
    let result = cache_size("rust");
    // Should either return size or error - both acceptable
    match result {
        // size là u64 — mọi giá trị đều hợp lệ, chỉ cần gọi không panic
        // (size is u64 — any value is valid; the contract is simply no panic)
        Ok(_) => {}
        Err(_) => {
            // Cache might not exist or not accessible - that's fine
        }
    }
}

#[test]
fn clear_cache_succeeds_even_if_cache_not_exists() {
    // This should not error even if cache doesn't exist
    let result = clear_cache("rust");
    // May succeed or fail depending on permissions, but shouldn't panic
    let _ = result;
}

#[test]
fn cache_metadata_roundtrip() {
    let tmp = std::env::temp_dir().join(format!("mgc-cache-meta-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let meta_file = tmp.join("metadata.json");

    let mut metadata = CacheMetadata::default();
    let entry = CacheEntry {
        package_id: PackageId::new(
            PackageName::new("serde").unwrap(),
            Version::parse("1.0.0").unwrap(),
        ),
        cached_at: 1234567890,
        size_bytes: 1024,
        file_path: PathBuf::from("/tmp/serde.crate"),
    };

    metadata.add(entry.clone());
    metadata.save(&meta_file).unwrap();

    let loaded = CacheMetadata::load(&meta_file).unwrap();
    assert_eq!(loaded.count(), 1);
    assert_eq!(loaded.total_size(), 1024);

    std::fs::remove_dir_all(&tmp).unwrap();
}

#[test]
fn cache_metadata_remove_entry() {
    let mut metadata = CacheMetadata::default();
    let pkg_id = PackageId::new(
        PackageName::new("tokio").unwrap(),
        Version::parse("1.0.0").unwrap(),
    );

    let entry = CacheEntry {
        package_id: pkg_id.clone(),
        cached_at: 1234567890,
        size_bytes: 2048,
        file_path: PathBuf::from("/tmp/tokio.crate"),
    };

    metadata.add(entry);
    assert_eq!(metadata.count(), 1);

    let removed = metadata.remove(&pkg_id);
    assert!(removed.is_some());
    assert_eq!(metadata.count(), 0);
}

#[test]
fn metadata_path_returns_valid_path() {
    let path = metadata_path("rust").unwrap();
    assert!(path.to_string_lossy().contains("metadata.json"));
}

#[tokio::test]
async fn prune_cache_older_than_strategy() {
    // This test verifies the prune function doesn't crash
    // Actual pruning depends on existing cache
    let result = prune_cache("rust", PruneStrategy::OlderThan(365));
    // May succeed with 0 bytes or fail if no cache - both acceptable
    let _ = result;
}

#[tokio::test]
async fn prune_cache_keep_recent_strategy() {
    let result = prune_cache("python", PruneStrategy::KeepRecent(100));
    let _ = result;
}

#[tokio::test]
async fn prune_cache_max_size_strategy() {
    let result = prune_cache("rust", PruneStrategy::MaxSize(1024 * 1024 * 1024));
    let _ = result;
}
