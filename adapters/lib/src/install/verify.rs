//! `install/verify.rs` — Integrity verification for Rust/Python packages.
//! Tương tự web install/integrity.rs nhưng cho Rust crates và Python wheels.

use mgc_types::{MgError, MgResult, PackageId};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Verify Cargo.lock integrity.
/// Kiểm tra integrity của Cargo.lock.
///
/// Cargo.lock contains checksums in the `checksum` field for each dependency.
/// Cargo.lock chứa checksum trong field `checksum` cho mỗi dependency.
pub fn verify_cargo_lock(project_root: &Path) -> MgResult<()> {
    let lock_path = project_root.join("Cargo.lock");

    if !lock_path.exists() {
        return Err(MgError::Other(
            "Cargo.lock not found (run cargo fetch first)".to_string(),
        ));
    }

    // TODO: parse Cargo.lock TOML and verify checksums against downloaded crates
    // TODO: parse Cargo.lock TOML và verify checksum với crates đã tải
    // For now, trust cargo's internal verification
    // Hiện tại tin cargo verification nội bộ
    Ok(())
}

/// Verify Python package integrity (PEP 503 hash).
/// Kiểm tra integrity package Python (PEP 503 hash).
///
/// PyPI provides SHA-256 hashes in the simple API index.
/// PyPI cung cấp SHA-256 hash trong simple API index.
pub fn verify_python_package(package_path: &Path, expected_hash: Option<&str>) -> MgResult<()> {
    if expected_hash.is_none() {
        // No hash available: warn but allow (permissive mode)
        // Không có hash: cảnh báo nhưng cho qua (chế độ permissive)
        eprintln!(
            "WARNING: No integrity hash for Python package '{}'",
            package_path.display()
        );
        return Ok(());
    }

    let expected = expected_hash.expect("expected_hash checked non-None above");

    // Compute SHA-256 of package file
    // Tính SHA-256 của package file
    let actual = compute_sha256_file(package_path)?;

    // Compare (case-insensitive hex)
    // So sánh (hex không phân biệt hoa thường)
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(MgError::Other(format!(
            "integrity mismatch for '{}': expected {}, got {}",
            package_path.display(),
            expected,
            actual
        )))
    }
}

/// Compute SHA-256 hash of a file.
/// Tính SHA-256 hash của file.
fn compute_sha256_file(path: &Path) -> MgResult<String> {
    let bytes =
        std::fs::read(path).map_err(|e| MgError::Other(format!("failed to read file: {}", e)))?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

/// Verify Rust crate checksum from Cargo.lock.
/// Kiểm tra checksum Rust crate từ Cargo.lock.
///
/// Cargo.lock format: checksum = "sha256-hex"
pub fn verify_crate_checksum(
    crate_path: &Path,
    package_id: &PackageId,
    expected_checksum: &str,
) -> MgResult<()> {
    let actual = compute_sha256_file(crate_path)?;

    // Cargo uses "sha256-hex" format, strip prefix if present
    // Cargo dùng format "sha256-hex", bỏ prefix nếu có
    let expected = expected_checksum
        .strip_prefix("sha256-")
        .unwrap_or(expected_checksum);

    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(MgError::Other(format!(
            "crate checksum mismatch for {}: expected {}, got {}",
            package_id, expected, actual
        )))
    }
}

/// Verify Python wheel RECORD file integrity.
/// Kiểm tra integrity file RECORD của Python wheel.
///
/// Wheels contain a RECORD file listing all files with SHA-256 hashes.
/// Wheels chứa file RECORD liệt kê mọi file với SHA-256 hash.
pub fn verify_wheel_record(wheel_path: &Path) -> MgResult<()> {
    // TODO: extract wheel and verify RECORD file
    // TODO: extract wheel và verify file RECORD
    // This requires zip extraction and RECORD parsing
    // Cần zip extraction và RECORD parsing

    // For now, trust wheel signature if present
    // Hiện tại tin wheel signature nếu có
    if wheel_path.extension().and_then(|s| s.to_str()) == Some("whl") {
        Ok(())
    } else {
        Err(MgError::Other(
            "not a valid wheel file (must have .whl extension)".to_string(),
        ))
    }
}
