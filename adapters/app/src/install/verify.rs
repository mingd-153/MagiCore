//! `install/verify.rs` — Integrity verification for mobile app packages.
//! Verifies pubspec.lock (Flutter), gradle.lockfile (Kotlin), Package.resolved (Swift), Podfile.lock (ObjC).

use mgc_types::{MgError, MgResult};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Verify Flutter pubspec.lock integrity.
/// Kiểm tra integrity của pubspec.lock.
pub fn verify_pubspec_lock(project_root: &Path) -> MgResult<()> {
    let lock_path = project_root.join("pubspec.lock");

    if !lock_path.exists() {
        return Err(MgError::Other(
            "pubspec.lock not found (run flutter pub get first)".to_string(),
        ));
    }

    // Issue #13: parse pubspec.lock YAML and verify package checksums
    // pubspec.lock contains resolved: version and archive_sha256
    Ok(())
}

/// Verify Gradle lockfile integrity.
/// Kiểm tra integrity của gradle.lockfile.
pub fn verify_gradle_lockfile(project_root: &Path) -> MgResult<()> {
    let lock_path = project_root.join("gradle.lockfile");

    if !lock_path.exists() {
        // Gradle lockfile is optional, not all projects use it
        return Ok(());
    }

    // Issue #13: parse gradle.lockfile format
    // Format: artifact=group:name:version=sha256:hash
    Ok(())
}

/// Verify Swift Package.resolved integrity.
/// Kiểm tra integrity của Package.resolved.
pub fn verify_package_resolved(project_root: &Path) -> MgResult<()> {
    let resolved_path = project_root.join("Package.resolved");

    if !resolved_path.exists() {
        return Err(MgError::Other(
            "Package.resolved not found (run swift package resolve first)".to_string(),
        ));
    }

    // Issue #13: parse Package.resolved JSON
    // Contains: state.revision (git commit hash) or state.version
    Ok(())
}

/// Verify CocoaPods Podfile.lock integrity.
/// Kiểm tra integrity của Podfile.lock.
pub fn verify_podfile_lock(project_root: &Path) -> MgResult<()> {
    let lock_path = project_root.join("Podfile.lock");

    if !lock_path.exists() {
        return Err(MgError::Other(
            "Podfile.lock not found (run pod install first)".to_string(),
        ));
    }

    // Issue #13: parse Podfile.lock YAML
    // Contains PODS section with version locks
    Ok(())
}

/// Verify package file integrity with SHA-256.
/// Kiểm tra integrity file package với SHA-256.
pub fn verify_package_file(file_path: &Path, expected_hash: Option<&str>) -> MgResult<()> {
    if expected_hash.is_none() {
        // No hash available: warn but allow (permissive mode)
        eprintln!(
            "WARNING: No integrity hash for package '{}'",
            file_path.display()
        );
        return Ok(());
    }

    let expected = expected_hash.expect("expected_hash checked non-None above");
    let actual = compute_sha256_file(file_path)?;

    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(MgError::Other(format!(
            "integrity mismatch for '{}': expected {}, got {}",
            file_path.display(),
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

/// Verify Flutter package archive SHA-256 from pubspec.lock.
/// Kiểm tra SHA-256 archive Flutter package từ pubspec.lock.
pub fn verify_flutter_archive_sha256(archive_path: &Path, expected_sha256: &str) -> MgResult<()> {
    verify_package_file(archive_path, Some(expected_sha256))
}
