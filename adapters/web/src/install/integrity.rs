//! `install/integrity.rs` — Tarball integrity verification (SRI checksums).
//! Tách từ download.rs để tách concern verification khỏi fetch/pipeline logic.

use base64::Engine;
use mgc_types::adapter::ResolvedPackage;
use mgc_types::{MgError, MgResult};

use crate::lockfile::{compute_sha512_b64, compute_tarball_integrity, strict_integrity_enforced};

/// Compute SHA-256 hash of bytes and return as base64 string (SRI format).
/// Hàm helper cho integrity verification — tính hash của tarball bytes.
pub fn compute_sha256_b64_str(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

/// Verify tarball integrity against package SRI field.
/// Wrapper cho verify_sri_integrity — entry point chính.
pub fn verify_tarball_integrity(pkg: &ResolvedPackage, bytes: &[u8]) -> MgResult<()> {
    verify_sri_integrity(pkg, bytes)
}

/// Prepare package for cache by computing integrity if missing, then verify.
/// Chuẩn bị package trước khi cache — compute integrity nếu thiếu, rồi verify.
pub fn prepare_verified_tarball_for_cache(pkg: &mut ResolvedPackage, bytes: &[u8]) -> MgResult<()> {
    if pkg.integrity.is_empty() {
        pkg.integrity = compute_tarball_integrity(bytes);
    }
    verify_tarball_integrity(pkg, bytes)
}

/// Verify SRI (Subresource Integrity) checksums for tarball bytes.
/// Kiểm tra integrity field của package khớp với bytes thật.
/// Hỗ trợ sha256/sha512 (strong), cảnh báo sha1/md5 (weak).
pub fn verify_sri_integrity(pkg: &ResolvedPackage, bytes: &[u8]) -> MgResult<()> {
    // Empty integrity: strict mode fail, permissive mode warn
    // Integrity trống: strict mode fail, permissive mode cho qua
    if pkg.integrity.is_empty() {
        if strict_integrity_enforced() {
            return Err(MgError::Other(format!(
                "strict integrity: '{}' has no SRI integrity field",
                pkg.id.name_str()
            )));
        }
        return Ok(());
    }

    let mut has_weak_algorithm = false;
    let mut has_strong_algorithm = false;

    // Parse SRI format: "sha512-base64 sha256-base64" (space separated)
    // Parse format SRI: "sha512-base64 sha256-base64" (cách nhau bởi space)
    for entry in pkg.integrity.split_whitespace() {
        let Some((algorithm, expected)) = entry.split_once('-') else {
            continue;
        };

        // Reject weak algorithms in strict mode, warn in permissive
        // Reject thuật toán yếu ở strict mode, cảnh báo ở permissive
        if matches!(algorithm, "sha1" | "md5") {
            has_weak_algorithm = true;
            if strict_integrity_enforced() {
                return Err(MgError::Other(format!(
                    "strict integrity: '{}' uses weak hash algorithm '{}' (only sha256/sha512 allowed)",
                    pkg.id.name_str(),
                    algorithm
                )));
            }
            eprintln!(
                "WARNING: Package '{}' uses weak hash algorithm '{}', consider updating",
                pkg.id.name_str(),
                algorithm
            );
            continue;
        }

        // Compute actual hash for strong algorithms
        // Tính hash thật cho thuật toán mạnh
        let actual = match algorithm {
            "sha256" => {
                has_strong_algorithm = true;
                compute_sha256_b64_str(bytes)
            }
            "sha512" => {
                has_strong_algorithm = true;
                compute_sha512_b64(bytes)
            }
            _ => {
                eprintln!(
                    "WARNING: Package '{}' uses unknown hash algorithm '{}'",
                    pkg.id.name_str(),
                    algorithm
                );
                continue;
            }
        };

        // Match found: integrity OK
        // Tìm thấy match: integrity OK
        if actual == expected {
            return Ok(());
        }
    }

    // Only weak algorithms present: fail
    // Chỉ có thuật toán yếu: fail
    if has_weak_algorithm && !has_strong_algorithm {
        return Err(MgError::Other(format!(
            "integrity check failed for '{}': only weak algorithms present (sha1/md5)",
            pkg.id.name_str()
        )));
    }

    // None of the hashes matched
    // Không hash nào match
    Err(MgError::Other(format!(
        "integrity mismatch for '{}': none of the SRI entries matched",
        pkg.id.name_str()
    )))
}
