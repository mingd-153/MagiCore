use std::fs;
use std::path::{Path, PathBuf};

use mg_linker::linker::{LinkerFactory, LinkerOptions, LinkerStrategy, PackageLinkInfo};
use mg_store::store::cas::ContentStore;
use mg_store::SqliteStore;
use tempfile::tempdir;

fn create_cas_store() -> (ContentStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let index = SqliteStore::open_in_memory().unwrap();
    let store = ContentStore::new(dir.path().join("cas_store"), Box::new(index)).unwrap();
    (store, dir)
}

fn create_store_file(store_path: &Path, content: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let hash = hex::encode(Sha256::digest(content));
    let shard = &hash[..2];
    let file_path = store_path
        .join("files")
        .join("sha256")
        .join(shard)
        .join(&hash);
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(&file_path, content).unwrap();
    hash
}

fn compute_dep_graph_hash(packages: &[PackageLinkInfo]) -> String {
    let mut hasher = blake3::Hasher::new();
    let mut sorted = packages.to_vec();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    for pkg in &sorted {
        hasher.update(pkg.name.as_bytes());
        hasher.update(b"\0");
        hasher.update(pkg.version.as_bytes());
        hasher.update(b"\0");
        for dep in &pkg.dependencies {
            hasher.update(dep.as_bytes());
            hasher.update(b",");
        }
        hasher.update(b"\0");
    }
    hasher.finalize().to_hex().to_string()
}

#[test]
fn test_hoisted_linker_layout() {
    let (_cas_store, _cas_dir) = create_cas_store();
    let tmp = tempdir().unwrap();
    let project_root = tmp.path().join("project");
    let store_path = tmp.path().join("store");

    let content = b"module.exports = 42;";
    let hash = create_store_file(&store_path, content);

    let pkg = PackageLinkInfo::new(
        "test-pkg".into(),
        "1.0.0".into(),
        vec![],
        vec![],
        vec![],
        vec![("index.js".into(), hash)],
        true,
        vec![],
        content.len() as u64,
        "graph-hash".into(),
    );

    let options = LinkerOptions {
        project_root: project_root.clone(),
        virtual_store_dir: PathBuf::from(".mg"),
        store_path,
        strategy: LinkerStrategy::Hoisted,
        hoist: true,
        hoist_pattern: vec!["*".into()],
        symlinks: true,
        gvs_root: tmp.path().join("gvs").join("v1"),
        ..Default::default()
    };

    let linker = LinkerFactory::create(options, &_cas_store).unwrap();
    let result = linker.link_all(&[pkg], &_cas_store, &project_root).unwrap();

    assert!(
        project_root.join(".mg").exists(),
        ".mg dir should exist"
    );

    let nm = project_root
        .join(".mg")
        .join("node_modules")
        .join("test-pkg");
    assert!(
        nm.exists(),
        "hoisted: node_modules/test-pkg should exist in .mg"
    );
    assert!(
        nm.join("index.js").exists(),
        "hoisted: file should be linked inside .mg/node_modules/test-pkg"
    );

    assert_eq!(result.linked.len(), 1);
    assert_eq!(result.linked[0].name, "test-pkg");
    assert_eq!(result.linked[0].version, "1.0.0");
}

