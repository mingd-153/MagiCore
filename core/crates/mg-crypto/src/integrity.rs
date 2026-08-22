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

    /// Convert to SRI string — Chuyển sang chuỗi SRI
    pub fn to_string(&self) -> String {
        format!("{}-{}", self.algorithm, self.hash)
    }

    /// Create BLAKE3 SRI hash — Tạo BLAKE3 SRI hash
    pub fn from_blake3(hash: &Blake3Hash) -> Self {
        SriHash {
            algorithm: "blake3".to_string(),
            hash: hash.to_base64(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_sri_parse() {
        let sri = SriHash::parse("blake3-YWJjMTIz").unwrap();
        assert_eq!(sri.algorithm, "blake3");
        assert_eq!(sri.hash, "YWJjMTIz");
    }

    #[test]
    fn test_sri_to_string() {
        let sri = SriHash {
            algorithm: "blake3".to_string(),
            hash: "abc123".to_string(),
        };
        assert_eq!(sri.to_string(), "blake3-abc123");
    }

    #[test]
    fn test_compute_and_verify() {
        let data = b"hello world";
        let sri = IntegrityVerifier::compute(data);

        assert_eq!(sri.algorithm, "blake3");
        IntegrityVerifier::verify(data, &sri).unwrap();
    }

    #[test]
    fn test_verify_mismatch() {
        let data = b"hello world";
        let sri = IntegrityVerifier::compute(data);

        let wrong_data = b"wrong data";
        assert!(IntegrityVerifier::verify(wrong_data, &sri).is_err());
    }

    #[test]
    fn test_compute_file() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        tmpfile.write_all(b"test content").unwrap();
        tmpfile.flush().unwrap();

        let sri = IntegrityVerifier::compute_file(tmpfile.path()).unwrap();
        assert_eq!(sri.algorithm, "blake3");

        IntegrityVerifier::verify_file(tmpfile.path(), &sri).unwrap();
    }

    #[test]
    fn test_unsupported_algorithm() {
        let sri = SriHash {
            algorithm: "sha256".to_string(),
            hash: "abc".to_string(),
        };
        assert!(IntegrityVerifier::verify(b"data", &sri).is_err());
    }
}
