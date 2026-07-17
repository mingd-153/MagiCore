pub mod checksum;
pub mod integrity;

use anyhow::Result;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum HashAlgorithm {
    Sha256,
    Blake3,
}

pub fn hash(data: &[u8], algorithm: HashAlgorithm) -> Result<String> {
    Ok(match algorithm {
        HashAlgorithm::Sha256 => {
            let mut hasher = Sha256::new();
            hasher.update(data);
            hex::encode(hasher.finalize())
        }
        HashAlgorithm::Blake3 => hex::encode(blake3::hash(data).as_bytes()),
    })
}

pub fn verify_hash(data: &[u8], expected: &str, algorithm: HashAlgorithm) -> Result<bool> {
    Ok(hash(data, algorithm)? == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &[u8] = b"hello";
    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    #[test]
    fn test_hash_sha256_known_vector() {
        let got = hash(HELLO, HashAlgorithm::Sha256).unwrap();
        assert_eq!(got, HELLO_SHA256);
    }

    #[test]
    fn test_hash_sha256_different_inputs_differ() {
        let a = hash(b"abc", HashAlgorithm::Sha256).unwrap();
        let b = hash(b"xyz", HashAlgorithm::Sha256).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn test_verify_hash_match() {
        assert!(verify_hash(HELLO, HELLO_SHA256, HashAlgorithm::Sha256).unwrap());
    }

    #[test]
    fn test_verify_hash_mismatch() {
        assert!(!verify_hash(b"world", HELLO_SHA256, HashAlgorithm::Sha256).unwrap());
    }

    #[test]
    fn test_hash_blake3_known_vector() {
        let got = hash(HELLO, HashAlgorithm::Blake3).unwrap();
        assert_eq!(
            got,
            "ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f"
        );
    }

    #[test]
    fn test_hash_empty() {
        let got = hash(b"", HashAlgorithm::Sha256).unwrap();
        assert_eq!(
            got,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
