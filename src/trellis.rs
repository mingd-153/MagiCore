use crate::global::{set, get, remove};
use anyhow::Result;

/// A very small wrapper around the global memory that mimics a "trellis"
/// structure – a hierarchical key space where keys are dot‑separated
/// paths (e.g. "agent.session.id").  Values are stored as strings.
pub struct Trellis;

impl Trellis {
    /// Store a value at the given hierarchical key.
    pub fn put(key: impl Into<String>, value: impl Into<String>) -> Result<()> {
        set(key, value);
        Ok(())
    }

    /// Retrieve a value for the given hierarchical key.
    pub fn fetch(key: &str) -> Result<Option<String>> {
        Ok(get(key))
    }

    /// Delete a key from the store.
    pub fn delete(key: &str) -> Result<Option<String>> {
        Ok(remove(key))
    }
}
