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
#[path = "test/verify_tests.rs"]
mod tests;
