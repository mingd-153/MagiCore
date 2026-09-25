//! Lockfile verification and tamper detection
//! Xác minh lockfile và phát hiện tamper

use crate::ecosystem_tag::EcosystemTag;
use crate::schema::SOURCE_KIND_DELEGATED_TOOL;
use crate::{Lockfile, LockfileError, LockfileResult};
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
    /// Signature is valid, but its signer is not in the project's trust roots.
    /// Chữ ký hợp lệ nhưng signer không nằm trong trust roots của project.
    UntrustedKey(String),
}

/// Verify lockfile integrity and signature — Verify tính toàn vẹn và chữ ký lockfile
pub fn verify_lockfile(lockfile_path: &Path) -> LockfileResult<VerificationStatus> {
    verify_lockfile_inner(lockfile_path, None)
}

/// Verify a v3 lock and require its signer fingerprint to appear in the
/// caller-supplied project trust roots. Cryptographic validity alone is not
/// proof that the project trusts the signer.
/// (Ngoài xác minh mật mã, yêu cầu signer phải có trong trust roots.)
pub fn verify_lockfile_with_trust(
    lockfile_path: &Path,
    trust_keys: &[String],
) -> LockfileResult<VerificationStatus> {
    verify_lockfile_inner(lockfile_path, Some(trust_keys))
}

fn verify_lockfile_inner(
    lockfile_path: &Path,
    trust_keys: Option<&[String]>,
) -> LockfileResult<VerificationStatus> {
    let sig_path = lockfile_path.with_extension("lock.sig");

    // Check if signature file exists
    if !sig_path.exists() {
        return Ok(VerificationStatus::Unsigned);
    }

    // Try to load and verify
    match crate::parser::load_and_verify_lockfile(lockfile_path, &sig_path) {
        Ok(lockfile) => {
            let signer_id = lockfile
                .metadata
                .signer
                .as_ref()
                .map(|signer| signer.key_id.as_str())
                .unwrap_or_default();
            if trust_keys.is_some_and(|keys| !keys.iter().any(|key| key == signer_id)) {
                Ok(VerificationStatus::UntrustedKey(signer_id.to_string()))
            } else {
                Ok(VerificationStatus::Valid)
            }
        }
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

/// Ownership-completeness gate (v3): every package whose ecosystem is not
/// `other` must carry a registry source OR a `delegated-tool` provenance —
/// otherwise the lock cannot explain where the pin came from. Kept as a
/// standalone check (NOT folded into `verify_lockfile`) so signature
/// verification semantics stay unchanged; install/promotion flows call this
/// explicitly.
/// Cổng kiểm tra tính đầy đủ quyền sở hữu (v3): mọi package có ecosystem
/// khác `other` phải mang registry nguồn HOẶC provenance `delegated-tool`
/// — nếu không lock không giải thích được pin từ đâu ra. Tách thành hàm độc
/// lập (KHÔNG ghép vào `verify_lockfile`) để ngữ nghĩa verify chữ ký giữ
/// nguyên; luồng install/promotion sẽ gọi tường minh.
pub fn verify_ownership_completeness(lockfile: &Lockfile) -> LockfileResult<()> {
    for pkg in &lockfile.packages {
        if pkg.ecosystem == EcosystemTag::Other {
            // `other` = v2 imports / unclassified data — exempt (v2 files
            // carry no ecosystem data at all).
            // `other` = import v2 / dữ liệu chưa phân loại — được miễn
            // (file v2 không mang dữ liệu ecosystem).
            continue;
        }

        let has_registry = pkg.registry.is_some();
        let has_delegated_provenance = pkg
            .provenance
            .as_ref()
            .is_some_and(|p| p.source_kind == SOURCE_KIND_DELEGATED_TOOL);

        if !has_registry && !has_delegated_provenance {
            return Err(LockfileError::IncompleteProvenance(format!(
                "package '{}@{}' (ecosystem {}) has neither a registry nor delegated-tool provenance",
                pkg.name, pkg.version, pkg.ecosystem
            )));
        }
    }
    Ok(())
}

/// Get verification status message — Lấy message trạng thái verify
pub fn verification_status_message(status: &VerificationStatus) -> String {
    match status {
        VerificationStatus::Valid => "✓ Lockfile signature valid".to_string(),
        VerificationStatus::Unsigned => {
            "WARN: Lockfile not signed — run 'mgc trust sign'".to_string()
        }
        VerificationStatus::Tampered(msg) => format!("✗ Lockfile tampered: {}", msg),
        VerificationStatus::InvalidSignature(msg) => format!("✗ Invalid signature: {}", msg),
        VerificationStatus::UntrustedKey(key_id) => {
            format!("WARN: Lockfile signer is not trusted: {key_id}")
        }
    }
}
