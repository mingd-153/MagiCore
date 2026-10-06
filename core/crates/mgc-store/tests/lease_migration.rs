#![allow(clippy::unwrap_used)]
//! Lease-column migration regression (Gate 11-B P0-3): the ALTER TABLE
//! migration for the crash-recovery lease columns must (a) be idempotent
//! across re-opens, (b) re-add the columns after they are dropped
//! (fail-closed revalidation), and (c) keep tolerating only SQLite's
//! "duplicate column name" race — the exact substring the migration
//! classifies on.
//! (Test hồi quy migration cột lease (P0-3): ALTER TABLE migration cột
//! lease phải (a) idempotent qua các lần mở lại, (b) thêm lại cột sau khi
//! bị drop (revalidate fail-closed), và (c) chỉ dung thứ race "duplicate
//! column name" của SQLite — đúng substring migration phân loại.)

use mgc_store::Database;

/// Read `column` presence on `cas_generations` through a raw connection.
/// (Đọc sự hiện diện của `column` trên cas_generations qua connection thô.)
fn has_column(db_path: &std::path::Path, column: &str) -> bool {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    let mut stmt = conn.prepare("PRAGMA table_info(cas_generations)").unwrap();
    let rows = stmt.query_map([], |row| row.get::<_, String>(1)).unwrap();
    for name in rows {
        if name.unwrap() == column {
            return true;
        }
    }
    false
}

#[test]
fn open_twice_on_clean_db_is_idempotent_with_lease_columns() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("store.db");

    let _db = Database::open(&db_path).unwrap();
    // A second open must stay Ok (idempotent — the duplicate-column path
    // is tolerated by design).
    // (Mở lần hai phải vẫn Ok (idempotent — đường duplicate-column được
    // dung thứ theo thiết kế).)
    let _db2 = Database::open(&db_path).unwrap();

    assert!(has_column(&db_path, "lease_pid"));
    assert!(has_column(&db_path, "lease_started_at"));
}

#[test]
fn reopen_after_drop_column_restores_lease_schema() {
    // Simulate a store whose lease columns were removed (hand-edited or a
    // partial migration): reopening must RE-ADD them — never hand back a
    // store missing its lease schema (fail-closed revalidation).
    // (Mô phỏng store bị xóa cột lease (sửa tay hoặc migration dở): mở
    // lại phải THÊM LẠI — không bao giờ trả store thiếu schema lease
    // (revalidate fail-closed).)
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("store.db");

    // First open creates the full schema (including lease columns).
    // (Mở lần đầu tạo trọn schema (gồm cột lease).)
    drop(Database::open(&db_path).unwrap());

    // SQLite ≥ 3.35 supports ALTER TABLE DROP COLUMN.
    // (SQLite ≥ 3.35 hỗ trợ ALTER TABLE DROP COLUMN.)
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "ALTER TABLE cas_generations DROP COLUMN lease_pid;
             ALTER TABLE cas_generations DROP COLUMN lease_started_at;",
        )
        .unwrap();
    }
    assert!(
        !has_column(&db_path, "lease_pid"),
        "precondition: column dropped"
    );
    assert!(
        !has_column(&db_path, "lease_started_at"),
        "precondition: column dropped"
    );

    // Reopen: migration re-adds both columns and passes revalidation.
    // (Mở lại: migration thêm lại cả hai cột và qua revalidation.)
    drop(Database::open(&db_path).unwrap());
    assert!(has_column(&db_path, "lease_pid"));
    assert!(has_column(&db_path, "lease_started_at"));
}

#[test]
fn sqlite_duplicate_column_error_string_is_stable() {
    // Pin the EXACT substring the migration classifies on. If SQLite ever
    // changes this message, add_lease_column would stop tolerating the
    // concurrent-migration race and this test flags it.
    // (Ghim ĐÚNG substring migration phân loại. Nếu SQLite đổi message
    // này, add_lease_column ngừng dung thứ race migration song song và
    // test này cảnh báo.)
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE t (a INTEGER);").unwrap();
    conn.execute("ALTER TABLE t ADD COLUMN b INTEGER", [])
        .unwrap();
    let err = conn
        .execute("ALTER TABLE t ADD COLUMN b INTEGER", [])
        .unwrap_err();
    assert!(
        err.to_string().contains("duplicate column name"),
        "SQLite duplicate-column message changed: {err}"
    );
}
