/// Cryptographic primitives for MegaGate
/// 
/// Provides hashing, integrity verification, and checksum utilities.

use anyhow::Result;
use sha2::{Sha256, Sha512, Digest};
use blake3::Hasher as Blake3Hasher;

pub mod integrity;
pub mod checksum;

/// Hash algorithm types
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HashAlgorithm {
    Sha256,
    Sha512,
    Blake3,
}

/// Compute hash of data using specified algorithm
pub fn hash(data: &[u8], algorithm: HashAlgorithm) -> Result<String> {
    match algorithm {
        HashAlgorithm::Sha256 => {
            let mut hasher = Sha256::new();
            hasher.update(data);
            Ok(hex::encode(hasher.finalize()))
        }
        HashAlgorithm::Sha512 => {
            let mut hasher = Sha512::new();
            hasher.update(data);
            Ok(hex::encode(hasher.finalize()))
        }
        HashAlgorithm::Blake3 => {
            let mut hasher = Blake3Hasher::new();
            hasher.update(data);
            Ok(hasher.finalize().to_hex().to_string())
        }
    }
}

/// Verify hash matches expected value
pub fn verify_hash(data: &[u8], expected: &str, algorithm: HashAlgorithm) -> Result<bool> {
    let computed = hash(data, algorithm)?;
    Ok(computed == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_hash() {
        let data = b"hello world";
        let hash = hash(data, HashAlgorithm::Sha256).unwrap();
        assert_eq!(hash.len(), 64); // SHA-256 produces 32 bytes = 64 hex chars
    }

    #[test]
    fn test_verify_hash() {
        let data = b"test data";
        let computed = hash(data, HashAlgorithm::Blake3).unwrap();
        assert!(verify_hash(data, &computed, HashAlgorithm::Blake3).unwrap());
    }
}
