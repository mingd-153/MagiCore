use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

/// Global in‑memory key/value store for agent state.
///
/// The store is a simple `HashMap<String, String>` protected by a `Mutex`
/// and exposed via the `GLOBAL_MEMORY` static.  It can be used by any
/// part of the application without having to pass a context object.
pub static GLOBAL_MEMORY: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Set a value in the global memory.
pub fn set(key: impl Into<String>, value: impl Into<String>) {
    let mut map = GLOBAL_MEMORY.lock().unwrap();
    map.insert(key.into(), value.into());
}

/// Get a value from the global memory.
/// Returns `None` if the key does not exist.
pub fn get(key: &str) -> Option<String> {
    let map = GLOBAL_MEMORY.lock().unwrap();
    map.get(key).cloned()
}

/// Remove a key from the global memory.
pub fn remove(key: &str) -> Option<String> {
    let mut map = GLOBAL_MEMORY.lock().unwrap();
    map.remove(key)
}
