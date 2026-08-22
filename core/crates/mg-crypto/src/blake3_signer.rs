//! BLAKE3 hasher with SIMD acceleration
//! BLAKE3 hasher với tăng tốc SIMD (AVX2/NEON)

use crate::{CryptoError, CryptoResult};
use std::io::Read;
use std::path::Path;

/// BLAKE3 hash wrapper — BLAKE3 hash wrapper
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blake3Hash(pub [u8; 32]);

impl Blake3Hash {
    /// Convert to hex string — Chuyển sang chuỗi hex
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }

    /// Parse from hex string — Parse từ chuỗi hex
    pub fn from_hex(s: &str) -> CryptoResult<Self> {
        let bytes = hex::decode(s)
            .map_err(|e| CryptoError::Blake3Failed(format!("invalid hex: {}", e)))?;
        if bytes.len() != 32 {
            return Err(CryptoError::Blake3Failed(format!(
                "expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Blake3Hash(arr))
    }

    /// Convert to base64 — Chuyển sang base64
    pub fn to_base64(&self) -> String {
        base64::encode(&self.0)
    }

    /// Parse from base64 — Parse từ base64
    pub fn from_base64(s: &str) -> CryptoResult<Self> {
        let bytes = base64::decode(s)?;
        if bytes.len() != 32 {
            return Err(CryptoError::Blake3Failed(format!(
                "expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Blake3Hash(arr))
    }
}

impl std::fmt::Display for Blake3Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// BLAKE3 hasher — BLAKE3 hasher
pub struct Blake3Hasher;

impl Blake3Hasher {
    /// Hash bytes with SIMD acceleration — Hash bytes với tăng tốc SIMD
    pub fn hash_bytes(data: &[u8]) -> Blake3Hash {
        let hash = blake3::hash(data);
        Blake3Hash(*hash.as_bytes())
    }

    /// Hash string — Hash chuỗi
    pub fn hash_string(s: &str) -> Blake3Hash {
        Self::hash_bytes(s.as_bytes())
    }

    /// Hash file with streaming (no full load to RAM) — Hash file với streaming (không load hết vào RAM)
    pub fn hash_file(path: &Path) -> CryptoResult<Blake3Hash> {
        let mut file = std::fs::File::open(path)?;
        let mut hasher = blake3::Hasher::new();
        let mut buffer = vec![0u8; 8192]; // 8KB buffer — buffer 8KB

        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        let hash = hasher.finalize();
        Ok(Blake3Hash(*hash.as_bytes()))
    }

    /// Hash file with memory mapping (fast for large files) — Hash file với memory mapping (nhanh cho file lớn)
    #[cfg(not(target_env = "msvc"))]
    pub fn hash_file_mmap(path: &Path) -> CryptoResult<Blake3Hash> {
        let file = std::fs::File::open(path)?;
        // SAFETY: We trust the file descriptor is valid
        // AN TOÀN: Tin file descriptor hợp lệ
        let mmap = unsafe {
            memmap2::Mmap::map(&file)
                .map_err(|e| CryptoError::Blake3Failed(format!("mmap failed: {}", e)))?
        };
        Ok(Self::hash_bytes(&mmap))
    }

    /// Verify hash matches expected — Verify hash khớp với expected
    pub fn verify(data: &[u8], expected: &Blake3Hash) -> bool {
        let actual = Self::hash_bytes(data);
        actual == *expected
    }
}

// Import hex crate for encoding
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, String> {
        if s.len() % 2 != 0 {
            return Err("odd length".to_string());
        }
        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16)
                    .map_err(|e| format!("invalid hex: {}", e))
            })
            .collect()
    }
}

// Import base64 encode/decode
mod base64 {
    pub fn encode(bytes: &[u8]) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, crate::CryptoError> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(|e| crate::CryptoError::Blake3Failed(format!("base64 decode: {}", e)))
    }
}

#[cfg(not(target_env = "msvc"))]
use memmap2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_empty() {
        let hash = Blake3Hasher::hash_bytes(b"");
        assert_eq!(hash.0.len(), 32);
    }

    #[test]
    fn test_hash_hello_world() {
        let hash = Blake3Hasher::hash_string("hello world");
        let expected = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";
        assert_eq!(hash.to_hex(), expected);
    }

    #[test]
    fn test_hash_verify() {
        let data = b"test data";
        let hash = Blake3Hasher::hash_bytes(data);
        assert!(Blake3Hasher::verify(data, &hash));
        assert!(!Blake3Hasher::verify(b"wrong data", &hash));
    }

    #[test]
    fn test_hash_hex_roundtrip() {
        let hash = Blake3Hasher::hash_string("test");
        let hex = hash.to_hex();
        let parsed = Blake3Hash::from_hex(&hex).unwrap();
        assert_eq!(hash, parsed);
    }

    #[test]
    fn test_hash_base64_roundtrip() {
        let hash = Blake3Hasher::hash_string("test");
        let b64 = hash.to_base64();
        let parsed = Blake3Hash::from_base64(&b64).unwrap();
        assert_eq!(hash, parsed);
    }
}
