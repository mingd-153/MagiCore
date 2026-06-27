use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lru::LruCache;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use super::index::{PackageInfo, StoreError, StoreIndex};

pub struct SqliteStore {
    conn: Mutex<Connection>,
    cache: Mutex<LruCache<String, PackageInfo>>,
    path: PathBuf,
    readonly: bool,
}

impl SqliteStore {
    pub fn open(path: &Path, readonly: bool) -> Result<Self, StoreError> {
        let conn = if readonly {
            Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?
        } else {
            Connection::open(path)?
        };

        Self::apply_pragmas(&conn)?;

        if !readonly {
            Self::create_tables(&conn)?;
        }

        Ok(Self {
            conn: Mutex::new(conn),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(1000).unwrap())),
            path: path.to_path_buf(),
            readonly,
        })
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        Self::apply_pragmas(&conn)?;

        Self::create_tables(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(1000).unwrap())),
            path: PathBuf::from(":memory:"),
            readonly: false,
        })
    }

    fn apply_pragmas(conn: &Connection) -> Result<(), StoreError> {
        let sql = [
            ("journal_mode", "WAL"),
            ("synchronous", "NORMAL"),
            ("mmap_size", "536870912"),
            ("cache_size", "-32000"),
            ("temp_store", "MEMORY"),
            ("wal_autocheckpoint", "10000"),
            ("busy_timeout", "5000"),
        ];
        for (name, value) in sql {
            conn.pragma_update(None, name, value).ok();
        }
        Ok(())
    }

    fn create_tables(conn: &Connection) -> Result<(), StoreError> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS packages (
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                integrity TEXT NOT NULL,
                shard TEXT NOT NULL,
                filename TEXT NOT NULL,
                is_executable INTEGER DEFAULT 0,
                manifest_json TEXT,
                size_bytes INTEGER,
                compressed_size_bytes INTEGER,
                created_at INTEGER DEFAULT (unixepoch()),
                PRIMARY KEY (integrity)
            ) WITHOUT ROWID",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_packages_name
                ON packages(name, version)",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS projects (
                project_hash TEXT PRIMARY KEY,
                path TEXT NOT NULL,
                last_used INTEGER DEFAULT (unixepoch())
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS integrity_cache (
                file_path TEXT PRIMARY KEY,
                integrity TEXT NOT NULL,
                mtime INTEGER NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_readonly(&self) -> bool {
        self.readonly
    }

    pub fn vacuum(&self) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute("VACUUM", [])?;
        Ok(())
    }

    fn row_to_package(row: &rusqlite::Row) -> rusqlite::Result<PackageInfo> {
        Ok(PackageInfo {
            name: row.get(0)?,
            version: row.get(1)?,
            integrity: row.get(2)?,
            shard: row.get(3)?,
            filename: row.get(4)?,
            is_executable: row.get::<_, i32>(5)? != 0,
            manifest_json: row.get(6)?,
            size_bytes: row.get::<_, i64>(7)? as u64,
            compressed_size_bytes: row.get::<_, i64>(8)? as u64,
            created_at: row.get::<_, i64>(9)? as u64,
        })
    }
}

