//! Lockfile verification and tamper detection
//! Xác minh lockfile và phát hiện tamper

use crate::{LockfileError, LockfileResult};
use std::path::Path;

/// Verification result — Kết quả verify
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationStatus {
    /// Lockfile is valid and signed — Lockfile hợp lệ và đã ký
    Valid,
    /// Lockfile is unsigned (warning) — Lockfile chưa ký (cảnh báo)
    Unsigned,
    /// Lockfile has been tampered with — Lockfile đã bị tamper
    Tampered(String),
    /// Signature is invalid — Chữ ký không hợp lệ
    InvalidSignature(String),
}

/// Verify lockfile integrity and signature — Verify tính toàn vẹn và chữ ký lockfile
pub fn verify_lockfile(lockfile_path: &Path) -> LockfileResult<VerificationStatus> {
    let sig_path = lockfile_path.with_extension("lock.sig");

    // Check if signature file exists
    if !sig_path.exists() {
        return Ok(VerificationStatus::Unsigned);
    }

    // Try to load and verify
    match crate::parser::load_and_verify_lockfile(lockfile_path, &sig_path) {
        Ok(_) => Ok(VerificationStatus::Valid),
        Err(LockfileError::TamperedLockfile(msg)) => Ok(VerificationStatus::Tampered(msg)),
        Err(LockfileError::VerificationFailed(msg)) => {
            Ok(VerificationStatus::InvalidSignature(msg))
        }
        Err(e) => Err(e),
    }
}

/// Quick check if lockfile is tampered (without full verification) — Kiểm tra nhanh lockfile bị tamper
pub fn is_lockfile_tampered(lockfile_path: &Path) -> LockfileResult<bool> {
    let status = verify_lockfile(lockfile_path)?;
    Ok(matches!(status, VerificationStatus::Tampered(_)))
}

/// Get verification status message — Lấy message trạng thái verify
pub fn verification_status_message(status: &VerificationStatus) -> String {
    match status {
        VerificationStatus::Valid => "✓ Lockfile signature valid".to_string(),
        VerificationStatus::Unsigned => "⚠ Lockfile not signed — run 'mgc trust sign'".to_string(),
        VerificationStatus::Tampered(msg) => format!("✗ Lockfile tampered: {}", msg),
        VerificationStatus::InvalidSignature(msg) => format!("✗ Invalid signature: {}", msg),
    }
}
