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
        Self {
            algorithm,
            hash,
            size,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HashAlgorithm;

    fn make_integrity(data: &[u8]) -> Integrity {
        let hash = crate::hash(data, HashAlgorithm::Sha256).unwrap();
        Integrity::new(HashAlgorithm::Sha256, hash, data.len() as u64)
    }

    // --- Integrity::verify ---

    #[test]
    fn test_integrity_verify_correct() {
        let data = b"hello";
        let integrity = make_integrity(data);
        assert!(integrity.verify(data).unwrap());
    }

    #[test]
    fn test_integrity_verify_incorrect_data() {
        let data = b"hello";
        let integrity = make_integrity(data);
        assert!(!integrity.verify(b"world").unwrap());
    }

    #[test]
    fn test_integrity_verify_size_mismatch() {
        let data = b"hello";
        let integrity = make_integrity(data);
        // Correct data wrong size → false (short-circuit before hash)
        assert!(!integrity.verify(b"helloo").unwrap());
        assert!(!integrity.verify(b"hell").unwrap());
    }

    #[test]
    fn test_integrity_verify_tampered_hash_field() {
        // Correct data but Integrity stores wrong hash
        let integrity = Integrity::new(HashAlgorithm::Sha256, "0000".into(), 5);
        assert!(!integrity.verify(b"hello").unwrap());
    }

    // --- IntegrityMap::verify_all ---

    #[test]
    fn test_integrity_map_verify_all_all_match() {
        let mut imap = IntegrityMap::new();
        let mut files = HashMap::new();

        let a_data = b"alpha".to_vec();
        let b_data = b"beta".to_vec();
        let a_hash = crate::hash(&a_data, HashAlgorithm::Sha256).unwrap();
        let b_hash = crate::hash(&b_data, HashAlgorithm::Sha256).unwrap();

        imap.insert("a.txt".into(), Integrity::new(HashAlgorithm::Sha256, a_hash, 5));
        imap.insert("b.txt".into(), Integrity::new(HashAlgorithm::Sha256, b_hash, 4));
        files.insert("a.txt".into(), a_data);
        files.insert("b.txt".into(), b_data);

        assert!(imap.verify_all(&files).unwrap());
    }

    #[test]
    fn test_integrity_map_verify_all_one_mismatch() {
        let mut imap = IntegrityMap::new();
        let mut files = HashMap::new();

        let a_data = b"alpha".to_vec();
        let a_hash = crate::hash(&a_data, HashAlgorithm::Sha256).unwrap();

        imap.insert("a.txt".into(), Integrity::new(HashAlgorithm::Sha256, a_hash, 5));
        imap.insert("b.txt".into(), Integrity::new(HashAlgorithm::Sha256, "badhash".into(), 4));
        files.insert("a.txt".into(), a_data);
        files.insert("b.txt".into(), b"beta".to_vec());

        assert!(!imap.verify_all(&files).unwrap());
    }

    #[test]
    fn test_integrity_map_verify_all_missing_file() {
        let mut imap = IntegrityMap::new();
        let mut files = HashMap::new();

        let data = b"present".to_vec();
        let hash = crate::hash(&data, HashAlgorithm::Sha256).unwrap();

        imap.insert("present.txt".into(), Integrity::new(HashAlgorithm::Sha256, hash.clone(), 7));
        imap.insert("missing.txt".into(), Integrity::new(HashAlgorithm::Sha256, hash, 7));
        files.insert("present.txt".into(), data);

        assert!(!imap.verify_all(&files).unwrap());
    }

    #[test]
    fn test_integrity_map_verify_all_empty_map() {
        let imap = IntegrityMap::new();
        let files = HashMap::new();
        assert!(imap.verify_all(&files).unwrap());
    }
}
