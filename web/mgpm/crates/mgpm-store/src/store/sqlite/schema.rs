use rusqlite::Connection;

use super::super::index::{PackageInfo, StoreError};

// Column order for packages table — must match row_to_package and all SELECT queries
pub const PACKAGE_COLUMNS: &[&str] = &[
    "name",
    "version",
    "integrity",
    "shard",
    "filename",
    "is_executable",
    "manifest_json",
    "size_bytes",
    "compressed_size_bytes",
    "created_at",
    "metadata",
];

const PACKAGE_COLUMNS_SQL: &str = "name, version, integrity, shard, filename, is_executable, manifest_json, size_bytes, compressed_size_bytes, created_at, metadata";

pub fn create_tables(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS packages (
            integrity TEXT NOT NULL,
            name TEXT NOT NULL,
            version TEXT NOT NULL,
            shard TEXT NOT NULL,
            filename TEXT NOT NULL,
            is_executable INTEGER DEFAULT 0,
            manifest_json TEXT,
            size_bytes INTEGER,
            compressed_size_bytes INTEGER,
            metadata TEXT DEFAULT '{}',
            created_at INTEGER DEFAULT (unixepoch()),
            generation INTEGER DEFAULT 0,
            PRIMARY KEY (integrity)
        ) WITHOUT ROWID;

        CREATE INDEX IF NOT EXISTS idx_packages_name
            ON packages(name, version);

        CREATE INDEX IF NOT EXISTS idx_packages_created
            ON packages(created_at);

        CREATE TABLE IF NOT EXISTS projects (
            project_hash TEXT PRIMARY KEY,
            path TEXT NOT NULL,
            metadata TEXT DEFAULT '{}',
            last_used INTEGER DEFAULT (unixepoch())
        );

        CREATE TABLE IF NOT EXISTS dependencies (
            project_hash TEXT NOT NULL,
            package_integrity TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'prod',
            constraint_spec TEXT,
            metadata TEXT DEFAULT '{}',
            PRIMARY KEY (project_hash, package_integrity, kind)
        );

        CREATE INDEX IF NOT EXISTS idx_dep_package
            ON dependencies(package_integrity);

        CREATE TABLE IF NOT EXISTS integrity_cache (
            file_path TEXT PRIMARY KEY,
            integrity TEXT NOT NULL,
            algorithm TEXT NOT NULL DEFAULT 'sha256',
            mtime INTEGER NOT NULL
        ) WITHOUT ROWID;

        CREATE TABLE IF NOT EXISTS kv_store (
            key TEXT PRIMARY KEY,
            value BLOB NOT NULL
        ) WITHOUT ROWID;

        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS gc_state (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            generation INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER DEFAULT (unixepoch())
        );",
    )
    .or_else(|_| {
        conn.execute("CREATE TABLE IF NOT EXISTS packages (integrity TEXT PRIMARY KEY, name TEXT NOT NULL, version TEXT NOT NULL, shard TEXT NOT NULL, filename TEXT NOT NULL, is_executable INTEGER DEFAULT 0, manifest_json TEXT, size_bytes INTEGER, compressed_size_bytes INTEGER, metadata TEXT DEFAULT '{}', created_at INTEGER DEFAULT (unixepoch()), generation INTEGER DEFAULT 0) WITHOUT ROWID", []).ok();
        conn.execute("CREATE INDEX IF NOT EXISTS idx_packages_name ON packages(name, version)", []).ok();
        conn.execute("CREATE INDEX IF NOT EXISTS idx_packages_created ON packages(created_at)", []).ok();
        conn.execute("CREATE TABLE IF NOT EXISTS projects (project_hash TEXT PRIMARY KEY, path TEXT NOT NULL, metadata TEXT DEFAULT '{}', last_used INTEGER DEFAULT (unixepoch()))", []).ok();
        conn.execute("CREATE TABLE IF NOT EXISTS dependencies (project_hash TEXT NOT NULL, package_integrity TEXT NOT NULL, kind TEXT NOT NULL DEFAULT 'prod', constraint_spec TEXT, metadata TEXT DEFAULT '{}', PRIMARY KEY (project_hash, package_integrity, kind))", []).ok();
        conn.execute("CREATE INDEX IF NOT EXISTS idx_dep_package ON dependencies(package_integrity)", []).ok();
        conn.execute("CREATE TABLE IF NOT EXISTS integrity_cache (file_path TEXT PRIMARY KEY, integrity TEXT NOT NULL, algorithm TEXT NOT NULL DEFAULT 'sha256', mtime INTEGER NOT NULL) WITHOUT ROWID", []).ok();
        conn.execute("CREATE TABLE IF NOT EXISTS kv_store (key TEXT PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID", []).ok();
        conn.execute("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT (datetime('now')))", []).ok();
        conn.execute("CREATE TABLE IF NOT EXISTS gc_state (id INTEGER PRIMARY KEY AUTOINCREMENT, generation INTEGER NOT NULL DEFAULT 0, created_at INTEGER DEFAULT (unixepoch()))", []).ok();
        Ok::<(), rusqlite::Error>(())
    })?;
    Ok(())
}

pub fn migrate_schema(conn: &Connection) -> Result<(), StoreError> {
    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let migrations: &[(&str, &str)] = &[
        ("1", "INSERT INTO schema_version (version) VALUES (1)"),
    ];

    for (ver, sql) in migrations {
        let ver_num: i64 = ver.parse().unwrap_or(0);
        if current_version < ver_num {
            conn.execute(sql, [])?;
        }
    }
    Ok(())
}

pub fn row_to_package(row: &rusqlite::Row) -> rusqlite::Result<PackageInfo> {
    Ok(PackageInfo {
        name: row.get("name")?,
        version: row.get("version")?,
        integrity: row.get("integrity")?,
        shard: row.get("shard")?,
        filename: row.get("filename")?,
        is_executable: row.get::<_, i32>("is_executable")? != 0,
        manifest_json: row.get("manifest_json")?,
        metadata: row.get("metadata")?,
        size_bytes: row.get::<_, i64>("size_bytes")? as u64,
        compressed_size_bytes: row.get::<_, i64>("compressed_size_bytes")? as u64,
        created_at: row.get::<_, i64>("created_at")? as u64,
    })
}
