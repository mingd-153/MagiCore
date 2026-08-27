use super::*;
use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[test]
fn test_cache_entry_from_path() {
    let tmp = tmp();
    let model = tmp.path().join("model.bin");
    std::fs::write(&model, vec![0u8; 1024]).unwrap();

    let entry = CacheEntry::from_path("test/model", &model).unwrap();
    assert_eq!(entry.model_id, "test/model");
    assert_eq!(entry.size_bytes, 1024);
}

#[test]
fn test_cache_entry_age() {
    let entry = CacheEntry {
        model_id: "test".into(),
        size_bytes: 0,
        cached_at: SystemTime::now(),
        last_accessed: SystemTime::now(),
        checksum: None,
    };

    assert_eq!(entry.age_days(), 0);
    assert_eq!(entry.days_since_access(), 0);
}

#[test]
fn test_save_load_metadata() {
    let tmp = tmp();
    let model = tmp.path().join("model.bin");
    std::fs::write(&model, b"test").unwrap();

    let entry = CacheEntry::from_path("bert/base", &model).unwrap();
    save_metadata(&model, &entry).unwrap();

    let loaded = load_metadata(&model).unwrap();
    assert_eq!(loaded.model_id, "bert/base");
    assert_eq!(loaded.size_bytes, 4);
}
