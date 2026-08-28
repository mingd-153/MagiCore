//! Tests for search cache
//! Tests cho search cache

use mgc_search::cache::SearchCache;
use mgc_search::types::*;
use tempfile::tempdir;

#[test]
fn test_cache_insert_and_get() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test_cache.db");

    let cache = SearchCache::new_with_path(&db_path).unwrap();

    let results = vec![SearchResult {
        name: "test".to_string(),
        registry: Registry::Npm,
        full_path: "test".to_string(),
        version: "1.0.0".to_string(),
        description: "Test package".to_string(),
        metadata: ResultMetadata::default(),
        score: 90.0,
    }];

    cache.insert("test", &results).unwrap();

    let cached = cache.get("test").unwrap();
    assert!(cached.is_some());
    assert_eq!(cached.unwrap()[0].name, "test");
}

#[test]
fn test_cache_miss() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test_cache.db");

    let cache = SearchCache::new_with_path(&db_path).unwrap();

    let cached = cache.get("nonexistent").unwrap();
    assert!(cached.is_none());
}

#[test]
fn test_track_choice() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test_cache.db");

    let cache = SearchCache::new_with_path(&db_path).unwrap();

    // Track 3 times
    cache.track_choice("gin", Registry::Go).unwrap();
    cache.track_choice("gin", Registry::Go).unwrap();
    cache.track_choice("gin", Registry::Go).unwrap();

    // Should return Go registry (installed 3+ times)
    let choice = cache.get_user_choice("gin").unwrap();
    assert_eq!(choice, Some(Registry::Go));
}

#[test]
fn test_track_choice_multiple_registries() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test_cache.db");

    let cache = SearchCache::new_with_path(&db_path).unwrap();

    // Track npm twice
    cache.track_choice("cors", Registry::Npm).unwrap();
    cache.track_choice("cors", Registry::Npm).unwrap();

    // Track go 4 times (more popular)
    cache.track_choice("cors", Registry::Go).unwrap();
    cache.track_choice("cors", Registry::Go).unwrap();
    cache.track_choice("cors", Registry::Go).unwrap();
    cache.track_choice("cors", Registry::Go).unwrap();

    // Should prefer Go (higher install count)
    let choice = cache.get_user_choice("cors").unwrap();
    assert_eq!(choice, Some(Registry::Go));
}

#[test]
fn test_user_choice_below_threshold() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test_cache.db");

    let cache = SearchCache::new_with_path(&db_path).unwrap();

    // Track only 2 times (below threshold of 3)
    cache.track_choice("axios", Registry::Npm).unwrap();
    cache.track_choice("axios", Registry::Npm).unwrap();

    // Should not return choice (below threshold)
    let choice = cache.get_user_choice("axios").unwrap();
    assert!(choice.is_none());
}

#[test]
fn test_cache_update() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test_cache.db");

    let cache = SearchCache::new_with_path(&db_path).unwrap();

    let results1 = vec![SearchResult {
        name: "test".to_string(),
        registry: Registry::Npm,
        full_path: "test".to_string(),
        version: "1.0.0".to_string(),
        description: "Old version".to_string(),
        metadata: ResultMetadata::default(),
        score: 80.0,
    }];

    cache.insert("test", &results1).unwrap();

    let results2 = vec![SearchResult {
        name: "test".to_string(),
        registry: Registry::Npm,
        full_path: "test".to_string(),
        version: "2.0.0".to_string(),
        description: "New version".to_string(),
        metadata: ResultMetadata::default(),
        score: 90.0,
    }];

    cache.insert("test", &results2).unwrap();

    let cached = cache.get("test").unwrap().unwrap();
    assert_eq!(cached[0].version, "2.0.0");
    assert_eq!(cached[0].description, "New version");
}
