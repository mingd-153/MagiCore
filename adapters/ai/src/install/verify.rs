//! Compatibility entry point for streaming model checksum verification.

use mgc_types::MgResult;
use std::path::Path;

/// Verify a model checksum (`sha256:`/`blake3:` prefix; bare hex means BLAKE3).
pub fn verify_model_checksum(path: &Path, expected_checksum: &str) -> MgResult<bool> {
    crate::install::download::verify_file_checksum(path, expected_checksum)?;
    Ok(true)
}

#[cfg(test)]
#[path = "test/verify_tests.rs"]
mod tests;
