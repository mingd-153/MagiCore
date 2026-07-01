use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Mutex;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use lru::LruCache;
use rusqlite::Connection;

use super::*;

impl SqliteStore {
    pub fn open(path: &Path, readonly: bool) -> Result<Self, StoreError> {
        let conn = if readonly {
            conn_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?
        } else {
            Connection::open(path)?
        };

        // Set secure file permissions (owner read/write only) for non-readonly DBs
        if !readonly {
            #[cfg(unix)]
            {
                use std::fs;
                let mut perms = fs::metadata(path)?.permissions();
                perms.set_mode(0o600);
                fs::set_permissions(path, perms)?;
            }
        }

        let ram = detect_available_ram();

        if !readonly {
            apply_pragmas(&conn, ram)?;
            create_tables(&conn)?;
            migrate_schema(&conn)?;
            health_check(&conn)?;
        } else {
            apply_pragmas_readonly(&conn, ram)?;
        }

        let lru_size = adaptive_lru_size(ram);

        let store = Self {
            conn: Mutex::new(conn),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(lru_size).unwrap())),
            path: path.to_path_buf(),
            readonly,
            generation: Mutex::new(0),
        };

        if !readonly {
            let gen: i64 = store
                .conn
                .lock()
                .unwrap()
                .query_row(
                    "SELECT COALESCE(MAX(generation), 0) FROM gc_state",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            *store.generation.lock().unwrap() = gen as u64;
        }

        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        let ram = detect_available_ram();

        apply_pragmas(&conn, ram)?;
        // Hard limit: 512MB max for in-memory database (131072 pages × 4096 bytes)
        conn.query_row("PRAGMA max_page_count = 131072", [], |_| Ok(()))?;
        create_tables(&conn)?;
        migrate_schema(&conn)?;
        health_check(&conn)?;

        let lru_size = adaptive_lru_size(ram);

        Ok(Self {
            conn: Mutex::new(conn),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(lru_size).unwrap())),
            path: Path::new(":memory:").to_path_buf(),
            readonly: false,
            generation: Mutex::new(0),
        })
    }

    pub fn health_check(&self) -> Result<Vec<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("PRAGMA quick_check")?;
        let result: String = stmt.query_row([], |row| row.get(0))?;
        if result != "ok" {
            return Err(StoreError::IntegrityCheck(result));
        }

        let (db_size_mb, wal_size_kb, cache_entries) = self.get_store_stats_with_conn(&conn)?;

        Ok(vec![
            format!("db_size: {} MB", db_size_mb),
            format!("wal_size: {} KB", wal_size_kb),
            format!("cache_entries: {}", cache_entries),
            format!("readonly: {}", self.readonly),
        ])
    }

    pub fn deep_integrity_check(&self) -> Result<Vec<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("PRAGMA integrity_check")?;
        let result: String = stmt.query_row([], |row| row.get(0))?;
        if result != "ok" {
            return Err(StoreError::IntegrityCheck(result));
        }

        let (db_size_mb, wal_size_kb, cache_entries) = self.get_store_stats_with_conn(&conn)?;

        Ok(vec![
            format!("db_size: {} MB", db_size_mb),
            format!("wal_size: {} KB", wal_size_kb),
            format!("cache_entries: {}", cache_entries),
            format!("readonly: {}", self.readonly),
        ])
    }

    pub fn vacuum(&self) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute("VACUUM", [])?;
        Ok(())
    }

    pub(crate) fn checkpoint_if_needed(conn: &Connection) {
        let wal_size: i64 = conn
            .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| row.get(0))
            .unwrap_or(0);
        if wal_size > 4000 {
            conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
                .ok();
        }
    }

    // Internal helper that takes an already-locked connection to avoid deadlock
    fn get_store_stats_with_conn(
        &self,
        conn: &Connection,
    ) -> Result<(u64, u64, usize), StoreError> {
        let page_count: i64 = conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .unwrap_or(0);
        let page_size: i64 = conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))
            .unwrap_or(0);
        let wal_size = get_wal_size(&self.path);
        let db_size = ((page_count * page_size) / (1024 * 1024)) as u64;
        let wal_size_kb = (wal_size / 1024) as u64;
        let cache_entries = self.cache.lock().unwrap().len();
        Ok((db_size, wal_size_kb, cache_entries))
    }

    // Public version that locks the connection itself
    pub(crate) fn get_store_stats(&self) -> Result<(u64, u64, usize), StoreError> {
        let conn = self.conn.lock().unwrap();
        self.get_store_stats_with_conn(&conn)
    }
}
