/// Lockfile serialization utilities
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Serialize to JSON string
pub fn to_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
}

/// Deserialize from JSON string
pub fn from_json<T: for<'de> Deserialize<'de>>(s: &str) -> Result<T> {
    Ok(serde_json::from_str(s)?)
}
