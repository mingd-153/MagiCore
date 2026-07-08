use super::*;

impl SqliteStore {
    pub fn set_kv(&self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO kv_store (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub fn get_kv(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let result: Result<Vec<u8>, _> = conn.query_row(
            "SELECT value FROM kv_store WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::from(e)),
        }
    }

    pub fn delete_kv(&self, key: &str) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute(
            "DELETE FROM kv_store WHERE key = ?1",
            rusqlite::params![key],
        )?;
        Ok(())
    }
}
