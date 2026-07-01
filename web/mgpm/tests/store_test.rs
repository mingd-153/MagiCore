#![cfg(test)]

use std::fs;

use mgpm_store::{CasContentStore, IntegrityHash, PackageInfo, SqliteStore};

fn make_store() -> (CasContentStore, SqliteStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("store.db");
    let index = SqliteStore::open(&db_path, false).unwrap();
    let store = CasContentStore::new(dir.path().join("cas"), Box::new(index.clone())).unwrap();
    (store, index, dir)
}

fn add_package(
    store: &CasContentStore,
    name: &str,
    version: &str,
    hash: &IntegrityHash,
    data: &[u8],
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let info = PackageInfo {
        name: name.to_string(),
        version: version.to_string(),
        integrity: hash.hash.clone(),
        shard: hash.shard.clone(),
        filename: hash.filename.clone(),
        is_executable: hash.is_executable,
        manifest_json: Some("{}".to_string()),
        metadata: None,
        size_bytes: data.len() as u64,
        compressed_size_bytes: data.len() as u64,
        created_at: now,
    };
    store.index().add_package(&info).unwrap();
}

#[test]
fn test_store_full_lifecycle() {
    let (store, sqlite, dir) = make_store();

    let src = dir.path().join("input.txt");
    fs::write(&src, b"lifecycle test data").unwrap();

    let hash = store.import_file(&src).unwrap();
    assert!(store.contains(&hash));

    add_package(&store, "test-pkg", "1.0.0", &hash, b"lifecycle test data");

    let lookup = store
        .index()
        .get_by_integrity(&hash.hash)
        .unwrap()
        .expect("package should exist in index");
    assert_eq!(lookup.name, "test-pkg");
    assert_eq!(lookup.version, "1.0.0");

    let export_path = dir.path().join("exported.txt");
    store.export_to(&hash, &export_path).unwrap();
    let exported_content = fs::read(&export_path).unwrap();
    assert_eq!(exported_content, b"lifecycle test data");

    store.verify(&export_path).unwrap();

    let project_path = dir.path().join("my-project");
    fs::create_dir_all(&project_path).unwrap();
    store.index().register_project(&project_path).unwrap();

    store.remove(&hash).unwrap();
    assert!(!store.contains(&hash));

    sqlite.advance_generation().unwrap();
    sqlite.advance_generation().unwrap();

    let unreferenced = store.index().get_unreferenced_packages().unwrap();
    assert!(unreferenced.iter().any(|p| p.integrity == hash.hash));
}

#[test]
fn test_store_import_export_cycle() {
    let (store, _sqlite, dir) = make_store();

    let files: [(&str, &[u8]); 3] = [
        ("alpha.txt", b"alpha content"),
        ("beta.txt", b"beta content with more data"),
        ("gamma.txt", b"gamma"),
    ];

    let mut imported: Vec<(IntegrityHash, Vec<u8>)> = Vec::new();
    for (name, content) in &files {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        let hash = store.import_file(&path).unwrap();
        assert!(store.contains(&hash));
        imported.push((hash, content.to_vec()));
    }

    for (hash, _) in &imported {
        let cas_path = hash.cas_path(store.root());
        let verified = store.verify(&cas_path).unwrap();
        assert_eq!(verified.hash, hash.hash);
    }

    for (i, (hash, expected)) in imported.iter().enumerate() {
        let out = dir.path().join(format!("out_{}.bin", i));
        store.export_to(hash, &out).unwrap();
        let actual = fs::read(&out).unwrap();
        assert_eq!(&actual, expected);
    }
}

#[test]
fn test_store_verify_and_prune() {
    let (store, _sqlite, dir) = make_store();

    let src = dir.path().join("original.txt");
    let data = b"original content for verify test";
    fs::write(&src, data).unwrap();
    let hash = store.import_file(&src).unwrap();

    let cas_path = hash.cas_path(store.root());
    fs::write(&cas_path, b"corrupted data").unwrap();

    let result = store.verify(&cas_path).unwrap();
    assert_ne!(result.hash, hash.hash);

    store.remove(&hash).unwrap();
    assert!(!store.contains(&hash));
}

#[test]
fn test_store_empty_file() {
    let (store, _sqlite, dir) = make_store();

    let src = dir.path().join("empty.txt");
    fs::write(&src, b"").unwrap();

    let hash = store.import_file(&src).unwrap();
    assert!(store.contains(&hash));
    assert_eq!(hash.hash.len(), 64);
    assert!(!hash.is_executable);

    let cas_path = hash.cas_path(store.root());
    let content = fs::read(&cas_path).unwrap();
    assert!(content.is_empty());
}

#[test]
fn test_store_executable_file() {
    let (store, _sqlite, dir) = make_store();

    let src = dir.path().join("script.sh");
    let content = b"#!/bin/sh\necho hello";
    fs::write(&src, content).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&src).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&src, perms).unwrap();
    }

    let hash = store.import_file(&src).unwrap();
    assert!(store.contains(&hash));
    assert!(hash.is_executable);
    assert!(hash.filename.ends_with("-exec"));

    let export_path = dir.path().join("exported_script.sh");
    store.export_to(&hash, &export_path).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = fs::metadata(&export_path).unwrap();
        assert!(
            (meta.permissions().mode() & 0o111) != 0,
            "exported file should be executable"
        );
    }

    let exported_content = fs::read(&export_path).unwrap();
    assert_eq!(exported_content, content);
}
