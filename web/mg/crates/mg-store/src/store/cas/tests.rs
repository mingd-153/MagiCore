use std::fs;

use tempfile::tempdir;

use super::super::SqliteStore;
use super::{ContentStore, IntegrityHash, TarballEntry};

fn make_store() -> (ContentStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let index = SqliteStore::open_in_memory().unwrap();
    let store = ContentStore::new(dir.path().join("cas"), Box::new(index)).unwrap();
    (store, dir)
}

#[test]
fn test_import_bytes_new() {
    let (store, _dir) = make_store();
    let hash = store.import_bytes(b"hello world").unwrap();
    assert_eq!(hash.hash.len(), 64);
    assert_eq!(hash.shard.len(), 2);
    assert!(store.contains(&hash));
}

#[test]
fn test_import_bytes_dedup() {
    let (store, _dir) = make_store();
    let h1 = store.import_bytes(b"same content").unwrap();
    let h2 = store.import_bytes(b"same content").unwrap();
    assert_eq!(h1.hash, h2.hash);
    assert_eq!(h1.cas_path(store.root()), h2.cas_path(store.root()));
}

#[test]
fn test_import_file() {
    let (store, dir) = make_store();
    let src = dir.path().join("test.txt");
    fs::write(&src, b"file content").unwrap();
    let hash = store.import_file(&src).unwrap();
    assert!(store.contains(&hash));
}

#[test]
fn test_import_file_symlink_rejected() {
    let (store, dir) = make_store();
    let src = dir.path().join("real.txt");
    let link = dir.path().join("link.txt");
    fs::write(&src, b"content").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&src, &link).unwrap();
    let result = store.import_file(&link);
    assert!(result.is_err());
}

#[test]
fn test_export_hardlink() {
    let (store, dir) = make_store();
    let hash = store.import_bytes(b"export test").unwrap();
    let dest = dir.path().join("output.txt");
    store.export_to(&hash, &dest).unwrap();
    assert!(dest.exists());
    let content = fs::read(&dest).unwrap();
    assert_eq!(content, b"export test");
}

#[test]
fn test_export_verify_hash() {
    let (store, dir) = make_store();
    let hash = store.import_bytes(b"verify export").unwrap();
    let dest = dir.path().join("output.txt");
    store.export_to(&hash, &dest).unwrap();
    let computed = IntegrityHash::from_bytes(b"verify export", false);
    assert_eq!(hash.hash, computed.hash);
}

#[test]
fn test_verify_content() {
    let (store, dir) = make_store();
    let file_path = dir.path().join("data.bin");
    fs::write(&file_path, b"verify me").unwrap();
    let hash = store.verify(&file_path).unwrap();
    assert_eq!(hash.hash.len(), 64);
}

#[test]
fn test_contains_missing() {
    let (store, _dir) = make_store();
    let hash = IntegrityHash::from_bytes(b"nonexistent", false);
    assert!(!store.contains(&hash));
}

#[test]
fn test_remove() {
    let (store, _dir) = make_store();
    let hash = store.import_bytes(b"remove me").unwrap();
    assert!(store.contains(&hash));
    store.remove(&hash).unwrap();
    assert!(!store.contains(&hash));
}

#[test]
fn test_import_tarball_entries() {
    let (store, _dir) = make_store();
    let entries = vec![
        TarballEntry {
            path: "dir/file1.txt".into(),
            data: b"content1".to_vec(),
            executable: false,
        },
        TarballEntry {
            path: "dir/file2.js".into(),
            data: b"content2".to_vec(),
            executable: false,
        },
    ];
    let hashes = store.import_tarball_entries(entries).unwrap();
    assert_eq!(hashes.len(), 2);
    for h in &hashes {
        assert!(store.contains(h));
    }
}

#[test]
fn test_import_bytes_exact_content() {
    let (store, dir) = make_store();
    let hash = store.import_bytes(b"exact match").unwrap();
    let dest = dir.path().join("verify.txt");
    store.export_to(&hash, &dest).unwrap();
    let exported = fs::read(&dest).unwrap();
    assert_eq!(exported, b"exact match");
}

#[test]
fn test_export_nonexistent_hash() {
    let (store, dir) = make_store();
    let hash = IntegrityHash::from_bytes(b"ghost", false);
    let dest = dir.path().join("ghost.txt");
    let result = store.export_to(&hash, &dest);
    assert!(result.is_err());
}

#[test]
fn test_export_dest_already_exists() {
    let (store, dir) = make_store();
    let hash = store.import_bytes(b"existing dest").unwrap();
    let dest = dir.path().join("exists.txt");
    fs::write(&dest, b"preexisting").unwrap();
    let result = store.export_to(&hash, &dest);
    assert!(result.is_err());
}

#[test]
fn test_multiple_imports_unique() {
    let (store, _dir) = make_store();
    let h1 = store.import_bytes(b"content a").unwrap();
    let h2 = store.import_bytes(b"content b").unwrap();
    let h3 = store.import_bytes(b"content c").unwrap();
    assert_ne!(h1.hash, h2.hash);
    assert_ne!(h2.hash, h3.hash);
    assert_ne!(h1.hash, h3.hash);
}

#[test]
fn test_remove_nonexistent() {
    let (store, _dir) = make_store();
    let hash = IntegrityHash::from_bytes(b"never added", false);
    store.remove(&hash).unwrap();
}

#[test]
fn test_import_bytes_empty() {
    let (store, _dir) = make_store();
    let hash = store.import_bytes(b"").unwrap();
    assert!(store.contains(&hash));
    assert_eq!(hash.hash.len(), 64);
}

#[test]
fn test_export_then_remove_then_verify_gone() {
    let (store, dir) = make_store();
    let hash = store.import_bytes(b"lifecycle test").unwrap();
    let dest = dir.path().join("lifecycle.txt");
    store.export_to(&hash, &dest).unwrap();
    assert!(dest.exists());
    store.remove(&hash).unwrap();
    assert!(!store.contains(&hash));
}

#[test]
fn test_large_content_import_export() {
    let (store, dir) = make_store();
    let data = vec![0xABu8; 65536];
    let hash = store.import_bytes(&data).unwrap();
    let dest = dir.path().join("large.bin");
    store.export_to(&hash, &dest).unwrap();
    let exported = fs::read(&dest).unwrap();
    assert_eq!(exported.len(), 65536);
    assert!(exported.iter().all(|&b| b == 0xAB));
}
