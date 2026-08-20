/// SQLite-backed database for installed packages and integrity metadata.
use anyhow::Result;
use mg_types::{PackageId, Version};
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
            CREATE TABLE IF NOT EXISTS trust_policy (
                package_id TEXT PRIMARY KEY,
                policy TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS release_policy (
                ecosystem TEXT PRIMARY KEY,
                min_age_secs INTEGER NOT NULL
            );
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

    // ── T5 security trust gate ──────────────────────────────────────────

    /// Policy recorded for a package. `package_id` is the raw key: `name` or
    /// `name@version`.
    pub fn get_trust_policy(&self, package_id: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT policy FROM trust_policy WHERE package_id = ?1")?;
        let mut rows = stmt.query_map(params![package_id], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(Ok(policy)) => Ok(Some(policy)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Record approve/deny for a package (`name` covers all versions,
    /// `name@version` covers exactly one).
    pub fn upsert_trust_policy(&self, package_id: &str, policy: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.conn.execute(
            "INSERT OR REPLACE INTO trust_policy (package_id, policy, updated_at) VALUES (?1, ?2, ?3)",
            params![package_id, policy, now],
        )?;
        Ok(())
    }

    pub fn list_trust_policies(&self) -> Result<Vec<(String, String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT package_id, policy, updated_at FROM trust_policy ORDER BY package_id",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn clear_trust_policy(&self, package_id: &str) -> Result<bool> {
        let n = self.conn.execute(
            "DELETE FROM trust_policy WHERE package_id = ?1",
            params![package_id],
        )?;
        Ok(n > 0)
    }

    /// Remove policies for packages no longer installed (bare name or
    /// name@version). Returns number of rows removed. `mg trust prune`.
    pub fn prune_trust_policies(&self) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM trust_policy WHERE package_id NOT IN (
                 SELECT id || '@' || version FROM packages
             ) AND package_id NOT IN (
                 SELECT id FROM packages
             )",
            [],
        )?;
        Ok(n)
    }

    /// Min-release-age (seconds) for an ecosystem. None = not set (use default).
    pub fn release_policy(&self, ecosystem: &str) -> Result<Option<u64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT min_age_secs FROM release_policy WHERE ecosystem = ?1")?;
        let mut rows = stmt.query_map(params![ecosystem], |row| row.get::<_, u64>(0))?;
        match rows.next() {
            Some(Ok(v)) => Ok(Some(v)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    pub fn upsert_release_policy(&self, ecosystem: &str, min_age_secs: u64) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO release_policy (ecosystem, min_age_secs) VALUES (?1, ?2)",
            params![ecosystem, min_age_secs],
        )?;
        Ok(())
    }

    /// Highest installed version for a package name — None = not installed.
    /// Used by the T5 no-downgrade guard (semver compare, not string compare).
    pub fn latest_installed_version(&self, name: &str) -> Result<Option<Version>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT version FROM packages WHERE id = ?1")?;
        let rows = stmt.query_map(params![name], |row| row.get::<_, String>(0))?;
        let mut best: Option<Version> = None;
        for row in rows {
            let v = match Version::parse(&row?) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if best.as_ref().is_none_or(|b| v > *b) {
                best = Some(v);
            }
        }
        Ok(best)
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

    #[test]
    fn test_trust_policy_upsert_get_list_clear() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        assert_eq!(db.get_trust_policy("react").unwrap(), None);

        db.upsert_trust_policy("react", "approved").unwrap();
        assert_eq!(
            db.get_trust_policy("react").unwrap().as_deref(),
            Some("approved")
        );

        db.upsert_trust_policy("react", "denied").unwrap();
        assert_eq!(
            db.get_trust_policy("react").unwrap().as_deref(),
            Some("denied")
        );

        let list = db.list_trust_policies().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "react");
        assert_eq!(list[0].1, "denied");
        assert!(list[0].2 > 0);

        assert!(db.clear_trust_policy("react").unwrap());
        assert_eq!(db.get_trust_policy("react").unwrap(), None);
    }

    #[test]
    fn test_trust_policy_exact_version_and_bare_name_are_distinct() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        db.upsert_trust_policy("react", "approved").unwrap();
        db.upsert_trust_policy("react@18.2.0", "denied").unwrap();

        // bare covers any version; exact overrides for that version via keys.
        assert_eq!(
            db.get_trust_policy("react").unwrap().as_deref(),
            Some("approved")
        );
        assert_eq!(
            db.get_trust_policy("react@18.2.0").unwrap().as_deref(),
            Some("denied")
        );
        assert_eq!(db.get_trust_policy("react@19.0.0").unwrap(), None);
    }

    #[test]
    fn test_prune_trust_policies_keeps_installed() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let installed = PackageId::parse("react@18.2.0").unwrap();
        db.insert_package(&installed, None).unwrap();

        db.upsert_trust_policy("react", "approved").unwrap();
        db.upsert_trust_policy("react@18.2.0", "approved").unwrap();
        db.upsert_trust_policy("ghost@1.0.0", "approved").unwrap();

        assert_eq!(db.prune_trust_policies().unwrap(), 1); // only ghost removed
        assert!(db.get_trust_policy("react").unwrap().is_some());
        assert!(db.get_trust_policy("react@18.2.0").unwrap().is_some());
        assert!(db.get_trust_policy("ghost@1.0.0").unwrap().is_none());
    }

    #[test]
    fn test_release_policy_default_and_upsert() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        assert_eq!(db.release_policy("web").unwrap(), None);

        db.upsert_release_policy("web", 86400).unwrap();
        assert_eq!(db.release_policy("web").unwrap(), Some(86400));

        db.upsert_release_policy("web", 0).unwrap();
        assert_eq!(db.release_policy("web").unwrap(), Some(0));
        assert_eq!(db.release_policy("game").unwrap(), None);
    }

    #[test]
    fn test_latest_installed_version_semver_compared() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        assert_eq!(db.latest_installed_version("react").unwrap(), None);

        db.insert_package(&PackageId::parse("react@9.0.0").unwrap(), None)
            .unwrap();
        db.insert_package(&PackageId::parse("react@10.2.0").unwrap(), None)
            .unwrap();
        db.insert_package(&PackageId::parse("react@2.5.0").unwrap(), None)
            .unwrap();

        // "10.2.0" > "9.0.0" as semver even though "9" sorts after "10" as text.
        let latest = db.latest_installed_version("react").unwrap().unwrap();
        assert_eq!(latest, Version::new(10, 2, 0));
    }
}
