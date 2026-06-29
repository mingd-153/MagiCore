use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use super::*;

impl StoreIndex for SqliteStore {
    fn add_package(&self, info: &PackageInfo) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        
        // Check if package with this integrity already exists
        let existing = conn.query_row(
            "SELECT name, version FROM packages WHERE integrity = ?1",
            rusqlite::params![info.integrity],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        );
        
        match existing {
            Ok((existing_name, existing_version)) => {
                // Package with this integrity exists — verify it's the same package
                if existing_name != info.name || existing_version != info.version {
                    return Err(StoreError::Database(format!(
                        "integrity collision: hash {} already used by {}@{}",
                        info.integrity, existing_name, existing_version
                    )));
                }
                // Same package — idempotent, update cache only
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // New integrity — insert
                conn.execute(
                    "INSERT INTO packages
                     (name, version, integrity, shard, filename, is_executable,
                      manifest_json, metadata, size_bytes, compressed_size_bytes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        info.name,
                        info.version,
                        info.integrity,
                        info.shard,
                        info.filename,
                        info.is_executable as i32,
                        info.manifest_json,
                        info.metadata,
                        info.size_bytes as i64,
                        info.compressed_size_bytes as i64,
                    ],
                )?;
            }
            Err(e) => return Err(StoreError::from(e)),
        }

        SqliteStore::checkpoint_if_needed(&conn);

        let mut cache = self.cache.lock().unwrap();
        cache.put(info.integrity.clone(), info.clone());