#[test]
fn test_isolated_linker_layout() {
    let (_cas_store, _cas_dir) = create_cas_store();
    let tmp = tempdir().unwrap();
    let project_root = tmp.path().join("project");
    let store_path = tmp.path().join("store");

    let content = b"module.exports = 42;";
    let hash = create_store_file(&store_path, content);

    let pkg = PackageLinkInfo::new(
        "test-pkg".into(),
        "1.0.0".into(),
        vec![],
        vec![],
        vec![],
        vec![("index.js".into(), hash)],
        true,
        vec![],
        content.len() as u64,
        "graph-hash".into(),
    );

    let dep_graph_hash = compute_dep_graph_hash(&[pkg.clone()]);

    let options = LinkerOptions {
        project_root: project_root.clone(),
        virtual_store_dir: PathBuf::from(".mg"),
        store_path,
        strategy: LinkerStrategy::Isolated,
        gvs_root: tmp.path().join("gvs").join("v1"),
        symlinks: true,
        ..Default::default()
    };

    let linker = LinkerFactory::create(options, &_cas_store).unwrap();
    let result = linker.link_all(&[pkg], &_cas_store, &project_root).unwrap();

    let nm_pkg = project_root.join("node_modules").join("test-pkg");
    assert!(nm_pkg.exists());
    assert!(nm_pkg.is_symlink());

    let target = fs::read_link(&nm_pkg).unwrap();
    let target_str = target.to_string_lossy();
    assert!(
        target_str.contains(".mg"),
        "isolated symlink should point into .mg virtual store, got: {target_str}"
    );

    assert_eq!(result.linked.len(), 1);
    assert_eq!(result.linked[0].name, "test-pkg");
    assert_eq!(result.dep_graph_hash, dep_graph_hash);
    let linked_path = &result.linked[0].path;
    assert!(
        linked_path.exists(),
        "linked package path should exist in virtual store"
    );
}

#[test]
fn test_linker_empty_package() {
    let (_cas_store, _cas_dir) = create_cas_store();
    let tmp = tempdir().unwrap();
    let project_root = tmp.path().join("project");
    let store_path = tmp.path().join("store");

    let pkg = PackageLinkInfo::new(
        "empty-pkg".into(),
        "0.0.0".into(),
        vec![],
        vec![],
        vec![],
        vec![],
        true,
        vec![],
        0,
        "empty".into(),
    );

    let options = LinkerOptions {
        project_root: project_root.clone(),
        virtual_store_dir: PathBuf::from(".mg"),
        store_path,
        strategy: LinkerStrategy::Hoisted,
        hoist: true,
        hoist_pattern: vec!["*".into()],
        symlinks: true,
        gvs_root: tmp.path().join("gvs").join("v1"),
        ..Default::default()
    };

    let linker = LinkerFactory::create(options, &_cas_store).unwrap();
    let result = linker.link_all(&[pkg], &_cas_store, &project_root).unwrap();

    assert_eq!(result.linked.len(), 1);
    assert_eq!(result.linked[0].name, "empty-pkg");

    let nm_dir = project_root
        .join(".mg")
        .join("node_modules")
        .join("empty-pkg");
    assert!(
        nm_dir.exists(),
        "empty package dir should exist in .mg/node_modules"
    );
    let file_count = fs::read_dir(&nm_dir).unwrap().count();
    assert_eq!(
        file_count, 0,
        "empty package directory should contain no files"
    );
}

#[test]
fn test_linker_multiple_packages() {
    let (_cas_store, _cas_dir) = create_cas_store();
    let tmp = tempdir().unwrap();
    let project_root = tmp.path().join("project");
    let store_path = tmp.path().join("store");

    let lib_content = b"module.exports = { lib: true };";
    let app_content = b"const lib = require('lib');";
    let hash_lib = create_store_file(&store_path, lib_content);
    let hash_app = create_store_file(&store_path, app_content);

    let lib_pkg = PackageLinkInfo::new(
        "lib".into(),
        "1.0.0".into(),
        vec![],
        vec![],
        vec![],
        vec![("index.js".into(), hash_lib)],
        false,
        vec![],
        lib_content.len() as u64,
        "graph-lib".into(),
    );

    let app_pkg = PackageLinkInfo::new(
        "app".into(),
        "2.0.0".into(),
        vec!["lib".into()],
        vec![],
        vec![],
        vec![("main.js".into(), hash_app)],
        true,
        vec![],
        app_content.len() as u64,
        "graph-app".into(),
    );

    let packages = vec![lib_pkg, app_pkg];

    let options = LinkerOptions {
        project_root: project_root.clone(),
        virtual_store_dir: PathBuf::from(".mg"),
        store_path,
        strategy: LinkerStrategy::Hoisted,
        hoist: true,
        hoist_pattern: vec!["*".into()],
        symlinks: true,
        gvs_root: tmp.path().join("gvs").join("v1"),
        ..Default::default()
    };

    let linker = LinkerFactory::create(options, &_cas_store).unwrap();
    let result = linker
        .link_all(&packages, &_cas_store, &project_root)
        .unwrap();

    assert_eq!(result.linked.len(), 2);

    let mg_nm = project_root.join(".mg").join("node_modules");
    assert!(
        mg_nm.join("app").exists(),
        "app should be in .mg/node_modules"
    );
    assert!(
        mg_nm.join("lib").exists(),
        "lib should be in .mg/node_modules"
    );

    assert!(
        mg_nm.join("app").join("main.js").exists(),
        "app/main.js should exist"
    );
    assert!(
        mg_nm.join("lib").join("index.js").exists(),
        "lib/index.js should exist"
    );

    assert!(
        project_root.join(".mg").exists(),
        ".mg dir should exist"
    );
}

