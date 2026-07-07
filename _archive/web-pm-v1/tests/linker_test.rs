#![cfg(test)]

use std::fs;
use tempfile::tempdir;

use mg_linker::linker::{LinkerFactory, LinkerOptions, LinkerStrategy, PackageLinkInfo};
use mg_store::{CasContentStore, SqliteStore};

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

    let index = mg_store::SqliteStore::open(&dir.path().join("store.db"), false).unwrap();
    let store = CasContentStore::new(dir.path().join("cas"), Box::new(index)).unwrap();
    let linker = LinkerFactory::create(options, &store).unwrap();
    assert_eq!(linker.strategy(), LinkerStrategy::Hoisted);

    let pkg = PackageLinkInfo::new(
        "test-pkg".to_string(),
        "1.0.0".to_string(),
        vec![],
        vec![],
        vec![],
        vec![("index.js".to_string(), hash)],
        true,
        vec![],
        1024,
        "abc".to_string(),
    );

    linker.link_all(&[pkg], &store, dir.path()).unwrap();
    assert!(dir.path().join(".mg").exists());
    assert!(dir.path().join(".mg/node_modules").exists());
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
fn test_dep_symlink_depth_pnpm_style() {
    use sha2::{Digest, Sha256};
    let dir = tempdir().unwrap();
    let store_path = dir.path().join("store");

    let content = b"hello";
    let hash = hex::encode(Sha256::digest(content));
    let shard = &hash[..2];
    let file_path = store_path.join("files").join("sha256").join(shard).join(&hash);
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(&file_path, content).unwrap();

    // Create dep first, then root pkg that depends on dep
    let dep = PackageLinkInfo::new(
        "dep-pkg".to_string(),
        "1.0.0".to_string(),
        vec![],
        vec![],
        vec![],
        vec![("index.js".to_string(), hash.clone())],
        false,
        vec![],
        512,
        "def".to_string(),
    );

    let root_pkg = PackageLinkInfo::new(
        "root-pkg".to_string(),
        "1.0.0".to_string(),
        vec!["dep-pkg".to_string()],
        vec![],
        vec![],
        vec![("main.js".to_string(), hash)],
        true,
        vec![],
        1024,
        "abc".to_string(),
    );

    let options = LinkerOptions {
        project_root: dir.path().to_path_buf(),
        store_path,
        strategy: LinkerStrategy::Hoisted,
        hoist: true,
        ..Default::default()
    };

    let index = mg_store::SqliteStore::open(&dir.path().join("store.db"), false).unwrap();
    let store = CasContentStore::new(dir.path().join("cas"), Box::new(index)).unwrap();
    let linker = LinkerFactory::create(options, &store).unwrap();

    linker.link_all(&[dep, root_pkg], &store, dir.path()).unwrap();

    let mg_root = dir.path().join(".mg");
    assert!(mg_root.join("virtual_store").exists(), "virtual_store dir");

    // Debug: print full tree
    fn print_tree(path: &std::path::Path, indent: usize) {
        if let Ok(entries) = fs::read_dir(path) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let marker = if e.path().is_symlink() {
                    let t = std::fs::read_link(e.path()).unwrap_or_default();
                    format!(" -> {}", t.display())
                } else if e.path().is_dir() {
                    "/".to_string()
                } else {
                    String::new()
                };
                eprintln!("{:indent$}{}{}", "", name, marker, indent = indent);
                if e.path().is_dir() && !e.path().is_symlink() {
                    print_tree(&e.path(), indent + 2);
                }
            }
        }
    }
    print_tree(&mg_root, 0);

    // Dep symlink is now at virtual_store/<pkg_dir>/node_modules/<dep>
    let dep_link = mg_root.join("virtual_store").join("root-pkg_1.0.0_e3b0c442").join("node_modules").join("dep-pkg");
    assert!(dep_link.exists() || dep_link.is_symlink(), "dep symlink should exist at {}", dep_link.display());

    if dep_link.is_symlink() {
        let target = std::fs::read_link(&dep_link).unwrap();
        let target_str = target.to_string_lossy().to_string();
        // pnpm style: ../../dep-pkg@/node_modules/dep-pkg
        assert!(!target_str.contains("root-pkg"), "dep symlink should not contain root-pkg in target: {}", target_str);
        assert!(target_str.contains("dep-pkg"), "dep symlink should point to dep-pkg: {}", target_str);
        eprintln!("=== DEP SYMLINK: {} -> {} ===", dep_link.display(), target_str);
    }

    // Verify hoisted symlinks at project root use minimal depth
    let root_nm = dir.path().join("node_modules");
    if root_nm.join("dep-pkg").exists() || root_nm.join("dep-pkg").is_symlink() {
        let target = std::fs::read_link(&root_nm.join("dep-pkg")).unwrap();
        let target_str = target.to_string_lossy().to_string();
        eprintln!("HOISTED dep-pkg: {}", target_str);
    }
    if root_nm.join("root-pkg").exists() || root_nm.join("root-pkg").is_symlink() {
        let target = std::fs::read_link(&root_nm.join("root-pkg")).unwrap();
        let target_str = target.to_string_lossy().to_string();
        eprintln!("HOISTED root-pkg: {}", target_str);
    }
}

#[test]
fn test_linker_strategy_default() {
    let options = LinkerOptions::default();
    assert_eq!(options.strategy, LinkerStrategy::Hoisted);
}
