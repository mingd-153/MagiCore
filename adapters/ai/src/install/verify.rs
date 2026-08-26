//! Model checksum verification (BLAKE3).

use mgc_types::{MgError, MgResult};
use std::path::Path;

/// Verify model file checksum (BLAKE3 hex)
pub fn verify_model_checksum(path: &Path, expected_checksum: &str) -> MgResult<bool> {
    if !path.exists() {
        return Err(MgError::Other(format!(
            "Model file not found: {}",
            path.display()
        )));
    }

    let content = std::fs::read(path)?;
    let computed = compute_hash(&content);

    if computed.eq_ignore_ascii_case(expected_checksum) {
        Ok(true)
    } else {
        Err(MgError::Other(format!(
            "Checksum mismatch: expected {}, got {}",
            expected_checksum, computed
        )))
    }
}

/// Compute BLAKE3 hex digest
fn compute_hash(data: &[u8]) -> String {
    let hash = mgc_crypto::Blake3Hasher::hash_bytes(data);
    hash.to_hex()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn test_verify_valid_checksum() {
        let tmp = tmp();
        let model = tmp.path().join("model.bin");
        std::fs::write(&model, b"test data").unwrap();

        let hash = mgc_crypto::Blake3Hasher::hash_bytes(b"test data");
        let checksum = hash.to_hex();

        let result = verify_model_checksum(&model, &checksum);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_verify_invalid_checksum() {
        let tmp = tmp();
        let model = tmp.path().join("model.bin");
        std::fs::write(&model, b"test data").unwrap();

        let wrong = "deadbeef";
        let result = verify_model_checksum(&model, wrong);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_missing_file() {
        let tmp = tmp();
        let missing = tmp.path().join("missing.bin");

        let result = verify_model_checksum(&missing, "abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_compute_hash() {
        let hash = compute_hash(b"hello");
        assert_eq!(hash.len(), 64); // BLAKE3 = 32 bytes = 64 hex chars
    }
}
