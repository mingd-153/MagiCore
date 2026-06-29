use std::fs;

use tempfile::tempdir;

use super::StoreVerifier;
use crate::store::cas::ContentStore;
use crate::store::index::PackageInfo;
use crate::store::SqliteStore;

fn make_store_ready() -> (ContentStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let index = SqliteStore::open_in_memory().unwrap();

    // Advance generation to make newly added packages unreferenced by default
    // After 2 advances: MAX(generation) = 2, MAX - 1 = 1
    // Packages with generation < 1 are unreferenced
    index.advance_generation().unwrap();
    index.advance_generation().unwrap();

    let store = ContentStore::new(dir.path().join("cas"), Box::new(index)).unwrap();
    (store, dir)
}

fn make_store() -> (ContentStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let index = SqliteStore::open_in_memory().unwrap();
    let store = ContentStore::new(dir.path().join("cas"), Box::new(index)).unwrap();
    (store, dir)
}

fn add_test_package(
    store: &ContentStore,
    name: &str,
    version: &str,
    data: &[u8],
) -> PackageInfo {
    let hash = store.import_bytes(data).unwrap();
    let info = PackageInfo {
        name: name.to_string(),
        version: version.to_string(),
        integrity: hash.hash.clone(),
        shard: hash.shard.clone(),
        filename: hash.filename.clone(),
        is_executable: false,
        manifest_json: None,
        metadata: None,
        size_bytes: data.len() as u64,
        compressed_size_bytes: data.len() as u64,
        created_at: 0,
    };
    store.index().add_package(&info).unwrap();
    info
}

#[test]
fn test_verify_clean_store() {
    let (store, _dir) = make_store();
    add_test_package(&store, "pkg-a", "1.0.0", b"content a");
    add_test_package(&store, "pkg-b", "2.0.0", b"content b");

    let verifier = StoreVerifier::new(&store, store.index());
    let report = verifier.verify(false).unwrap();

    assert_eq!(report.total_packages, 2);
    assert_eq!(report.verified, 2);
    assert!(report.corrupted_files.is_empty());
    assert!(report.missing_files.is_empty());
    assert!(report.is_healthy());
}

#[test]
fn test_verify_with_corrupted_file() {
    let (store, _dir) = make_store();
    let info = add_test_package(&store, "pkg-a", "1.0.0", b"original content");

    let cas_path = store.root().join(&info.shard).join(&info.filename);
    fs::write(&cas_path, b"tampered content").unwrap();

    let verifier = StoreVerifier::new(&store, store.index());
    let report = verifier.verify(false).unwrap();

    assert_eq!(report.total_packages, 1);
    assert_eq!(report.verified, 0);
    assert_eq!(report.corrupted_files.len(), 1);
    assert!(!report.is_healthy());
}

#[test]
fn test_verify_with_missing_file() {
    let (store, _dir) = make_store();
    let info = add_test_package(&store, "pkg-a", "1.0.0", b"content");

    let cas_path = store.root().join(&info.shard).join(&info.filename);
    fs::remove_file(&cas_path).unwrap();

    let verifier = StoreVerifier::new(&store, store.index());
    let report = verifier.verify(false).unwrap();

    assert_eq!(report.total_packages, 1);
    assert_eq!(report.verified, 0);
    assert_eq!(report.missing_files.len(), 1);
    assert!(!report.is_healthy());
}

#[test]
fn test_status_empty_store() {
    let (store, _dir) = make_store();

    let verifier = StoreVerifier::new(&store, store.index());
    let report = verifier.status().unwrap();

    assert_eq!(report.total_packages, 0);
    assert_eq!(report.total_projects, 0);
    assert_eq!(report.total_size_bytes, 0);
    assert!(report.unreferenced_packages.is_empty());
}

#[test]
fn test_status_with_packages() {
    let (store, _dir) = make_store();
    add_test_package(&store, "pkg-a", "1.0.0", b"content a");
    add_test_package(&store, "pkg-b", "2.0.0", b"content b");

    let verifier = StoreVerifier::new(&store, store.index());
    let report = verifier.status().unwrap();

    assert_eq!(report.total_packages, 2);
    assert!(report.total_size_bytes > 0);
}

