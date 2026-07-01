//! Integration tests for the content store

use mgpm_store::ContentStore;
use std::fs;

#[test]
fn test_store_import_export() {
    let dir = tempfile::tempdir().unwrap();
    let store = ContentStore::new(dir.path().to_path_buf()).unwrap();

    let src = dir.path().join("test.txt");
    fs::write(&src, "hello store").unwrap();

    let (hash, _) = store.import_file(&src).unwrap();
    assert!(store.has_file(&hash));

    let stored_path = store.get_file(&hash).unwrap();
    let content = fs::read_to_string(&stored_path).unwrap();
    assert_eq!(content, "hello store");
}

#[test]
fn test_deduplication() {
    let dir = tempfile::tempdir().unwrap();
    let store = ContentStore::new(dir.path().to_path_buf()).unwrap();

    let src1 = dir.path().join("file1.txt");
    let src2 = dir.path().join("file2.txt");
    fs::write(&src1, "duplicate content").unwrap();
    fs::write(&src2, "duplicate content").unwrap();

    let (hash1, _) = store.import_file(&src1).unwrap();
    let (hash2, _) = store.import_file(&src2).unwrap();

    assert_eq!(hash1, hash2);
    assert_eq!(store.get_ref_count(&hash1), 2);
}

#[test]
fn test_gc() {
    let dir = tempfile::tempdir().unwrap();
    let store = ContentStore::new(dir.path().to_path_buf()).unwrap();

    let src = dir.path().join("gc_test.txt");
    fs::write(&src, "gc me").unwrap();

    let (hash, _) = store.import_file(&src).unwrap();
    assert!(store.has_file(&hash));

    store.dec_ref(&hash).unwrap();
    assert_eq!(store.get_ref_count(&hash), 0);

    let removed = store.gc().unwrap();
    assert!(removed >= 1);
    assert!(!store.has_file(&hash));
}

#[test]
fn test_orphan_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let store = ContentStore::new(dir.path().to_path_buf()).unwrap();

    let orphan_hash = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let orphan_path = dir
        .path()
        .join("files")
        .join("sha256")
        .join(&orphan_hash[..2])
        .join(orphan_hash);
    fs::create_dir_all(orphan_path.parent().unwrap()).unwrap();
    fs::write(&orphan_path, "orphan").unwrap();

    let removed = store.gc().unwrap();
    assert!(removed >= 1);
    assert!(!orphan_path.exists());
}

#[test]
fn test_cross_filesystem_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let store = ContentStore::new(dir.path().to_path_buf()).unwrap();

    let src = dir.path().join("cross_fs.txt");
    fs::write(&src, "cross-fs test").unwrap();

    // import_file_fallback should succeed even if reflink/hardlink fail
    let (hash, method) = store.import_file_fallback(&src).unwrap();
    assert!(store.has_file(&hash));
    assert!(matches!(
        method,
        mgpm_store::ImportMethod::Copy
            | mgpm_store::ImportMethod::Hardlink
            | mgpm_store::ImportMethod::Reflink
    ));
}

#[test]
fn test_verify_integrity() {
    let dir = tempfile::tempdir().unwrap();
    let store = ContentStore::new(dir.path().to_path_buf()).unwrap();

    let src = dir.path().join("verify.txt");
    fs::write(&src, "integrity check").unwrap();

    let (hash, _) = store.import_file(&src).unwrap();
    let path = store.get_file(&hash).unwrap();

    assert!(store.verify_integrity(&hash, &path).is_ok());
}
