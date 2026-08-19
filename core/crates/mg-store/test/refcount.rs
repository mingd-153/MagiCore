//! Integration tests for CAS blob refcount (slices 4-5 of T1 backing-store).
//! test riêng tại test/ (RULE §5).

use mg_store::Database;
use std::path::PathBuf;

struct TestDb {
    _dir: tempfile::TempDir,
    db: Database,
}

impl TestDb {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("store.db")).unwrap();
        Self { _dir: dir, db }
    }
}

const PROJ_A: &str = "/proj/a";
const PROJ_B: &str = "/proj/b";

#[test]
fn cas_claim_is_idempotent_per_project() {
    let db = TestDb::new();
    let db = &db.db;
    db.cas_claim(PROJ_A, "blob-1").unwrap();
    db.cas_claim(PROJ_A, "blob-1").unwrap();
    db.cas_claim(PROJ_B, "blob-1").unwrap();

    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(live, vec!["blob-1".to_string()]);
}

#[test]
fn cas_release_removes_single_project_claim() {
    let db = TestDb::new();
    let db = &db.db;
    db.cas_claim(PROJ_A, "blob-1").unwrap();
    db.cas_claim(PROJ_B, "blob-1").unwrap();
    db.cas_release(PROJ_A, "blob-1").unwrap();

    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(live, vec!["blob-1".to_string()]);

    db.cas_release(PROJ_B, "blob-1").unwrap();
    assert!(db.list_cas_live_refs().unwrap().is_empty());
}

#[test]
fn clear_all_cas_refs_resets_project_claims() {
    let db = TestDb::new();
    let db = &db.db;
    db.cas_claim(PROJ_A, "blob-1").unwrap();
    db.cas_claim(PROJ_A, "blob-2").unwrap();
    db.cas_claim(PROJ_B, "blob-1").unwrap();
    db.clear_all_cas_refs(PROJ_A).unwrap();

    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(live, vec!["blob-1".to_string()]);
}

#[test]
fn cas_refs_isolated_from_package_refs_table() {
    let db = TestDb::new();
    let db = &db.db;
    let root: PathBuf = PROJ_A.into();
    let pkg = mg_types::PackageId::parse("react@18.2.0").unwrap();

    db.set_ref(root.to_str().unwrap(), &pkg).unwrap();
    db.cas_claim(PROJ_A, "blob-a").unwrap();
    db.clear_ref(root.to_str().unwrap(), &pkg).unwrap();
    assert_eq!(db.list_cas_live_refs().unwrap(), vec!["blob-a".to_string()]);

    db.cas_release(PROJ_A, "blob-a").unwrap();
    assert!(db.list_cas_live_refs().unwrap().is_empty());
}
