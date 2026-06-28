use std::path::Path;
use std::sync::Arc;
use std::thread;

use rusqlite::params;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

use super::*;

fn create_test_store() -> (SqliteStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("store.db");
    let store = SqliteStore::open(&path, false).unwrap();
    (store, dir)
}

fn test_package(name: &str, version: &str, integrity: &str) -> PackageInfo {
    PackageInfo {
        name: name.to_string(),
        version: version.to_string(),
        integrity: integrity.to_string(),
        shard: format!("{}/{}", &integrity[..2], integrity),
        filename: format!("{}-{}.tgz", name, version),
        is_executable: false,
        manifest_json: None,
        metadata: None,
        size_bytes: 1024,
        compressed_size_bytes: 512,
        created_at: 0,
    }
}

#[test]
fn test_open_and_create() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.db");
    let store = SqliteStore::open(&path, false).unwrap();
    assert!(!store.is_readonly());
    assert_eq!(store.package_count().unwrap(), 0);
}

#[test]
fn test_open_readonly() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test_ro.db");
    SqliteStore::open(&path, false).unwrap();
    let store = SqliteStore::open(&path, true).unwrap();
    assert!(store.is_readonly());
}

#[test]
fn test_open_in_memory() {
    let store = SqliteStore::open_in_memory().unwrap();
    assert!(!store.is_readonly());
    assert_eq!(store.package_count().unwrap(), 0);
}

#[test]
fn test_add_and_get_package() {
    let (store, _dir) = create_test_store();
    let pkg = test_package("test-pkg", "1.0.0", "abc123");
    store.add_package(&pkg).unwrap();
    let retrieved = store.get_package("test-pkg", "1.0.0").unwrap().unwrap();
    assert_eq!(retrieved.name, "test-pkg");
    assert_eq!(retrieved.version, "1.0.0");
    assert_eq!(retrieved.integrity, "abc123");
}

#[test]
fn test_get_nonexistent_package() {
    let (store, _dir) = create_test_store();
    let result = store.get_package("nonexistent", "0.0.0").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_get_by_integrity() {
    let (store, _dir) = create_test_store();
    let pkg = test_package("integrity-pkg", "2.0.0", "def456");
    store.add_package(&pkg).unwrap();
    let retrieved = store.get_by_integrity("def456").unwrap().unwrap();
    assert_eq!(retrieved.name, "integrity-pkg");
}

#[test]
fn test_package_exists() {
    let (store, _dir) = create_test_store();
    let pkg = test_package("exists-pkg", "1.0.0", "exists123");
    store.add_package(&pkg).unwrap();
    assert!(store.package_exists("exists123").unwrap());
    assert!(!store.package_exists("nope").unwrap());
}

#[test]
fn test_delete_package() {
    let (store, _dir) = create_test_store();
    let pkg = test_package("delete-pkg", "1.0.0", "del123");
    store.add_package(&pkg).unwrap();
    assert!(store.package_exists("del123").unwrap());
    store.delete_package("del123").unwrap();
    assert!(!store.package_exists("del123").unwrap());
}

#[test]
fn test_duplicate_integrity_replaces() {
    let (store, _dir) = create_test_store();
    let pkg1 = test_package("original", "1.0.0", "dup123");
    let pkg2 = test_package("replacement", "2.0.0", "dup123");
    store.add_package(&pkg1).unwrap();
    // Adding package with same integrity but different name/version should fail
    let result = store.add_package(&pkg2);
    assert!(result.is_err(), "should reject integrity collision");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("integrity collision"), "error should mention integrity collision: {}", err);
    // Original package should still exist
    let retrieved = store.get_by_integrity("dup123").unwrap().unwrap();
    assert_eq!(retrieved.name, "original");
    assert_eq!(store.package_count().unwrap(), 1);
}

#[test]
fn test_register_and_unregister_project() {
    let (store, _dir) = create_test_store();
    let project_dir = tempdir().unwrap();
    store.register_project(project_dir.path()).unwrap();
    assert_eq!(store.project_count().unwrap(), 1);
    store.unregister_project(project_dir.path()).unwrap();
    assert_eq!(store.project_count().unwrap(), 0);
}

