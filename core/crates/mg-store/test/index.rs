//! T6 store-index tests: msgpack roundtrip, corrupt-rebuild, isolated upsert,
//! prune safety. Test riêng tại test/ (RULE §5).

use mg_store::{Database, FileEntry, Layout, StoreIndex};
use std::path::PathBuf;

fn test_layout() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn open_index(dir: &tempfile::TempDir) -> StoreIndex {
    let layout = Layout::new(dir.path().to_path_buf());
    let db = Database::open(&dir.path().join("store.db")).unwrap();
    StoreIndex::open(&layout, db).unwrap()
}

fn files(pairs: &[(&str, &str)]) -> Vec<FileEntry> {
    pairs
        .iter()
        .enumerate()
        .map(|(i, (path, hash))| FileEntry {
            path: path.to_string(),
            blob_hash: hash.to_string(),
            size: (i as u64 + 1) * 10,
        })
        .collect()
}

#[test]
fn upsert_then_verify_roundtrip() {
    let dir = test_layout();
    let mut index = open_index(&dir);
    let pkg = mg_types::PackageId::parse("react@18.2.0").unwrap();
    index
        .upsert_package_files(
            &pkg,
            files(&[("index.js", "aaaa"), ("package.json", "bbbb")]),
        )
        .unwrap();
    let v = index.verify_package(&pkg).unwrap();
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].path, "index.js");
    assert_eq!(v[1].blob_hash, "bbbb");
    assert_eq!(index.list_blob_hashes().unwrap(), vec!["aaaa", "bbbb"]);
}

#[test]
fn upsert_second_package_keeps_first() {
    let dir = test_layout();
    let mut index = open_index(&dir);
    let a = mg_types::PackageId::parse("react@18.2.0").unwrap();
    let b = mg_types::PackageId::parse("vue@3.4.0").unwrap();
    index
        .upsert_package_files(&a, files(&[("a.js", "h1")]))
        .unwrap();
    index
        .upsert_package_files(&b, files(&[("b.js", "h2")]))
        .unwrap();
    assert_eq!(index.verify_package(&a).unwrap().len(), 1);
    assert_eq!(index.verify_package(&b).unwrap().len(), 1);
}

#[test]
fn reopen_reads_same_index_from_msgpack() {
    let dir = test_layout();
    let pkg = mg_types::PackageId::parse("lodash@4.17.21").unwrap();
    {
        let layout = Layout::new(dir.path().to_path_buf());
        let db = Database::open(&dir.path().join("store.db")).unwrap();
        let mut index = StoreIndex::open(&layout, db).unwrap();
        index
            .upsert_package_files(&pkg, files(&[("fp.js", "h9")]))
            .unwrap();
    }
    // New process view: fresh open reads msgpack, no SQLite rebuild needed.
    let layout = Layout::new(dir.path().to_path_buf());
    let db = Database::open(&dir.path().join("store.db")).unwrap();
    let index = StoreIndex::open(&layout, db).unwrap();
    assert_eq!(index.verify_package(&pkg).unwrap()[0].blob_hash, "h9");
}

#[test]
fn corrupt_msgpack_is_rebuilt_from_sqlite() {
    let dir = test_layout();
    let pkg = mg_types::PackageId::parse("chalk@5.0.0").unwrap();
    {
        let layout = Layout::new(dir.path().to_path_buf());
        let db = Database::open(&dir.path().join("store.db")).unwrap();
        let mut index = StoreIndex::open(&layout, db).unwrap();
        index
            .upsert_package_files(&pkg, files(&[("source.js", "c1")]))
            .unwrap();
    }
    // Corrupt the msgpack file on disk.
    std::fs::write(dir.path().join("index.msgpack"), b"\x81\xa5garbage").unwrap();
    let layout = Layout::new(dir.path().to_path_buf());
    let db = Database::open(&dir.path().join("store.db")).unwrap();
    let index = StoreIndex::open(&layout, db).unwrap();
    assert_eq!(index.verify_package(&pkg).unwrap()[0].blob_hash, "c1");
}

#[test]
fn missing_msgpack_rebuilds_empty_without_error() {
    let dir = test_layout();
    let layout = Layout::new(dir.path().to_path_buf());
    let db = Database::open(&dir.path().join("store.db")).unwrap();
    let index = StoreIndex::open(&layout, db).unwrap();
    assert!(index
        .verify_package(&mg_types::PackageId::parse("x@1.0.0").unwrap())
        .is_none());
}

#[test]
fn update_same_package_replaces_not_duplicates() {
    let dir = test_layout();
    let mut index = open_index(&dir);
    let pkg = mg_types::PackageId::parse("preact@10.0.0").unwrap();
    index
        .upsert_package_files(&pkg, files(&[("a.js", "h1"), ("b.js", "h2")]))
        .unwrap();
    index
        .upsert_package_files(&pkg, files(&[("only.js", "h3")]))
        .unwrap();
    let v = index.verify_package(&pkg).unwrap();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].path, "only.js");
    assert_eq!(index.list_blob_hashes().unwrap(), vec!["h3"]);
}

