#![cfg(test)]

use std::fs;
use tempfile::tempdir;

use mgpm_linker::linker::{LinkerFactory, LinkerOptions, LinkerStrategy, PackageLinkInfo};
use mgpm_store::{CasContentStore, SqliteStore};

#[test]
fn test_hoisted_linker_creates_node_modules() {
    use sha2::{Digest, Sha256};
    let dir = tempdir().unwrap();
    let store_path = dir.path().join("store");

    // Use the old flat store format (files/sha256/{shard}/{hash})
    let content = b"hello world";
    let hash = hex::encode(Sha256::digest(content));
    let shard = &hash[..2];
    let file_path = store_path.join("files").join("sha256").join(shard).join(&hash);
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(&file_path, content).unwrap();

    let options = LinkerOptions {
        project_root: dir.path().to_path_buf(),
        store_path,
        strategy: LinkerStrategy::Hoisted,
        hoist: true,
        ..Default::default()
    };

    let index = mgpm_store::SqliteStore::open(&dir.path().join("store.db"), false).unwrap();
    let store = CasContentStore::new(dir.path().join("cas"), Box::new(index)).unwrap();
    let linker = LinkerFactory::create(options, &store).unwrap();
    assert_eq!(linker.strategy(), LinkerStrategy::Hoisted);

    let pkg = PackageLinkInfo::new(
        "test-pkg".to_string(),
        "1.0.0".to_string(),
        vec![],
        vec![],
        vec![("index.js".to_string(), hash)],
        true,
        vec![],
        1024,
        "abc".to_string(),
    );

    linker.link_all(&[pkg], &store, dir.path()).unwrap();
    assert!(dir.path().join(".mgpm").exists());
    assert!(dir.path().join(".mgpm/node_modules").exists());
}

#[test]
fn test_isolated_linker_creates_node_modules() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("store.db");
    let index = SqliteStore::open(&db_path, false).unwrap();
    let store = CasContentStore::new(dir.path().join("cas"), Box::new(index.clone())).unwrap();

    let src = dir.path().join("test.txt");
    fs::write(&src, b"hello world").unwrap();
    store.import_file(&src).unwrap();

    let options = LinkerOptions {
        project_root: dir.path().to_path_buf(),
        store_path: store.root().to_path_buf(),
        strategy: LinkerStrategy::Isolated,
        gvs_root: dir.path().join("gvs"),
        ..Default::default()
    };

    let linker = LinkerFactory::create(options, &store).unwrap();
    assert_eq!(linker.strategy(), LinkerStrategy::Isolated);

    let pkg = PackageLinkInfo::new(
        "test-pkg".to_string(),
        "1.0.0".to_string(),
        vec![],
        vec![],
        vec![],
        true,
        vec![],
        1024,
        "abc".to_string(),
    );

    linker.link_all(&[pkg], &store, dir.path()).unwrap();
    assert!(dir.path().join("node_modules").exists());
}

#[test]
fn test_linker_strategy_default() {
    let options = LinkerOptions::default();
    assert_eq!(options.strategy, LinkerStrategy::Hoisted);
}
