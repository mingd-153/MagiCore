/// SQLite-backed database for installed packages and integrity metadata.
use anyhow::Result;
use mg_types::PackageId;
use rusqlite::{params, Connection};
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
            CREATE TABLE IF NOT EXISTS refs (
                project_root TEXT NOT NULL,
                package_id TEXT NOT NULL,
                ref_count INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (project_root, package_id)
            );
            CREATE TABLE IF NOT EXISTS cas_blob_refs (
                project_root TEXT NOT NULL,
                hash TEXT NOT NULL,
                PRIMARY KEY (project_root, hash)
            );
            CREATE INDEX IF NOT EXISTS idx_cas_blob_refs_hash ON cas_blob_refs (hash);
            CREATE TABLE IF NOT EXISTS package_files (
                id TEXT NOT NULL,
                version TEXT NOT NULL,
                path TEXT NOT NULL,
                blob_hash TEXT NOT NULL,
                size INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (id, version, path)
            );
            CREATE INDEX IF NOT EXISTS idx_package_files_blob ON package_files (blob_hash);
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA busy_timeout=5000;",
        )?;

        Ok(Self { conn })
    }

    /// Raw connection — for StoreIndex full scans.
    pub fn conn(&self) -> &Connection {
        &self.conn
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
        let mut stmt = self
            .conn
            .prepare("SELECT COUNT(*) FROM packages WHERE id = ?1 AND version = ?2")?;
        let count: i64 = stmt
            .query_row(params![id.name_str(), id.version().to_string()], |row| {
                row.get(0)
            })?;
        Ok(count > 0)
    }

    pub fn list_installed(&self) -> Result<Vec<DatabaseEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, version, integrity, installed_at FROM packages ORDER BY id")?;
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
        let mut stmt = self
            .conn
            .prepare("SELECT COUNT(*) FROM integrity_cache WHERE hash = ?1")?;
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

    /// Mark (project_root, package_id) as referenced — install. (02 §2.2)
    pub fn set_ref(&self, project_root: &str, id: &PackageId) -> Result<()> {
        self.conn.execute(
            "INSERT INTO refs (project_root, package_id, ref_count) VALUES (?1, ?2, 1)
             ON CONFLICT(project_root, package_id)
             DO UPDATE SET ref_count = 1",
            params![project_root, id.to_string()],
        )?;
        Ok(())
    }

    /// Remove the project reference for a package — remove/uninstall.
    pub fn clear_ref(&self, project_root: &str, id: &PackageId) -> Result<()> {
        self.conn.execute(
            "DELETE FROM refs WHERE project_root = ?1 AND package_id = ?2",
            params![project_root, id.to_string()],
        )?;
        Ok(())
    }

    /// Remove all references of a project — called before re-install so the
    /// refs table mirrors the current graph exactly (no stale entries).
    pub fn clear_all_refs(&self, project_root: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM refs WHERE project_root = ?1",
            params![project_root],
        )?;
        Ok(())
    }

    /// Return installed packages with no project referencing them —
    /// candidates for `mg store prune` (02 §2.2).
    pub fn list_unreferenced(&self) -> Result<Vec<PackageId>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.version FROM packages p
             LEFT JOIN refs r ON r.package_id = p.id || '@' || p.version
             GROUP BY p.id, p.version
             HAVING COUNT(r.project_root) = 0",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let version: String = row.get(1)?;
            Ok(format!("{id}@{version}"))
        })?;
        let mut result = Vec::new();
        for row in rows {
            if let Ok(parsed) = PackageId::parse(&row?) {
                result.push(parsed);
            }
        }
        Ok(result)
    }

    /// Register that `project_root` references a CAS blob — idempotent per
    /// (project, hash). Call after `clear_all_cas_refs(project)` on re-install
    /// so the table mirrors the current graph exactly.
    /// (Đăng ký project tham chiếu blob CAS — idempotent theo (project, hash).
    ///  Gọi sau clear_all_cas_refs khi re-install để bảng khớp graph hiện tại.)
    pub fn cas_claim(&self, project_root: &str, hash: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO cas_blob_refs (project_root, hash) VALUES (?1, ?2)",
            params![project_root, hash],
        )?;
        Ok(())
    }

    /// Remove all CAS blob claims of a project — called before re-install.
    /// (Xóa toàn bộ claim blob của project — gọi trước khi re-install.)
    pub fn clear_all_cas_refs(&self, project_root: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM cas_blob_refs WHERE project_root = ?1",
            params![project_root],
        )?;
        Ok(())
    }

    /// Remove a single project claim (package removed from the graph).
    /// (Xóa 1 claim của project — package bị gỡ khỏi graph.)
    pub fn cas_release(&self, project_root: &str, hash: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM cas_blob_refs WHERE project_root = ?1 AND hash = ?2",
            params![project_root, hash],
        )?;
        Ok(())
    }

    /// Blob hashes with at least one live project claim — must NOT be pruned.
    /// (Hash blob còn ít nhất 1 project claim — không được xóa.)
    pub fn list_cas_live_refs(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT hash FROM cas_blob_refs ORDER BY hash")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Replace the file listing for one package (schema + source of truth for
    /// StoreIndex). Atomic within a transaction.
    pub fn replace_package_files(
        &self,
        id: &PackageId,
        files: &[(String, String, u64)],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM package_files WHERE id = ?1 AND version = ?2",
            params![id.name_str(), id.version().to_string()],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO package_files (id, version, path, blob_hash, size) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (path, hash, size) in files {
                stmt.execute(params![
                    id.name_str(),
                    id.version().to_string(),
                    path,
                    hash,
                    size
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Full file listing for a package — ordered by path. Empty = not indexed.
    pub fn list_package_files(&self, id: &PackageId) -> Result<Vec<(String, String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, blob_hash, size FROM package_files
             WHERE id = ?1 AND version = ?2 ORDER BY path",
        )?;
        let rows = stmt.query_map(params![id.name_str(), id.version().to_string()], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Every blob hash referenced by any indexed package.
    pub fn list_all_blob_hashes(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT blob_hash FROM package_files ORDER BY blob_hash")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Number of indexed packages — cheap health check for StoreIndex.rebuild.
    pub fn count_indexed_packages(&self) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT id || '#' || version) FROM package_files",
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
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
        db.insert_package(&PackageId::parse("react@18.2.0").unwrap(), None)
            .unwrap();
        db.insert_package(&PackageId::parse("vue@3.4.0").unwrap(), None)
            .unwrap();
        let list = db.list_installed().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_refcount_set_clear_unreferenced() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let pkg = PackageId::parse("react@18.2.0").unwrap();
        db.insert_package(&pkg, None).unwrap();
        let project = "/tmp/proj-a";
        db.set_ref(project, &pkg).unwrap();
        assert!(db.list_unreferenced().unwrap().is_empty());
        db.clear_ref(project, &pkg).unwrap();
        let unreferenced = db.list_unreferenced().unwrap();
        assert_eq!(unreferenced, vec![pkg.clone()]);
        // Re-install in another project keeps it referenced.
        db.set_ref("/tmp/proj-b", &pkg).unwrap();
        assert!(db.list_unreferenced().unwrap().is_empty());
    }
}