#[test]
fn prune_removes_only_unreferenced_old_blobs() {
    let dir = test_layout();
    let mut index = open_index(&dir);
    let pkg = mg_types::PackageId::parse("pkg@1.0.0").unwrap();
    index
        .upsert_package_files(&pkg, files(&[("kept.js", "dead-beef-kept")]))
        .unwrap();

    // Create actual blob files under CAS layout: one referenced, one stale.
    let cas_root = dir.path().join("cas");
    let blob_dir = cas_root.join("files").join("blake3").join("de");
    std::fs::create_dir_all(&blob_dir).unwrap();
    let kept = blob_dir.join("dead-beef-kept");
    let stale = blob_dir.join("dead-beef-stale");
    std::fs::write(&kept, b"data").unwrap();
    std::fs::write(&stale, b"old").unwrap();
    backdate(&stale);

    let removed = index
        .prune_blobs(&cas_root, std::time::Duration::from_millis(0))
        .unwrap();
    assert_eq!(removed, 1);
    assert!(kept.exists());
    assert!(!stale.exists());
}

#[test]
fn prune_skips_blob_with_external_hardlink() {
    let dir = test_layout();
    let index = open_index(&dir);
    let cas_root = dir.path().join("cas");
    let blob_dir = cas_root.join("files").join("blake3").join("li");
    std::fs::create_dir_all(&blob_dir).unwrap();
    let blob = blob_dir.join("linked-blob-hash");
    std::fs::write(&blob, b"x").unwrap();
    let link = dir.path().join("external-link");
    std::fs::hard_link(&blob, &link).unwrap();
    backdate(&blob);

    let removed = index
        .prune_blobs(&cas_root, std::time::Duration::from_millis(0))
        .unwrap();
    assert_eq!(removed, 0);
    assert!(blob.exists());
}

// Helper: make sure ip_metadata mtime is old enough (prune uses elapsed > max_age).
fn backdate(path: &PathBuf) {
    let past = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    let times = std::fs::FileTimes::new().set_modified(past);
    let _ = std::fs::File::open(path).unwrap().set_times(times);
}

#[test]
fn prune_skips_fresh_blobs_even_if_unreferenced() {
    let dir = test_layout();
    let index = open_index(&dir);
    let cas_root = dir.path().join("cas");
    let blob_dir = cas_root.join("files").join("blake3").join("fr");
    std::fs::create_dir_all(&blob_dir).unwrap();
    let fresh = blob_dir.join("fresh-unreferenced");
    std::fs::write(&fresh, b"new").unwrap();

    let removed = index
        .prune_blobs(&cas_root, std::time::Duration::from_secs(60))
        .unwrap();
    assert_eq!(removed, 0);
    assert!(fresh.exists());
}

#[test]
fn prune_referenced_blob_is_never_removed() {
    let dir = test_layout();
    let mut index = open_index(&dir);
    let pkg = mg_types::PackageId::parse("keep@1.0.0").unwrap();
    index
        .upsert_package_files(&pkg, files(&[("r.js", "reference-me")]))
        .unwrap();
    let cas_root = dir.path().join("cas");
    let blob_dir = cas_root.join("files").join("blake3").join("re");
    std::fs::create_dir_all(&blob_dir).unwrap();
    let blob = blob_dir.join("reference-me");
    std::fs::write(&blob, b"keep").unwrap();
    backdate(&blob);

    let removed = index
        .prune_blobs(&cas_root, std::time::Duration::from_millis(0))
        .unwrap();
    assert_eq!(removed, 0);
    assert!(blob.exists());
}

#[test]
fn digest_of_msgpack_is_stable_across_reopen() {
    let dir = test_layout();
    let pkg = mg_types::PackageId::parse("stable@1.0.0").unwrap();
    let first = {
        let layout = Layout::new(dir.path().to_path_buf());
        let db = Database::open(&dir.path().join("store.db")).unwrap();
        let mut index = StoreIndex::open(&layout, db).unwrap();
        index
            .upsert_package_files(&pkg, files(&[("s.js", "s1")]))
            .unwrap();
        std::fs::read(layout.index_msgpack_path()).unwrap()
    };
    let second = {
        let layout = Layout::new(dir.path().to_path_buf());
        let db = Database::open(&dir.path().join("store.db")).unwrap();
        let _index = StoreIndex::open(&layout, db).unwrap();
        std::fs::read(layout.index_msgpack_path()).unwrap()
    };
    assert_eq!(first, second);
}
