//! Search results cache with SQLite
//! Cache kết quả tìm kiếm với SQLite

use crate::types::{Registry, SearchResult};
use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Search cache - stores results and user choices
/// Cache tìm kiếm - lưu kết quả và lựa chọn user
pub struct SearchCache {
    db: Connection,
}

impl SearchCache {
    /// Create new cache (or open existing)
    /// Tạo cache mới (hoặc mở có sẵn)
    ///
    /// Cache location: ~/.magicore/search_cache.db
    /// Vị trí cache: ~/.magicore/search_cache.db
    pub fn new() -> Result<Self> {
        let cache_path = Self::cache_path()?;
        Self::new_with_path(&cache_path)
    }

    /// Create cache with custom path (for testing)
    /// Tạo cache với đường dẫn tuỳ chỉnh (cho test)
    pub fn new_with_path(path: &std::path::Path) -> Result<Self> {
        // Ensure parent directory exists
        // Đảm bảo thư mục cha tồn tại
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = Connection::open(path)?;

        let mut cache = Self { db };
        cache.init_schema()?;

        Ok(cache)
    }

    /// Get cache file path
    /// Lấy đường dẫn file cache
    fn cache_path() -> Result<PathBuf> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".magicore").join("search_cache.db"))
    }

    /// Initialize database schema
    /// Khởi tạo schema database
    fn init_schema(&mut self) -> Result<()> {
        self.db.execute(
            "CREATE TABLE IF NOT EXISTS search_cache (
                query TEXT PRIMARY KEY,
                results TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                ttl INTEGER DEFAULT 604800
            )",
            [],
        )?;

        self.db.execute(
            "CREATE INDEX IF NOT EXISTS idx_search_cache_timestamp 
             ON search_cache(timestamp)",
            [],
        )?;

        self.db.execute(
            "CREATE TABLE IF NOT EXISTS user_choices (
                package_name TEXT,
                registry TEXT,
                install_count INTEGER DEFAULT 1,
                last_used INTEGER NOT NULL,
                PRIMARY KEY (package_name, registry)
            )",
            [],
        )?;

        self.db.execute(
            "CREATE INDEX IF NOT EXISTS idx_user_choices_count 
             ON user_choices(install_count DESC)",
            [],
        )?;

        Ok(())
    }

    /// Get cached search results (if not expired)
    /// Lấy kết quả tìm kiếm từ cache (nếu chưa hết hạn)
    pub fn get(&self, query: &str) -> Result<Option<Vec<SearchResult>>> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

        let mut stmt = self
            .db
            .prepare("SELECT results, timestamp, ttl FROM search_cache WHERE query = ?")?;

        let result = stmt.query_row([query], |row| {
            let results_json: String = row.get(0)?;
            let timestamp: i64 = row.get(1)?;
            let ttl: i64 = row.get(2)?;

            // Check if expired
            // Kiểm tra có hết hạn không
            if now - timestamp > ttl {
                return Ok(None);
            }

            let results: Vec<SearchResult> = serde_json::from_str(&results_json)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            Ok(Some(results))
        });

        match result {
            Ok(r) => Ok(r),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Insert search results into cache
    /// Chèn kết quả tìm kiếm vào cache
    pub fn insert(&self, query: &str, results: &[SearchResult]) -> Result<()> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

        let results_json = serde_json::to_string(results)?;

        self.db.execute(
            "INSERT OR REPLACE INTO search_cache (query, results, timestamp, ttl)
             VALUES (?1, ?2, ?3, 604800)",
            params![query, results_json, now],
        )?;

        Ok(())
    }

    /// Track user choice (increment install count)
    /// Theo dõi lựa chọn user (tăng số lần cài đặt)
    pub fn track_choice(&self, package_name: &str, registry: Registry) -> Result<()> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

        let registry_str = registry.as_str();

        // Try to update existing record
        // Thử update bản ghi có sẵn
        let updated = self.db.execute(
            "UPDATE user_choices 
             SET install_count = install_count + 1, last_used = ?3
             WHERE package_name = ?1 AND registry = ?2",
            params![package_name, registry_str, now],
        )?;

        // If no rows updated, insert new record
        // Nếu không có hàng nào được update, chèn bản ghi mới
        if updated == 0 {
            self.db.execute(
                "INSERT INTO user_choices (package_name, registry, install_count, last_used)
                 VALUES (?1, ?2, 1, ?3)",
                params![package_name, registry_str, now],
            )?;
        }

        Ok(())
    }

    /// Get user choice (if installed 3+ times)
    /// Lấy lựa chọn user (nếu đã cài 3+ lần)
    pub fn get_user_choice(&self, package_name: &str) -> Result<Option<Registry>> {
        let mut stmt = self.db.prepare(
            "SELECT registry FROM user_choices 
             WHERE package_name = ? AND install_count >= 3
             ORDER BY install_count DESC, last_used DESC
             LIMIT 1",
        )?;

        let result = stmt.query_row([package_name], |row| {
            let registry_str: String = row.get(0)?;
            Ok(Registry::parse(&registry_str))
        });

        match result {
            Ok(Some(r)) => Ok(Some(r)),
            Ok(None) => Ok(None),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ResultMetadata;
    use tempfile::tempdir;

    #[test]
    fn test_cache_insert_and_get() {
        let temp_dir = tempdir().unwrap();
        let cache = SearchCache::new_with_path(&temp_dir.path().join("search_cache.db")).unwrap();

        let results = vec![SearchResult {
            name: "test".to_string(),
            registry: Registry::Npm,
            full_path: "test".to_string(),
            version: "1.0.0".to_string(),
            description: "Test package".to_string(),
            metadata: ResultMetadata::default(),
            score: 90.0,
        }];

        cache.insert("test", &results).unwrap();

        let cached = cache.get("test").unwrap();
        assert!(cached.is_some());
        assert_eq!(cached.unwrap()[0].name, "test");
    }

    #[test]
    fn test_track_choice() {
        let temp_dir = tempdir().unwrap();
        let cache = SearchCache::new_with_path(&temp_dir.path().join("search_cache.db")).unwrap();

        // Track 3 times
        cache.track_choice("gin", Registry::Go).unwrap();
        cache.track_choice("gin", Registry::Go).unwrap();
        cache.track_choice("gin", Registry::Go).unwrap();

        // Should return Go registry
        let choice = cache.get_user_choice("gin").unwrap();
        assert_eq!(choice, Some(Registry::Go));
    }
}