#[test]
fn test_hoisted_vs_isolated_different_structure() {
    let (_cas_store, _cas_dir) = create_cas_store();
    let tmp = tempdir().unwrap();
    let store_path = tmp.path().join("store");

    let content = b"some content";
    let hash = create_store_file(&store_path, content);

    let pkg = PackageLinkInfo::new(
        "compare-pkg".into(),
        "1.0.0".into(),
        vec![],
        vec![],
        vec![],
        vec![("file.js".into(), hash)],
        true,
        vec![],
        content.len() as u64,
        "compare".into(),
    );

    let hoisted_root = tmp.path().join("hoisted");
    let hoisted_opts = LinkerOptions {
        project_root: hoisted_root.clone(),
        virtual_store_dir: PathBuf::from(".mg"),
        store_path: store_path.clone(),
        strategy: LinkerStrategy::Hoisted,
        hoist: true,
        hoist_pattern: vec!["*".into()],
        symlinks: true,
        gvs_root: tmp.path().join("gvs").join("v1"),
        ..Default::default()
    };

    let hoisted_linker = LinkerFactory::create(hoisted_opts, &_cas_store).unwrap();
    hoisted_linker
        .link_all(&[pkg.clone()], &_cas_store, &hoisted_root)
        .unwrap();

    assert!(hoisted_root.join(".mg").exists());

    let hoisted_nm = hoisted_root
        .join(".mg")
        .join("node_modules")
        .join("compare-pkg");
    assert!(
        hoisted_nm.exists(),
        "hoisted: package should be in .mg/node_modules"
    );
    assert!(
        hoisted_nm.join("file.js").exists(),
        "hoisted: file should be linked"
    );
    assert!(
        hoisted_nm.is_symlink(),
        "hoisted should symlink into virtual store"
    );

    let isolated_root = tmp.path().join("isolated");

    let isolated_opts = LinkerOptions {
        project_root: isolated_root.clone(),
        virtual_store_dir: PathBuf::from(".mg"),
        store_path: store_path.clone(),
        strategy: LinkerStrategy::Isolated,
        gvs_root: tmp.path().join("gvs").join("v1"),
        symlinks: true,
        ..Default::default()
    };

    let isolated_linker = LinkerFactory::create(isolated_opts, &_cas_store).unwrap();
    isolated_linker
        .link_all(&[pkg], &_cas_store, &isolated_root)
        .unwrap();

    let isolated_nm = isolated_root.join("node_modules").join("compare-pkg");
    assert!(isolated_nm.exists());
    assert!(isolated_nm.is_symlink());

    let isolated_target = fs::read_link(&isolated_nm).unwrap();
    let isolated_target_str = isolated_target.to_string_lossy();
    assert!(
        isolated_target_str.contains(".mg"),
        "isolated: symlink target should be inside .mg, got: {isolated_target_str}"
    );

    let hoisted_mg = hoisted_root.join(".mg").join("node_modules");
    let isolated_mg = isolated_root.join("node_modules").join(".mg");

    assert!(
        hoisted_mg.exists(),
        "hoisted: node_modules should live inside .mg"
    );
    assert!(
        isolated_mg.exists(),
        "isolated: .mg should live inside node_modules"
    );
}
