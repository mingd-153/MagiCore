//! Integrity verification (SRI hashes)
//! Verify tính toàn vẹn (SRI hashes)

use crate::blake3_signer::{Blake3Hash, Blake3Hasher};
use crate::{CryptoError, CryptoResult};

/// SRI (Subresource Integrity) hash — SRI hash
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SriHash {
    /// Algorithm (sha256, sha512, blake3) — Thuật toán
    pub algorithm: String,
    /// Hash value (base64) — Giá trị hash (base64)
    pub hash: String,
}

impl SriHash {
    /// Parse SRI hash from string (e.g., "blake3-abc123...")
    /// Parse SRI hash từ chuỗi
    pub fn parse(s: &str) -> CryptoResult<Self> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 2 {
            return Err(CryptoError::Blake3Failed(format!(
                "invalid SRI format: {}",
                s
            )));
        }
        Ok(SriHash {
            algorithm: parts[0].to_string(),
            hash: parts[1].to_string(),
        })
    }

    /// Create BLAKE3 SRI hash — Tạo BLAKE3 SRI hash
    pub fn from_blake3(hash: &Blake3Hash) -> Self {
        SriHash {
            algorithm: "blake3".to_string(),
            hash: hash.to_base64(),
        }
    }
}

// A9 FIX: Implement Display instead of inherent to_string()
impl std::fmt::Display for SriHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.algorithm, self.hash)
    }
}

/// Integrity verifier — Integrity verifier
pub struct IntegrityVerifier;

impl IntegrityVerifier {
    /// Verify data against SRI hash — Verify dữ liệu với SRI hash
    pub fn verify(data: &[u8], sri: &SriHash) -> CryptoResult<()> {
        match sri.algorithm.as_str() {
            "blake3" => {
                let actual = Blake3Hasher::hash_bytes(data);
                let expected = Blake3Hash::from_base64(&sri.hash)?;
                if actual == expected {
                    Ok(())
                } else {
                    Err(CryptoError::VerificationFailed(format!(
                        "BLAKE3 hash mismatch: expected {}, got {}",
                        expected, actual
                    )))
                }
            }
            "sha256" | "sha512" => Err(CryptoError::Blake3Failed(format!(
                "algorithm not supported: {} (use blake3)",
                sri.algorithm
            ))),
            _ => Err(CryptoError::Blake3Failed(format!(
                "unknown algorithm: {}",
                sri.algorithm
            ))),
        }
    }

    /// Verify file against SRI hash — Verify file với SRI hash
    pub fn verify_file(path: &std::path::Path, sri: &SriHash) -> CryptoResult<()> {
        let data = std::fs::read(path)?;
        Self::verify(&data, sri)
    }

    /// Compute SRI hash for data — Tính SRI hash cho dữ liệu
    pub fn compute(data: &[u8]) -> SriHash {
        let hash = Blake3Hasher::hash_bytes(data);
        SriHash::from_blake3(&hash)
    }

    /// Compute SRI hash for file — Tính SRI hash cho file
    pub fn compute_file(path: &std::path::Path) -> CryptoResult<SriHash> {
        let hash = Blake3Hasher::hash_file(path)?;
        Ok(SriHash::from_blake3(&hash))
    }
}