impl StoreIndex for SqliteStore {
    fn add_package(&self, info: &PackageInfo) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO packages
             (name, version, integrity, shard, filename, is_executable,
              manifest_json, size_bytes, compressed_size_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                info.name,
                info.version,
                info.integrity,
                info.shard,
                info.filename,
                info.is_executable as i32,
                info.manifest_json,
                info.size_bytes as i64,
                info.compressed_size_bytes as i64,
            ],
        )?;

        let mut cache = self.cache.lock().unwrap();
        cache.put(info.integrity.clone(), info.clone());

        Ok(())
    }

    fn get_package(&self, name: &str, version: &str) -> Result<Option<PackageInfo>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name, version, integrity, shard, filename, is_executable,
                    manifest_json, size_bytes, compressed_size_bytes, created_at
             FROM packages WHERE name = ?1 AND version = ?2",
        )?;

        let result = stmt.query_row(params![name, version], Self::row_to_package);

        match result {
            Ok(info) => {
                let mut cache = self.cache.lock().unwrap();
                cache.put(info.integrity.clone(), info.clone());
                Ok(Some(info))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::from(e)),
        }
    }

    fn get_by_integrity(&self, hash: &str) -> Result<Option<PackageInfo>, StoreError> {
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(info) = cache.get(hash) {
                return Ok(Some(info.clone()));
            }
        }

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name, version, integrity, shard, filename, is_executable,
                    manifest_json, size_bytes, compressed_size_bytes, created_at
             FROM packages WHERE integrity = ?1",
        )?;

        let result = stmt.query_row(params![hash], Self::row_to_package);

        match result {
            Ok(info) => {
                let mut cache = self.cache.lock().unwrap();
                cache.put(hash.to_string(), info.clone());
                Ok(Some(info))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::from(e)),
        }
    }

    fn package_exists(&self, hash: &str) -> Result<bool, StoreError> {
        {
            let cache = self.cache.lock().unwrap();
            if cache.contains(hash) {
                return Ok(true);
            }
        }

        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM packages WHERE integrity = ?1",
            params![hash],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    fn delete_package(&self, hash: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM packages WHERE integrity = ?1",
            params![hash],
        )?;

        let mut cache = self.cache.lock().unwrap();
        cache.pop(hash);

        Ok(())
    }

    fn register_project(&self, path: &Path) -> Result<(), StoreError> {
        let path_str = path.to_string_lossy();
        let hash = hex::encode(Sha256::digest(path_str.as_bytes()));

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO projects (project_hash, path, last_used)
             VALUES (?1, ?2, unixepoch())",
            params![hash, path_str.to_string()],
        )?;
        Ok(())
    }

    fn unregister_project(&self, path: &Path) -> Result<(), StoreError> {
        let path_str = path.to_string_lossy();

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM projects WHERE path = ?1",
            params![path_str.to_string()],
        )?;
        Ok(())
    }

    fn get_unreferenced_packages(&self) -> Result<Vec<PackageInfo>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT p.name, p.version, p.integrity, p.shard, p.filename,
                    p.is_executable, p.manifest_json, p.size_bytes,
                    p.compressed_size_bytes, p.created_at
             FROM packages p
             WHERE NOT EXISTS (
                 SELECT 1 FROM projects pr
                 JOIN package_project_usage ppu ON ppu.project_hash = pr.project_hash
                 WHERE ppu.package_hash = p.integrity
             )",
        )?;

        let packages = stmt
            .query_map([], Self::row_to_package)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(packages)
    }

    fn update_integrity_cache(&self, file_path: &Path, hash: &str) -> Result<(), StoreError> {
        let path_str = file_path.to_string_lossy();
        let mtime = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO integrity_cache (file_path, integrity, mtime)
             VALUES (?1, ?2, ?3)",
            params![path_str.to_string(), hash, mtime],
        )?;
        Ok(())
    }

    fn get_cached_integrity(&self, file_path: &Path) -> Result<Option<String>, StoreError> {
        let path_str = file_path.to_string_lossy();

        let conn = self.conn.lock().unwrap();
        let result: Result<String, _> = conn.query_row(
            "SELECT integrity FROM integrity_cache WHERE file_path = ?1",
            params![path_str.to_string()],
            |row| row.get(0),
        );

        match result {
            Ok(hash) => Ok(Some(hash)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::from(e)),
        }
    }

    fn begin_transaction(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("BEGIN TRANSACTION", [])?;
        Ok(())
    }

    fn commit(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("COMMIT", [])?;
        Ok(())
    }

    fn rollback(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("ROLLBACK", [])?;
        Ok(())
    }

    fn package_count(&self) -> Result<u64, StoreError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM packages",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    fn project_count(&self) -> Result<u64, StoreError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM projects",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    fn total_size(&self) -> Result<u64, StoreError> {
        let conn = self.conn.lock().unwrap();
        let size: i64 = conn.query_row(
            "SELECT COALESCE(SUM(size_bytes), 0) FROM packages",
            [],
            |row| row.get(0),
        )?;
        Ok(size as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_store() -> (SqliteStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("store.db");
        let store = SqliteStore::open(&path, false).unwrap();
        (store, dir)
    }

    fn test_package(name: &str, version: &str, integrity: &str) -> PackageInfo {
        PackageInfo {
            name: name.to_string(),
            version: version.to_string(),
            integrity: integrity.to_string(),
            shard: format!("{}/{}", &integrity[..2], integrity),
            filename: format!("{}-{}.tgz", name, version),
            is_executable: false,
            manifest_json: None,
            size_bytes: 1024,
            compressed_size_bytes: 512,
            created_at: 0,
        }
    }

    #[test]
    fn test_open_and_create() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = SqliteStore::open(&path, false).unwrap();
        assert!(!store.is_readonly());
        assert_eq!(store.package_count().unwrap(), 0);
    }

    #[test]
    fn test_open_readonly() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_ro.db");
        SqliteStore::open(&path, false).unwrap();
        let store = SqliteStore::open(&path, true).unwrap();
        assert!(store.is_readonly());
    }

    #[test]
    fn test_open_in_memory() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(!store.is_readonly());
        assert_eq!(store.package_count().unwrap(), 0);
    }

    #[test]
    fn test_add_and_get_package() {
        let (store, _dir) = create_test_store();
        let pkg = test_package("test-pkg", "1.0.0", "abc123");
        store.add_package(&pkg).unwrap();
        let retrieved = store.get_package("test-pkg", "1.0.0").unwrap().unwrap();
        assert_eq!(retrieved.name, "test-pkg");
        assert_eq!(retrieved.version, "1.0.0");
        assert_eq!(retrieved.integrity, "abc123");
    }

    #[test]
    fn test_get_nonexistent_package() {
        let (store, _dir) = create_test_store();
        let result = store.get_package("nonexistent", "0.0.0").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_by_integrity() {
        let (store, _dir) = create_test_store();
        let pkg = test_package("integrity-pkg", "2.0.0", "def456");
        store.add_package(&pkg).unwrap();
        let retrieved = store.get_by_integrity("def456").unwrap().unwrap();
        assert_eq!(retrieved.name, "integrity-pkg");
    }

    #[test]
    fn test_package_exists() {
        let (store, _dir) = create_test_store();
        let pkg = test_package("exists-pkg", "1.0.0", "exists123");
        store.add_package(&pkg).unwrap();
        assert!(store.package_exists("exists123").unwrap());
        assert!(!store.package_exists("nope").unwrap());
    }

    #[test]
    fn test_delete_package() {
        let (store, _dir) = create_test_store();
        let pkg = test_package("delete-pkg", "1.0.0", "del123");
        store.add_package(&pkg).unwrap();
        assert!(store.package_exists("del123").unwrap());
        store.delete_package("del123").unwrap();
        assert!(!store.package_exists("del123").unwrap());
    }

    #[test]
    fn test_duplicate_integrity_replaces() {
        let (store, _dir) = create_test_store();
        let pkg1 = test_package("original", "1.0.0", "dup123");
        let pkg2 = test_package("replacement", "2.0.0", "dup123");
        store.add_package(&pkg1).unwrap();
        store.add_package(&pkg2).unwrap();
        assert_eq!(store.package_count().unwrap(), 1);
        let retrieved = store.get_by_integrity("dup123").unwrap().unwrap();
        assert_eq!(retrieved.name, "replacement");
    }

    #[test]
    fn test_register_and_unregister_project() {
        let (store, _dir) = create_test_store();
        let project_path = Path::new("/tmp/test-project");
        store.register_project(project_path).unwrap();
        assert_eq!(store.project_count().unwrap(), 1);
        store.unregister_project(project_path).unwrap();
        assert_eq!(store.project_count().unwrap(), 0);
    }

    #[test]
    fn test_transaction_rollback() {
        let (store, _dir) = create_test_store();
        store.begin_transaction().unwrap();
        let pkg = test_package("rollback-pkg", "1.0.0", "rollback1");
        store.add_package(&pkg).unwrap();
        store.rollback().unwrap();
        assert_eq!(store.package_count().unwrap(), 0);
    }

    #[test]
    fn test_transaction_commit() {
        let (store, _dir) = create_test_store();
        store.begin_transaction().unwrap();
        let pkg = test_package("commit-pkg", "1.0.0", "commit1");
        store.add_package(&pkg).unwrap();
        store.commit().unwrap();
        assert_eq!(store.package_count().unwrap(), 1);
    }

    #[test]
    fn test_integrity_cache() {
        let (store, _dir) = create_test_store();
        let file_path = Path::new("/tmp/test-file.txt");
        store.update_integrity_cache(file_path, "cached-hash-123").unwrap();
        let cached = store.get_cached_integrity(file_path).unwrap().unwrap();
        assert_eq!(cached, "cached-hash-123");
    }

    #[test]
    fn test_missing_integrity_cache() {
        let (store, _dir) = create_test_store();
        let result = store.get_cached_integrity(Path::new("/nonexistent")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_package_count() {
        let (store, _dir) = create_test_store();
        assert_eq!(store.package_count().unwrap(), 0);
        for i in 0..10 {
            let pkg = test_package(
                &format!("count-pkg-{}", i),
                "1.0.0",
                &format!("count{}", i),
            );
            store.add_package(&pkg).unwrap();
        }
        assert_eq!(store.package_count().unwrap(), 10);
    }

    #[test]
    fn test_total_size() {
        let (store, _dir) = create_test_store();
        let pkg = test_package("size-test", "1.0.0", "size1");
        store.add_package(&pkg).unwrap();
        assert_eq!(store.total_size().unwrap(), 1024);
    }

    #[test]
    fn test_lru_cache_hit() {
        let (store, _dir) = create_test_store();
        let pkg = test_package("cache-hit", "1.0.0", "cachehit1");
        store.add_package(&pkg).unwrap();

        {
            let cache = store.cache.lock().unwrap();
            assert!(cache.contains("cachehit1"));
        }

        let retrieved = store.get_by_integrity("cachehit1").unwrap().unwrap();
        assert_eq!(retrieved.name, "cache-hit");
    }

    #[test]
    fn test_bulk_insert_performance() {
        let (store, _dir) = create_test_store();
        store.begin_transaction().unwrap();
        for i in 0..1000 {
            let pkg = test_package(
                &format!("bulk-{}", i),
                "1.0.0",
                &format!("{:040}", i),
            );
            store.add_package(&pkg).unwrap();
        }
        store.commit().unwrap();
        assert_eq!(store.package_count().unwrap(), 1000);
    }

    #[test]
    fn test_vacuum() {
        let (store, _dir) = create_test_store();
        let pkg = test_package("vacuum-test", "1.0.0", "vacuum1");
        store.add_package(&pkg).unwrap();
        store.delete_package("vacuum1").unwrap();
        store.vacuum().unwrap();
    }
}
