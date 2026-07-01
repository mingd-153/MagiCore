#![cfg(test)]

use std::fs;
use tempfile::tempdir;

use mgpm_linker::linker::{LinkerFactory, LinkerOptions, LinkerStrategy, PackageLinkInfo};
use mgpm_store::{CasContentStore, SqliteStore};

#[test]
fn test_hoisted_linker_creates_node_modules() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("store.db");
    let index = SqliteStore::open(&db_path, false).unwrap();
    let store = CasContentStore::new(dir.path().join("cas"), Box::new(index.clone())).unwrap();

    let src = dir.path().join("test.txt");
    fs::write(&src, b"hello world").unwrap();
    let hash = store.import_file(&src).unwrap();

    let options = LinkerOptions {
        project_root: dir.path().to_path_buf(),
        store_path: store.root().to_path_buf(),
        strategy: LinkerStrategy::Hoisted,
        hoist: true,
        ..Default::default()
    };

    let linker = LinkerFactory::create(options, &store).unwrap();
    assert_eq!(linker.strategy(), LinkerStrategy::Hoisted);

    let pkg = PackageLinkInfo::new(
        "test-pkg".to_string(),
        "1.0.0".to_string(),
        vec![],
        vec![],
        vec![("index.js".to_string(), hash.hash.clone())],
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
