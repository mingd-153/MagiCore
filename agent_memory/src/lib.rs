use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

/// Global in‑memory key/value store for agent state.
pub static GLOBAL_MEMORY: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Set a value in the global memory.
pub fn set(key: impl Into<String>, value: impl Into<String>) {
    let mut map = GLOBAL_MEMORY.lock().unwrap();
    map.insert(key.into(), value.into());
}

/// Get a value from the global memory.
pub fn get(key: &str) -> Option<String> {
    let map = GLOBAL_MEMORY.lock().unwrap();
    map.get(key).cloned()
}

/// Remove a key from the global memory.
pub fn remove(key: &str) -> Option<String> {
    let mut map = GLOBAL_MEMORY.lock().unwrap();
    map.remove(key)
}

/// A tiny wrapper that mimics a hierarchical "trellis".
pub struct Trellis;

pub mod conversation;

impl Trellis {
    /// Store a value at the given hierarchical key.
    pub fn put(key: impl Into<String>, value: impl Into<String>) -> anyhow::Result<()> {
        set(key, value);
        Ok(())
    }
    /// Retrieve a value for the given key.
    pub fn fetch(key: &str) -> anyhow::Result<Option<String>> {
        Ok(get(key))
    }
    /// Delete a key from the store.
    pub fn delete(key: &str) -> anyhow::Result<Option<String>> {
        Ok(remove(key))
    }
}