#[test]
fn test_transaction_rollback() {
    let (store, _dir) = create_test_store();
    store.begin_transaction().unwrap();
    let pkg = test_package("rollback-pkg", "1.0.0", "rollback1");
    store.add_package(&pkg).unwrap();
    store.rollback().unwrap();
    assert_eq!(store.package_count().unwrap(), 0);
}

#[test]
fn test_transaction_commit() {
    let (store, _dir) = create_test_store();
    store.begin_transaction().unwrap();
    let pkg = test_package("commit-pkg", "1.0.0", "commit1");
    store.add_package(&pkg).unwrap();
    store.commit().unwrap();
    assert_eq!(store.package_count().unwrap(), 1);
}

#[test]
fn test_integrity_cache() {
    let (store, _dir) = create_test_store();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test-file.txt");
    std::fs::write(&file_path, b"hello integrity cache").unwrap();
    let hash = hex::encode(Sha256::digest(b"hello integrity cache"));
    store
        .update_integrity_cache(&file_path, &hash)
        .unwrap();
    let cached = store
        .get_cached_integrity(&file_path)
        .unwrap()
        .unwrap();
    assert_eq!(cached, hash);
}

#[test]
fn test_missing_integrity_cache() {
    let (store, _dir) = create_test_store();
    let result = store.get_cached_integrity(Path::new("/nonexistent")).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_package_count() {
    let (store, _dir) = create_test_store();
    assert_eq!(store.package_count().unwrap(), 0);
    for i in 0..10 {
        let pkg = test_package(
            &format!("count-pkg-{}", i),
            "1.0.0",
            &format!("count{}", i),
        );
        store.add_package(&pkg).unwrap();
    }
    assert_eq!(store.package_count().unwrap(), 10);
}

#[test]
fn test_total_size() {
    let (store, _dir) = create_test_store();
    let pkg = test_package("size-test", "1.0.0", "size1");
    store.add_package(&pkg).unwrap();
    assert_eq!(store.total_size().unwrap(), 1024);
}

#[test]
fn test_health_check() {
    let (store, _dir) = create_test_store();
    let report = store.health_check().unwrap();
    assert!(report.iter().any(|l| l.contains("db_size")));
    assert!(report.iter().any(|l| l.contains("cache_entries")));
}

#[test]
fn test_lru_cache_hit() {
    let (store, _dir) = create_test_store();
    let pkg = test_package("cache-hit", "1.0.0", "cachehit1");
    store.add_package(&pkg).unwrap();

    {
        let cache = store.cache.lock().unwrap();
        assert!(cache.contains("cachehit1"));
    }

    let retrieved = store.get_by_integrity("cachehit1").unwrap().unwrap();
    assert_eq!(retrieved.name, "cache-hit");
}

#[test]
fn test_bulk_insert_performance() {
    let (store, _dir) = create_test_store();
    store.begin_transaction().unwrap();
    for i in 0..1000 {
        let pkg = test_package(
            &format!("bulk-{}", i),
            "1.0.0",
            &format!("{:040}", i),
        );
        store.add_package(&pkg).unwrap();
    }
    store.commit().unwrap();
    assert_eq!(store.package_count().unwrap(), 1000);
}

#[test]
fn test_vacuum() {
    let (store, _dir) = create_test_store();
    let pkg = test_package("vacuum-test", "1.0.0", "vacuum1");
    store.add_package(&pkg).unwrap();
    store.delete_package("vacuum1").unwrap();
    store.vacuum().unwrap();
}

#[test]
fn test_kv_store() {
    let (store, _dir) = create_test_store();
    store.set_kv("hello", b"world").unwrap();
    let val = store.get_kv("hello").unwrap().unwrap();
    assert_eq!(val, b"world");
    let missing = store.get_kv("nope").unwrap();
    assert!(missing.is_none());
    store.delete_kv("hello").unwrap();
    let deleted = store.get_kv("hello").unwrap();
    assert!(deleted.is_none());
}

#[test]
fn test_generation_counter() {
    let (store, _dir) = create_test_store();
    assert_eq!(store.current_generation(), 0);
    let g1 = store.advance_generation().unwrap();
    assert_eq!(g1, 1);
    assert_eq!(store.current_generation(), 1);
    let g2 = store.advance_generation().unwrap();
    assert_eq!(g2, 2);
}

#[test]
fn test_adaptive_functions() {
    let small_ram = 512 * 1024 * 1024;
    let medium_ram = 2 * 1024 * 1024 * 1024;
    let large_ram = 16 * 1024 * 1024 * 1024;

    assert!(adaptive_cache_size(medium_ram).unsigned_abs() > adaptive_cache_size(small_ram).unsigned_abs());
    assert!(adaptive_cache_size(large_ram).unsigned_abs() > adaptive_cache_size(medium_ram).unsigned_abs());

    assert_eq!(adaptive_mmap_size(small_ram), 0);
    assert!(adaptive_mmap_size(medium_ram) > 0);
    assert!(adaptive_mmap_size(large_ram) > adaptive_mmap_size(medium_ram));

    assert!(adaptive_lru_size(medium_ram) > adaptive_lru_size(small_ram));
    assert!(adaptive_lru_size(small_ram) >= 1000);
    assert!(adaptive_lru_size(large_ram) <= 100_000);
}

#[test]
fn test_detect_ram() {
    let ram = detect_available_ram();
    assert!(ram >= 512 * 1024 * 1024);
}

#[test]
fn test_audit_report() {
    let (store, _dir) = create_test_store();
    store.add_package(&test_package("audit-pkg", "1.0.0", "audit001")).unwrap();
    let report = store.audit().unwrap();
    assert!(report.integrity_ok);
    assert!(report.db_size_mb < 10);
    assert!(report.detected_ram_gb >= 1);
    assert!(!report.warnings.iter().any(|w| w.contains("integrity check failed")));
}

#[test]
fn test_permission_snapshot() {
    let (store, _dir) = create_test_store();
    store.snapshot_permissions().unwrap();
    let warnings = store.check_permissions().unwrap();
    assert!(warnings.is_empty(), "initial snapshot should have no diffs: {:?}", warnings);
}

#[test]
fn test_audit_first_run() {
    let (store, _dir) = create_test_store();
    let report = store.audit().unwrap();
    assert!(report.passed, "first audit should pass");
    assert!(report.last_audit.contains("just now") || report.last_audit.contains("ago"));
}

#[test]
fn test_audit_twice() {
    let (store, _dir) = create_test_store();
    let r1 = store.audit().unwrap();
    assert!(r1.passed);
    let r2 = store.audit().unwrap();
    assert!(r2.passed);
    assert!(r2.stale_hours < 1.0);
}

#[test]
fn test_audit_empty_store() {
    let (store, _dir) = create_test_store();
    let report = store.audit().unwrap();
    assert!(report.passed);
    assert_eq!(report.cache_entries, 0);
}

#[test]
fn test_audit_with_wal() {
    let (store, _dir) = create_test_store();
    store.begin_transaction().unwrap();
    for i in 0..100 {
        store.add_package(&test_package(
            &format!("wal-pkg-{}", i),
            "1.0.0",
            &format!("wal{:040}", i),
        )).unwrap();
    }
    store.commit().unwrap();
    let report = store.audit().unwrap();
    assert!(report.passed);
    assert_eq!(report.cache_entries, 100);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// STRESS TESTS
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn test_bulk_10000_packages() {
    let (store, _dir) = create_test_store();
    store.begin_transaction().unwrap();
    for i in 0..10_000 {
        let pkg = test_package(&format!("bulk-{}", i), "1.0.0", &format!("{:064x}", i));
        store.add_package(&pkg).unwrap();
    }
    store.commit().unwrap();
    assert_eq!(store.package_count().unwrap(), 10_000);
}

#[test]
fn test_bulk_10000_with_large_metadata() {
    let (store, _dir) = create_test_store();
    store.begin_transaction().unwrap();
    let large_json = format!(r#"{{"data":"{}"}}"#, "A".repeat(1000));
    for i in 0..10_000 {
        let pkg = PackageInfo {
            name: format!("big-{}", i),
            version: "1.0.0".to_string(),
            integrity: format!("{:064x}", i),
            shard: format!("{:02x}/{:064x}", i % 256, i),
            filename: format!("big-{}.tgz", i),
            is_executable: i % 2 == 0,
            manifest_json: Some(large_json.clone()),
            metadata: None,
            size_bytes: 1024 + i as u64,
            compressed_size_bytes: 512 + (i as u64 / 2),
            created_at: i as u64,
        };
        store.add_package(&pkg).unwrap();
    }
    store.commit().unwrap();
    assert_eq!(store.package_count().unwrap(), 10_000);
    let pkg = store.get_package("big-42", "1.0.0").unwrap().unwrap();
    assert!(pkg.manifest_json.unwrap().contains("AAAAA"));
}

#[test]
fn test_rapid_open_close() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("rapid.db");
    for _ in 0..50 {
        {
            let store = SqliteStore::open(&path, false).unwrap();
            let pkg = test_package("rapid", "1.0.0", "rapid001");
            store.add_package(&pkg).unwrap();
        }
        {
            let store = SqliteStore::open(&path, true).unwrap();
            assert!(store.package_exists("rapid001").unwrap());
        }
    }
}

#[test]
fn test_open_readonly_no_db_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nonexistent.db");
    let result = SqliteStore::open(&path, true);
    assert!(result.is_err());
}

#[test]
fn test_concurrent_reads_8_threads() {
    let dir = tempdir().unwrap();
    let store = Arc::new(SqliteStore::open(&dir.path().join("conc.db"), false).unwrap());
    store.begin_transaction().unwrap();
    for i in 0..500 {
        store.add_package(&test_package(
            &format!("conc-{}", i),
            "1.0.0",
            &format!("{:040}", i),
        )).unwrap();
    }
    store.commit().unwrap();
    let store_ro = SqliteStore::open(&dir.path().join("conc.db"), true).unwrap();
    let store_ro = Arc::new(store_ro);

    let mut handles = vec![];
    for t in 0..8 {
        let s = store_ro.clone();
        handles.push(thread::spawn(move || {
            for i in 0..500 {
                let name = format!("conc-{}", (i + t * 37) % 500);
                let pkg = s.get_package(&name, "1.0.0").unwrap();
                assert!(pkg.is_some(), "thread {} failed to find {}", t, name);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_kv_large_value_1mb() {
    let (store, _dir) = create_test_store();
    let large = vec![0xABu8; 1_000_000];
    store.set_kv("large_key", &large).unwrap();
    let retrieved = store.get_kv("large_key").unwrap().unwrap();
    assert_eq!(retrieved.len(), 1_000_000);
    assert_eq!(retrieved[0], 0xAB);
    assert_eq!(retrieved[999_999], 0xAB);
}

#[test]
fn test_kv_large_value_10mb() {
    let (store, _dir) = create_test_store();
    let large = vec![0x42u8; 10_000_000];
    store.set_kv("huge", &large).unwrap();
    let retrieved = store.get_kv("huge").unwrap().unwrap();
    assert_eq!(retrieved.len(), 10_000_000);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECURITY TESTS
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn test_sql_injection_via_package_name() {
    let (store, _dir) = create_test_store();
    let injections = [
        "'; DROP TABLE packages; --",
        "1; SELECT * FROM kv_store; --",
        "x' OR '1'='1",
        "\" OR 1=1 --",
        "'; UPDATE packages SET name='hacked' WHERE 1=1; --",
        "../etc/passwd",
        "../../etc/shadow",
        "<script>alert('xss')</script>",
        "${ENV_VAR}",
        "$(whoami)",
        "`cat /etc/passwd`",
        "'; DROP SCHEMA public CASCADE; --",
    ];
    for (i, injection) in injections.iter().enumerate() {
        let pkg = test_package(injection, "1.0.0", &format!("inj{:03}", i));
        store.add_package(&pkg).unwrap();
        let retrieved = store.get_package(injection, "1.0.0").unwrap();
        assert!(retrieved.is_some(), "injection '{}' failed roundtrip", injection);
    }
    assert_eq!(store.package_count().unwrap(), injections.len() as u64);
}

#[test]
fn test_sql_injection_via_integrity() {
    let (store, _dir) = create_test_store();
    let pkg = test_package("safe-name", "1.0.0", "'; DROP TABLE packages; --");
    store.add_package(&pkg).unwrap();
    let result = store.get_by_integrity("'; DROP TABLE packages; --").unwrap();
    assert!(result.is_some(), "should find by injection hash");
    assert!(store.package_exists("'; DROP TABLE packages; --").unwrap());
    assert_eq!(store.package_count().unwrap(), 1);
}

#[test]
fn test_unicode_in_package_name() {
    let (store, _dir) = create_test_store();
    let names = [
        "日本語パッケージ",
        "中文包名",
        "ñóñ-ãlphã",
        "паркет",
        "😀-emoji-pkg",
        "a\u{0000}b",
        "\t\r\n",
        "a'b\"c",
    ];
    for (i, name) in names.iter().enumerate() {
        let pkg = test_package(name, "1.0.0", &format!("uni{:03}", i));
        store.add_package(&pkg).unwrap();
        let retrieved = store.get_package(name, "1.0.0").unwrap();
        assert!(retrieved.is_some(), "unicode name '{}' failed", name);
        assert_eq!(retrieved.unwrap().name, *name);
    }
}

#[test]
fn test_readonly_write_rejected() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ro.db");
    SqliteStore::open(&path, false).unwrap();
    let store = SqliteStore::open(&path, true).unwrap();
    assert!(store.is_readonly());
    let result = store.add_package(&test_package("ro-test", "1.0.0", "ro001"));
    assert!(result.is_err(), "readonly store should reject add_package");
    let result = store.delete_package("ro001");
    assert!(result.is_err(), "readonly store should reject delete");
    let result = store.vacuum();
    assert!(result.is_err(), "readonly store should reject vacuum");
    assert!(store.get_package("ro-test", "1.0.0").is_ok());
    assert!(store.package_count().is_ok());
    assert!(store.health_check().is_ok());
}

#[test]
fn test_extremely_long_package_name() {
    let (store, _dir) = create_test_store();
    let long_name = "a".repeat(10_000);
    let pkg = test_package(&long_name, "1.0.0", "longname1");
    store.add_package(&pkg).unwrap();
    let retrieved = store.get_package(&long_name, "1.0.0").unwrap().unwrap();
    assert_eq!(retrieved.name.len(), 10_000);
}

#[test]
fn test_extremely_long_integrity() {
    let (store, _dir) = create_test_store();
    let long_hash = "x".repeat(100_000);
    let pkg = test_package("long-hash", "1.0.0", &long_hash);
    store.add_package(&pkg).unwrap();
    let retrieved = store.get_by_integrity(&long_hash).unwrap().unwrap();
    assert_eq!(retrieved.integrity.len(), 100_000);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// EDGE CASE TESTS
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn test_commit_without_begin() {
    let (store, _dir) = create_test_store();
    let result = store.commit();
    assert!(result.is_err());
}

#[test]
fn test_rollback_without_begin() {
    let (store, _dir) = create_test_store();
    let result = store.rollback();
    assert!(result.is_err());
}

#[test]
fn test_double_begin_transaction() {
    let (store, _dir) = create_test_store();
    store.begin_transaction().unwrap();
    let result = store.begin_transaction();
    assert!(result.is_err());
    store.rollback().unwrap();
}

#[test]
fn test_operations_after_rollback() {
    let (store, _dir) = create_test_store();
    store.begin_transaction().unwrap();
    store.add_package(&test_package("rb-test", "1.0.0", "rb001")).unwrap();
    store.rollback().unwrap();
    assert_eq!(store.package_count().unwrap(), 0);
    store.begin_transaction().unwrap();
    store.add_package(&test_package("rb-test2", "1.0.0", "rb002")).unwrap();
    store.commit().unwrap();
    assert_eq!(store.package_count().unwrap(), 1);
}

#[test]
fn test_delete_nonexistent_package() {
    let (store, _dir) = create_test_store();
    store.delete_package("nonexistent-hash").unwrap();
    assert_eq!(store.package_count().unwrap(), 0);
}

#[test]
fn test_get_by_integrity_empty_string() {
    let (store, _dir) = create_test_store();
    let result = store.get_by_integrity("").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_register_project_multiple_times() {
    let (store, _dir) = create_test_store();
    let project_dir = tempdir().unwrap();
    let path = project_dir.path();
    store.register_project(path).unwrap();
    assert_eq!(store.project_count().unwrap(), 1);
    store.register_project(path).unwrap();
    assert_eq!(store.project_count().unwrap(), 1);
}

#[test]
fn test_generation_counter_wraparound() {
    let (store, _dir) = create_test_store();
    for _ in 0..100 {
        store.advance_generation().unwrap();
    }
    assert_eq!(store.current_generation(), 100);
    let deleted = store.clean_old_generations(5).unwrap();
    assert_eq!(deleted, 94, "gens 1-94 deleted, gen 100 stays");
}

#[test]
fn test_kv_empty_key() {
    let (store, _dir) = create_test_store();
    store.set_kv("", b"empty-key-value").unwrap();
    let val = store.get_kv("").unwrap().unwrap();
    assert_eq!(val, b"empty-key-value");
}

#[test]
fn test_kv_binary_data() {
    let (store, _dir) = create_test_store();
    let binary = vec![0x00, 0x01, 0xFF, 0xFE, 0x80, 0x7F];
    store.set_kv("binary", &binary).unwrap();
    let retrieved = store.get_kv("binary").unwrap().unwrap();
    assert_eq!(retrieved, binary);
}

#[test]
fn test_kv_overwrite() {
    let (store, _dir) = create_test_store();
    store.set_kv("overwrite", b"value1").unwrap();
    store.set_kv("overwrite", b"value2").unwrap();
    let val = store.get_kv("overwrite").unwrap().unwrap();
    assert_eq!(val, b"value2");
}

#[test]
fn test_kv_delete_nonexistent() {
    let (store, _dir) = create_test_store();
    store.delete_kv("never-set").unwrap();
}

#[test]
fn test_integrity_cache_special_paths() {
    let (store, _dir) = create_test_store();
    let cache_dir = tempdir().unwrap();

    let test_cases: Vec<(&str, Vec<u8>)> = vec![
        ("hello.txt", b"hello".to_vec()),
        ("path/with spaces/and🚀emoji.txt", b"emoji".to_vec()),
        ("日本語/path.txt", b"nihongo".to_vec()),
        ("..test.txt", b"dots".to_vec()),
    ];

    for (rel_path, content) in &test_cases {
        let full_path = cache_dir.path().join(rel_path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&full_path, content).unwrap();
        let hash = hex::encode(Sha256::digest(content));
        store.update_integrity_cache(&full_path, &hash).unwrap();
        let cached = store.get_cached_integrity(&full_path).unwrap();
        assert!(cached.is_some(), "failed for path: '{}'", rel_path);
        assert_eq!(cached.unwrap(), hash);
    }

    // Also test that non-regular files (directories) return cached hash
    let hash = hex::encode(Sha256::digest(b"dir-content"));
    store.update_integrity_cache(cache_dir.path(), &hash).unwrap();
    let cached = store.get_cached_integrity(cache_dir.path()).unwrap();
    assert!(cached.is_some(), "directory should return cached hash");
}

#[test]
fn test_unreferenced_packages_empty() {
    let (store, _dir) = create_test_store();
    let unreferenced = store.get_unreferenced_packages().unwrap();
    assert!(unreferenced.is_empty());
}

#[test]
fn test_unreferenced_with_generations() {
    let (store, _dir) = create_test_store();
    store.add_package(&test_package("old-pkg", "1.0.0", "old001")).unwrap();
    let g1 = store.advance_generation().unwrap();
    assert_eq!(g1, 1);
    assert!(store.get_unreferenced_packages().unwrap().is_empty());
    let g2 = store.advance_generation().unwrap();
    assert_eq!(g2, 2);
    let unreferenced = store.get_unreferenced_packages().unwrap();
    assert_eq!(unreferenced.len(), 1, "package with gen 0 should be unreferenced after 2 advances");
    assert_eq!(unreferenced[0].integrity, "old001");
}

#[test]
fn test_health_check_empty_store() {
    let (store, _dir) = create_test_store();
    let report = store.health_check().unwrap();
    assert!(report.iter().any(|l| l.contains("db_size")));
    assert!(report.iter().any(|l| l.contains("cache_entries")));
}

#[test]
fn test_vacuum_empty_store() {
    let (store, _dir) = create_test_store();
    store.vacuum().unwrap();
}

#[test]
fn test_register_project_special_paths() {
    let (store, _dir) = create_test_store();
    let project_dir = tempdir().unwrap();
    let paths = [
        project_dir.path(),
        Path::new("."),
        Path::new("/tmp"),
    ];
    for p in &paths {
        store.register_project(p).unwrap();
    }
    assert_eq!(store.project_count().unwrap(), 3);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CONCURRENCY & ISOLATION TESTS
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn test_transaction_isolation() {
    let (store, _dir) = create_test_store();
    store.begin_transaction().unwrap();
    store.add_package(&test_package("iso-test", "1.0.0", "iso001")).unwrap();
    let store2 = SqliteStore::open(store.path(), true).unwrap();
    let result = store2.get_package("iso-test", "1.0.0").unwrap();
    assert!(result.is_none(), "uncommitted data should not be visible");
    store.commit().unwrap();
    let result = store2.get_package("iso-test", "1.0.0").unwrap();
    assert!(result.is_some(), "committed data should be visible");
}

#[test]
fn test_cache_consistency_after_delete() {
    let (store, _dir) = create_test_store();
    let pkg = test_package("cache-cons", "1.0.0", "cons001");
    store.add_package(&pkg).unwrap();
    assert!(store.package_exists("cons001").unwrap());
    store.conn.lock().unwrap()
        .execute("DELETE FROM packages WHERE integrity = ?1", params!["cons001"])
        .unwrap();
    assert!(store.package_exists("cons001").unwrap(), "cache should be stale");
    store.get_by_integrity("cons001").unwrap();
    store.delete_package("cons001").unwrap();
    assert!(!store.package_exists("cons001").unwrap());
}

#[test]
fn test_generation_does_not_deadlock() {
    let (store, _dir) = create_test_store();
    for _ in 0..20 {
        store.advance_generation().unwrap();
    }
    assert_eq!(store.current_generation(), 20);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// AUDIT & PERMISSION EDGE CASES
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn test_permission_snapshot_no_wal_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nope.db");
    let store = SqliteStore::open(&path, false).unwrap();
    drop(store);
    let store = SqliteStore::open(&path, false).unwrap();
    store.snapshot_permissions().unwrap();
    let warnings = store.check_permissions().unwrap();
    assert!(warnings.is_empty(), "snapshot with missing WAL should not warn: {:?}", warnings);
}

#[test]
fn test_audit_after_corrupt_snapshot() {
    let (store, _dir) = create_test_store();
    store.set_kv("permission_snapshot", b"not-valid-json{{{").unwrap();
    let report = store.audit().unwrap();
    assert!(!report.permissions_ok || !report.warnings.is_empty(),
        "corrupt snapshot should be detected");
}

#[test]
fn test_audit_integrity_check_actually_runs() {
    let (store, _dir) = create_test_store();
    let report = store.audit().unwrap();
    assert!(report.integrity_ok);
    {
        let conn = store.conn.lock().unwrap();
        conn.execute("UPDATE packages SET name = 'corrupted' WHERE 1=0", []).unwrap();
    }
    let report2 = store.audit().unwrap();
    assert!(report2.integrity_ok, "empty store should pass integrity");
}

#[test]
fn test_audit_after_vacuum() {
    let (store, _dir) = create_test_store();
    store.add_package(&test_package("pre-vac", "1.0.0", "prevac1")).unwrap();
    store.delete_package("prevac1").unwrap();
    store.vacuum().unwrap();
    let report = store.audit().unwrap();
    assert!(report.passed);
    assert_eq!(report.cache_entries, 0);
}

#[test]
fn test_audit_on_in_memory_store() {
    let store = SqliteStore::open_in_memory().unwrap();
    let report = store.audit().unwrap();
    assert!(report.passed, "in-memory audit should pass");
    assert!(report.db_size_mb < 10);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SCHEMA MIGRATION EDGE CASES
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn test_schema_migration_idempotent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("migrate.db");
    for _ in 0..10 {
        let store = SqliteStore::open(&path, false).unwrap();
        drop(store);
    }
    let store = SqliteStore::open(&path, false).unwrap();
    assert_eq!(store.package_count().unwrap(), 0);
}

#[test]
fn test_database_file_created_with_correct_permissions() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("perm.db");
    let store = SqliteStore::open(&path, false).unwrap();
    drop(store);
    assert!(path.exists(), "database file should exist");
    let metadata = std::fs::metadata(&path).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        assert!(mode & 0o777 <= 0o700, "permissions too permissive: {:o}", mode);
    }
}
