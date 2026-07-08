/// Integrity verification for packages and files
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Integrity information for a package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Integrity {
    /// Algorithm used
    pub algorithm: super::HashAlgorithm,
    /// Hash value
    pub hash: String,
    /// File size in bytes
    pub size: u64,
}

impl Integrity {
    pub fn new(algorithm: super::HashAlgorithm, hash: String, size: u64) -> Self {
        Self { algorithm, hash, size }
    }

    /// Verify data matches this integrity
    pub fn verify(&self, data: &[u8]) -> Result<bool> {
        if data.len() as u64 != self.size {
            return Ok(false);
        }
        super::verify_hash(data, &self.hash, self.algorithm)
    }
}

/// Integrity map for multiple files
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntegrityMap {
    entries: HashMap<String, Integrity>,
}

impl IntegrityMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: String, integrity: Integrity) {
        self.entries.insert(path, integrity);
    }

    pub fn get(&self, path: &str) -> Option<&Integrity> {
        self.entries.get(path)
    }

    pub fn verify_all(&self, data: &HashMap<String, Vec<u8>>) -> Result<bool> {
        for (path, integrity) in &self.entries {
            if let Some(file_data) = data.get(path) {
                if !integrity.verify(file_data)? {
                    return Ok(false);
                }
            } else {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
