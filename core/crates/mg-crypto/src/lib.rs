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
        HashAlgorithm::Sha256 | HashAlgorithm::Blake3 => {
            let mut hasher = Sha256::new();
            hasher.update(data);
            hex::encode(hasher.finalize())
        }
    })
}

pub fn verify_hash(data: &[u8], expected: &str, algorithm: HashAlgorithm) -> Result<bool> {
    Ok(hash(data, algorithm)? == expected)
}
