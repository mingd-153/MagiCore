//! `install/verify.rs` — Integrity verification for Rust/Python packages.
//! Tương tự web install/integrity.rs nhưng cho Rust crates và Python wheels.

use mgc_types::{MgError, MgResult, PackageId};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Refuse to claim artifact integrity from Cargo.lock alone.
/// Từ chối tuyên bố đã xác minh artifact chỉ dựa trên Cargo.lock.
///
/// A lockfile records expected checksums, but this helper has no mapping from
/// locked packages to downloaded crate archives. The native resolver/store
/// must verify those archives; file presence is not integrity verification.
/// Lockfile ghi checksum kỳ vọng nhưng helper này không ánh xạ package đã khóa
/// tới archive crate đã tải. Resolver/store native phải xác minh archive;
/// sự tồn tại của file không phải là xác minh integrity.
pub fn verify_cargo_lock(project_root: &Path) -> MgResult<()> {
    let lock_path = project_root.join("Cargo.lock");

    if !lock_path.exists() {
        return Err(MgError::Other(
            "Cargo.lock not found; MagiCore cannot verify this project until its native resolver writes the lockfile".to_string(),
        ));
    }

    Err(MgError::Other(
        "cannot verify crate artifact integrity from Cargo.lock alone; use the native resolver/store verification".to_string(),
    ))
}

/// Verify Python package integrity (PEP 503 hash).
/// Kiểm tra integrity package Python (PEP 503 hash).
///
/// PyPI provides SHA-256 hashes in the simple API index.
/// PyPI cung cấp SHA-256 hash trong simple API index.
pub fn verify_python_package(package_path: &Path, expected_hash: Option<&str>) -> MgResult<()> {
    if expected_hash.is_none() {
        return Err(MgError::Other(format!(
            "cannot verify Python package '{}': no expected SHA-256 hash was provided",
            package_path.display()
        )));
    }

    let Some(expected) = expected_hash else {
        // Guarded above, but re-check instead of unwrapping: a refactor
        // that moves the guard must fail closed, not panic.
        return Err(MgError::Other(format!(
            "cannot verify Python package '{}': expected hash vanished after the availability check",
            package_path.display()
        )));
    };

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

/// Refuse to claim wheel RECORD integrity until the RECORD is actually checked.
/// Từ chối tuyên bố đã xác minh RECORD cho tới khi RECORD được kiểm tra thật.
///
/// Wheels contain a RECORD file listing all files with SHA-256 hashes.
/// Wheels chứa file RECORD liệt kê mọi file với SHA-256 hash.
pub fn verify_wheel_record(wheel_path: &Path) -> MgResult<()> {
    if wheel_path.extension().and_then(|s| s.to_str()) == Some("whl") {
        Err(MgError::Other(format!(
            "cannot verify wheel '{}' RECORD: RECORD verification is not implemented",
            wheel_path.display()
        )))
    } else {
        Err(MgError::Other(
            "not a valid wheel file (must have .whl extension)".to_string(),
        ))
    }
}
