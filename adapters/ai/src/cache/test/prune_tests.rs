use super::*;
use crate::cache::metadata::CacheEntry;
use std::time::Duration;
use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[test]
fn test_prune_older_than() {
    let tmp = tmp();
    let old = tmp.path().join("old.bin");
    std::fs::write(&old, vec![0u8; 100]).unwrap();

    // Simulate old file
    let entry = CacheEntry {
        model_id: "old".into(),
        size_bytes: 100,
        cached_at: std::time::SystemTime::now() - Duration::from_secs(86400 * 10),
        last_accessed: std::time::SystemTime::now(),
        checksum: None,
    };
    super::super::metadata::save_metadata(&old, &entry).unwrap();

    let registry = Registry::Local(tmp.path().to_path_buf());
    let result = prune_cache(&registry, PruneStrategy::OlderThan(5)).unwrap();

    assert_eq!(result.removed_count, 1);
    assert_eq!(result.bytes_freed, 100);
}

#[test]
fn test_remove_model() {
    let tmp = tmp();
    let model = tmp.path().join("bert--base");
    std::fs::create_dir_all(&model).unwrap();
    std::fs::write(model.join("model.bin"), vec![0u8; 200]).unwrap();

    let registry = Registry::Local(tmp.path().to_path_buf());
    let bytes = remove_model(&registry, "bert/base").unwrap();

    assert_eq!(bytes, 200);
    assert!(!model.exists());
}

#[test]
fn test_remove_nonexistent() {
    let tmp = tmp();
    let registry = Registry::Local(tmp.path().to_path_buf());
    let bytes = remove_model(&registry, "missing").unwrap();
    assert_eq!(bytes, 0);
}
