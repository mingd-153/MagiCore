#![cfg(test)]

use std::fs;

use mgpm_store::{GlobalVirtualStore, SqliteStore, StoreIndex};
use tempfile::tempdir;

const HASH_A: &str = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
const HASH_B: &str = "b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3";
const HASH_C: &str = "c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4";

fn setup() -> (GlobalVirtualStore, SqliteStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("store.db");
    let index = SqliteStore::open(&db_path, false).unwrap();
    let gvs_root = dir.path().join("gvs");
    let gvs = GlobalVirtualStore::new(gvs_root);
    gvs.ensure_dirs().unwrap();
    (gvs, index, dir)
}

fn create_project(dir: &tempfile::TempDir, name: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn test_gvs_register_and_list() {
    let (gvs, index, dir) = setup();
    let proj_path = create_project(&dir, "proj-a");

    gvs.register(&proj_path, HASH_A, &index).unwrap();

    let projects = gvs.list_projects(&index).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(
        projects[0].path,
        fs::canonicalize(&proj_path).unwrap().to_string_lossy()
    );
}

#[test]
fn test_gvs_register_unregister() {
    let (gvs, index, dir) = setup();
    let proj_path = create_project(&dir, "proj-a");

    gvs.register(&proj_path, HASH_A, &index).unwrap();
    gvs.unregister(&proj_path, &index).unwrap();

    let projects = gvs.list_projects(&index).unwrap();
    assert_eq!(projects.len(), 0);
}

#[test]
fn test_gvs_gc_removes_unused() {
    let (gvs, index, dir) = setup();
    let proj_path = create_project(&dir, "proj-a");

    gvs.register(&proj_path, HASH_A, &index).unwrap();

    // Remove from index directly, leaving GVS directory orphaned
    index.unregister_project(&proj_path).unwrap();

    let report = gvs.gc(&index).unwrap();
    assert_eq!(report.removed_dirs.len(), 1);

    // Second GC should find nothing
    let report2 = gvs.gc(&index).unwrap();
    assert!(report2.removed_dirs.is_empty());
}

#[test]
fn test_gvs_status() {
    let (gvs, index, dir) = setup();
    let proj_path = create_project(&dir, "proj-a");

    gvs.register(&proj_path, HASH_A, &index).unwrap();

    let status = gvs.status(&index).unwrap();
    assert_eq!(status.total_projects, 1);
    assert_eq!(status.gvs_root, gvs.root());
}

#[test]
fn test_gvs_multiple_projects() {
    let (gvs, index, dir) = setup();

    let proj_a = create_project(&dir, "proj-a");
    let proj_b = create_project(&dir, "proj-b");
    let proj_c = create_project(&dir, "proj-c");

    gvs.register(&proj_a, HASH_A, &index).unwrap();
    gvs.register(&proj_b, HASH_B, &index).unwrap();
    gvs.register(&proj_c, HASH_C, &index).unwrap();

    let projects = gvs.list_projects(&index).unwrap();
    assert_eq!(projects.len(), 3);
}

#[test]
fn test_gvs_dep_graph_hash_validation() {
    let (gvs, index, dir) = setup();
    let proj_path = create_project(&dir, "proj-a");

    // Too short
    let result = gvs.register(&proj_path, "abc123", &index);
    assert!(result.is_err());

    // Non-hex characters
    let result = gvs.register(
        &proj_path,
        "g1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
        &index,
    );
    assert!(result.is_err());

    // Valid hash should succeed
    let result = gvs.register(&proj_path, HASH_A, &index);
    assert!(result.is_ok());
}
