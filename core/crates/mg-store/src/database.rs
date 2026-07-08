/// SQLite-backed database for installed packages and integrity metadata.

use anyhow::Result;
use rusqlite::{Connection, params};
use mg_types::PackageId;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DatabaseEntry {
    pub id: String,
    pub version: String,
    pub integrity: Option<String>,
    pub installed_at: u64,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS packages (
                id TEXT NOT NULL,
                version TEXT NOT NULL,
                integrity TEXT,
                installed_at INTEGER NOT NULL,
                PRIMARY KEY (id, version)
            );
            CREATE TABLE IF NOT EXISTS integrity_cache (
                hash TEXT PRIMARY KEY,
                verified_at INTEGER NOT NULL
            );
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA busy_timeout=5000;"
        )?;

        Ok(Self { conn })
    }

    pub fn insert_package(&self, id: &PackageId, integrity: Option<&str>) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.conn.execute(
            "INSERT OR REPLACE INTO packages (id, version, integrity, installed_at) VALUES (?1, ?2, ?3, ?4)",
            params![id.name_str(), id.version().to_string(), integrity, now],
        )?;

        Ok(())
    }

    pub fn is_installed(&self, id: &PackageId) -> Result<bool> {
        let mut stmt = self.conn.prepare(
            "SELECT COUNT(*) FROM packages WHERE id = ?1 AND version = ?2"
        )?;
        let count: i64 = stmt.query_row(
            params![id.name_str(), id.version().to_string()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn list_installed(&self) -> Result<Vec<DatabaseEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, version, integrity, installed_at FROM packages ORDER BY id"
        )?;
        let entries = stmt.query_map([], |row| {
            Ok(DatabaseEntry {
                id: row.get(0)?,
                version: row.get(1)?,
                integrity: row.get(2)?,
                installed_at: row.get(3)?,
            })
        })?;

        let mut result = Vec::new();
        for entry in entries {
            result.push(entry?);
        }
        Ok(result)
    }

    pub fn remove_package(&self, id: &PackageId) -> Result<()> {
        self.conn.execute(
            "DELETE FROM packages WHERE id = ?1 AND version = ?2",
            params![id.name_str(), id.version().to_string()],
        )?;
        Ok(())
    }

    pub fn verify_integrity(&self, hash: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare(
            "SELECT COUNT(*) FROM integrity_cache WHERE hash = ?1"
        )?;
        let count: i64 = stmt.query_row(params![hash], |row| row.get(0))?;
        Ok(count > 0)
    }

    pub fn cache_integrity(&self, hash: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.conn.execute(
            "INSERT OR REPLACE INTO integrity_cache (hash, verified_at) VALUES (?1, ?2)",
            params![hash, now],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_open_create() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let list = db.list_installed().unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_insert_and_check() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let pkg = PackageId::parse("react@18.2.0").unwrap();
        db.insert_package(&pkg, Some("sha256-abc123")).unwrap();
        assert!(db.is_installed(&pkg).unwrap());
    }

    #[test]
    fn test_list_installed() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        db.insert_package(&PackageId::parse("react@18.2.0").unwrap(), None).unwrap();
        db.insert_package(&PackageId::parse("vue@3.4.0").unwrap(), None).unwrap();
        let list = db.list_installed().unwrap();
        assert_eq!(list.len(), 2);
    }
}
