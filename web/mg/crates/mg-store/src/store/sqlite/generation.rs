use super::*;

impl SqliteStore {
    pub fn advance_generation(&self) -> Result<u64, StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // Atomic: insert new generation = max + 1 in single statement
        tx.execute(
            "INSERT INTO gc_state (generation) 
             VALUES ((SELECT COALESCE(MAX(generation), 0) + 1 FROM gc_state))",
            [],
        )?;

        // Get the new generation value
        let new_gen: i64 = tx.query_row(
            "SELECT COALESCE(MAX(generation), 0) FROM gc_state",
            [],
            |row| row.get(0),
        )?;

        tx.commit()?;
        *self.generation.lock().unwrap() = new_gen as u64;
        Ok(new_gen as u64)
    }

    pub fn current_generation(&self) -> u64 {
        *self.generation.lock().unwrap()
    }

    pub fn clean_old_generations(&self, keep: u64) -> Result<u64, StoreError> {
        let cutoff = self.current_generation().saturating_sub(keep);
        let conn = self.conn.lock().unwrap();
        let deleted: u64 = conn.execute(
            "DELETE FROM gc_state WHERE generation < ?1",
            rusqlite::params![cutoff as i64],
        )? as u64;
        Ok(deleted)
    }
}