#[test]
fn test_prune_dry_run() {
    let (store, _dir) = make_store_ready();
    add_test_package(&store, "pkg-a", "1.0.0", b"content a");

    let verifier = StoreVerifier::new(&store, store.index());
    let report = verifier.prune(true).unwrap();

    assert_eq!(report.unreferenced_packages.len(), 1);
    assert!(report.reclaimable_bytes > 0);

    let status = verifier.status().unwrap();
    assert_eq!(status.total_packages, 1);
}

#[test]
fn test_prune_removes_unreferenced() {
    let (store, _dir) = make_store_ready();
    add_test_package(&store, "pkg-a", "1.0.0", b"content a");

    let verifier = StoreVerifier::new(&store, store.index());
    let report = verifier.prune(false).unwrap();

    assert_eq!(report.unreferenced_packages.len(), 1);

    let status = verifier.status().unwrap();
    assert_eq!(status.total_packages, 0);
}

#[test]
fn test_prune_keeps_referenced() {
    let (store, _dir) = make_store();

    let data = b"referenced content";
    let hash = store.import_bytes(data).unwrap();
    let info = PackageInfo {
        name: "pkg-r".to_string(),
        version: "1.0.0".to_string(),
        integrity: hash.hash.clone(),
        shard: hash.shard.clone(),
        filename: hash.filename.clone(),
        is_executable: false,
        manifest_json: None,
        metadata: None,
        size_bytes: data.len() as u64,
        compressed_size_bytes: data.len() as u64,
        created_at: 0,
    };
    store.index().add_package(&info).unwrap();

    let verifier = StoreVerifier::new(&store, store.index());
    let report = verifier.prune(false).unwrap();

    // Package is NOT unreferenced (no gc_state yet, generation=0)
    assert_eq!(report.unreferenced_packages.len(), 0);

    let status = verifier.status().unwrap();
    assert_eq!(status.total_packages, 1);
}

#[test]
fn test_prune_empty_shards() {
    let (store, _dir) = make_store_ready();
    let info = add_test_package(&store, "pkg-a", "1.0.0", b"content a");
    let shard_dir = store.root().join(&info.shard);

    assert!(shard_dir.exists());

    let verifier = StoreVerifier::new(&store, store.index());
    verifier.prune(false).unwrap();

    assert!(!shard_dir.exists());
}

#[test]
fn test_verify_fix_removes_corrupted() {
    let (store, _dir) = make_store();
    let info = add_test_package(&store, "pkg-a", "1.0.0", b"original");

    let cas_path = store.root().join(&info.shard).join(&info.filename);
    fs::write(&cas_path, b"tampered").unwrap();

    let verifier = StoreVerifier::new(&store, store.index());
    let report = verifier.verify(true).unwrap();

    assert_eq!(report.corrupted_files.len(), 1);
    assert!(!cas_path.exists());
}

#[test]
fn test_verify_readonly_rejects_fix() {
    let dir = tempdir().unwrap();
    
    // Create a file-based store (not in-memory)
    let index = SqliteStore::open(&dir.path().join("index.db"), false).unwrap();
    index.advance_generation().unwrap();
    index.advance_generation().unwrap();
    
    let store = ContentStore::new(dir.path().join("cas"), Box::new(index)).unwrap();
    add_test_package(&store, "pkg-a", "1.0.0", b"content a");
    
    // Reopen as readonly
    let ro_index = SqliteStore::open(&dir.path().join("index.db"), true).unwrap();
    let ro_store = ContentStore::new(dir.path().join("cas"), Box::new(ro_index)).unwrap();
    
    let verifier = StoreVerifier::new(&ro_store, ro_store.index());
    let result = verifier.verify(true);
    
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("readonly"));
}

#[test]
fn test_prune_readonly_rejects() {
    let dir = tempdir().unwrap();
    
    let index = SqliteStore::open(&dir.path().join("index.db"), false).unwrap();
    index.advance_generation().unwrap();
    index.advance_generation().unwrap();
    
    let store = ContentStore::new(dir.path().join("cas"), Box::new(index)).unwrap();
    add_test_package(&store, "pkg-a", "1.0.0", b"content a");
    
    let ro_index = SqliteStore::open(&dir.path().join("index.db"), true).unwrap();
    let ro_store = ContentStore::new(dir.path().join("cas"), Box::new(ro_index)).unwrap();
    
    let verifier = StoreVerifier::new(&ro_store, ro_store.index());
    let result = verifier.prune(false);
    
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("readonly"));
}