        Ok(())
    }

    fn get_package(&self, name: &str, version: &str) -> Result<Option<PackageInfo>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name, version, integrity, shard, filename, is_executable,
                    manifest_json, size_bytes, compressed_size_bytes, created_at, metadata
             FROM packages WHERE name = ?1 AND version = ?2",
        )?;

        let result = stmt.query_row(rusqlite::params![name, version], row_to_package);

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
                    manifest_json, size_bytes, compressed_size_bytes, created_at, metadata
             FROM packages WHERE integrity = ?1",
        )?;

        let result = stmt.query_row(rusqlite::params![hash], row_to_package);

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
            rusqlite::params![hash],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    fn delete_package(&self, hash: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM packages WHERE integrity = ?1",
            rusqlite::params![hash],
        )?;

        SqliteStore::checkpoint_if_needed(&conn);

        let mut cache = self.cache.lock().unwrap();
        cache.pop(hash);

        Ok(())
    }

    fn register_project(&self, path: &Path) -> Result<(), StoreError> {
        if path.as_os_str().is_empty() {
            return Err(StoreError::Io {
                path: path.to_path_buf(),
                msg: "empty path is not allowed".to_string(),
            });
        }

        let normalized = if path.exists() {
            // Canonicalize resolves .., ., and symlinks
            std::fs::canonicalize(path).map_err(|e| StoreError::Io {
                path: path.to_path_buf(),
                msg: format!("failed to resolve project path: {}", e),
            })?
        } else {
            let path_str = path.to_string_lossy();
            if path_str.contains("..") {
                return Err(StoreError::Io {
                    path: path.to_path_buf(),
                    msg: "path traversal detected".to_string(),
                });
            }
            std::path::absolute(path).map_err(|e| StoreError::Io {
                path: path.to_path_buf(),
                msg: format!("failed to resolve path: {}", e),
            })?
        };

        let path_str = normalized.to_string_lossy().to_string();
        let hash = hex::encode(Sha256::digest(path_str.as_bytes()));

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO projects (project_hash, path, last_used)
             VALUES (?1, ?2, unixepoch())",
            rusqlite::params![hash, path_str],
        )?;
        Ok(())
    }

    fn unregister_project(&self, path: &Path) -> Result<(), StoreError> {
        if path.as_os_str().is_empty() {
            return Err(StoreError::Io {
                path: path.to_path_buf(),
                msg: "empty path is not allowed".to_string(),
            });
        }

        let normalized = if path.exists() {
            std::fs::canonicalize(path).map_err(|e| StoreError::Io {
                path: path.to_path_buf(),
                msg: format!("failed to resolve project path: {}", e),
            })?
        } else {
            let path_str = path.to_string_lossy();
            if path_str.contains("..") {
                return Err(StoreError::Io {
                    path: path.to_path_buf(),
                    msg: "path traversal detected".to_string(),
                });
            }
            std::path::absolute(path).map_err(|e| StoreError::Io {
                path: path.to_path_buf(),
                msg: format!("failed to resolve path: {}", e),
            })?
        };

        let path_str = normalized.to_string_lossy().to_string();

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM projects WHERE path = ?1",
            rusqlite::params![path_str],
        )?;
        Ok(())
    }

    fn get_unreferenced_packages(&self) -> Result<Vec<PackageInfo>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT p.name, p.version, p.integrity, p.shard, p.filename,
                    p.is_executable, p.manifest_json, p.size_bytes,
                    p.compressed_size_bytes, p.created_at, p.metadata
             FROM packages p
             WHERE p.generation < (
                 SELECT COALESCE(MAX(generation), 0) - 1 FROM gc_state
             )",
        )?;

        let packages = stmt
            .query_map([], row_to_package)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(packages)
    }

    fn update_integrity_cache(&self, file_path: &Path, hash: &str) -> Result<(), StoreError> {
        // Validate algorithm (only sha256 supported for now)
        let algorithm = "sha256";
        SqliteStore::validate_algorithm(algorithm)?;

        let path_str = file_path.to_string_lossy();

        // Re-verify file content to prevent TOCTOU (time-of-check-to-time-of-use)
        // Only for regular files; skip for directories and special paths
        if let Ok(meta) = std::fs::metadata(file_path) {
            if meta.is_file() {
                let computed_hash = {
                    let mut file = std::fs::File::open(file_path)?;
                    let mut hasher = Sha256::new();
                    let mut buf = [0u8; 8192];
                    loop {
                        let n = file.read(&mut buf)?;
                        if n == 0 {
                            break;
                        }
                        hasher.update(&buf[..n]);
                    }
                    hex::encode(hasher.finalize())
                };

                if computed_hash != hash {
                    return Err(StoreError::HashMismatch {
                        expected: hash.to_string(),
                        actual: computed_hash,
                    });
                }
            }
        }

        let mtime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO integrity_cache (file_path, integrity, algorithm, mtime)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![path_str.to_string(), hash, algorithm, mtime],
        )?;

        // Invalidate LRU cache if this hash was cached
        self.cache.lock().unwrap().pop(hash);

        Ok(())
    }

    fn get_cached_integrity(&self, file_path: &Path) -> Result<Option<String>, StoreError> {
        let path_str = file_path.to_string_lossy();

        // Get cached data (release lock before file I/O)
        let cached = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT integrity, mtime FROM integrity_cache WHERE file_path = ?1",
                rusqlite::params![path_str.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
        };

        match cached {
            Ok((cached_hash, _cached_mtime)) => {
                // Try to re-verify file content to prevent cache poisoning
                // If we can't read the file (directory, special file, etc.), return cached hash
                if let Ok(meta) = std::fs::metadata(file_path) {
                    // Only verify if it's a regular file
                    if meta.is_file() {
                        if let Ok(mut file) = std::fs::File::open(file_path) {
                            let mut hasher = Sha256::new();
                            let mut buf = [0u8; 8192];
                            loop {
                                let n = file.read(&mut buf).map_err(|e| {
                                    StoreError::Io { path: file_path.to_path_buf(), msg: e.to_string() }
                                })?;
                                if n == 0 { break; }
                                hasher.update(&buf[..n]);
                            }
                            let computed = hex::encode(hasher.finalize());
                            if computed == cached_hash {
                                return Ok(Some(cached_hash));
                            }
                            // Hash mismatch - stale cache, remove it
                            let conn = self.conn.lock().unwrap();
                            conn.execute(
                                "DELETE FROM integrity_cache WHERE file_path = ?1",
                                rusqlite::params![path_str.to_string()],
                            ).ok();
                            return Ok(None);
                        }
                    }
                    // Could not open file for verification - return cached hash
                }
                // Not a regular file or could not get metadata - return cached hash
                Ok(Some(cached_hash))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::from(e)),
        }
    }

    fn begin_transaction(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("BEGIN IMMEDIATE", [])?;
        Ok(())
    }

    fn commit(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("COMMIT", [])?;
        SqliteStore::checkpoint_if_needed(&conn);
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

    fn get_all_packages(&self) -> Result<Vec<PackageInfo>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name, version, integrity, shard, filename, is_executable,
                    manifest_json, size_bytes, compressed_size_bytes, created_at, metadata
             FROM packages",
        )?;
        let packages = stmt
            .query_map([], row_to_package)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(packages)
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

impl SqliteStore {
    fn validate_algorithm(algorithm: &str) -> Result<(), StoreError> {
        match algorithm {
            "sha256" | "blake3" => Ok(()),
            other => Err(StoreError::Database(format!(
                "unsupported integrity algorithm: {}. Supported: sha256, blake3",
                other
            ))),
        }
    }
}
