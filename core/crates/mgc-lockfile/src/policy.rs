//! Lock v4 trust policy + verification (design §3).
//! Chính sách tin cậy + xác minh lock v4.
//!
//! Principle (stated plainly so it cannot be overclaimed): a lone hash
//! is NOT anti-tamper — anyone who can rewrite the lock can recompute
//! it. Authenticity needs a signature verified against a trusted key.
//! READMEs, CLIs and docs must never call an unsigned lock
//! "zero-trust" or "tamper-proof".
//! (Nguyên tắc: hash đơn lẻ KHÔNG chống tamper. Chỉ chữ ký verify được
//! với key tin cậy mới cho authenticity.)

use std::path::Path;

use crate::canonical::{LockfileV4, payload_digest};
use crate::{LockfileError, LockfileResult};

/// Signature policy mode — Chế độ chính sách chữ ký.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockPolicyMode {
    /// Personal dev: unsigned locks pass silently.
    Off,
    /// Local default: unsigned/foreign-key locks pass with a warning.
    Warn,
    /// CI/release/`--frozen`: unsigned or foreign-key locks FAIL.
    Require,
}

impl LockPolicyMode {
    /// Parse `off` | `warn` | `require` (case-insensitive).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "warn" => Some(Self::Warn),
            "require" => Some(Self::Require),
            _ => None,
        }
    }

    /// Environment default: `CI=true` means `require`, else `warn`.
    /// (Mặc định theo môi trường: `CI=true` là `require`, còn lại `warn`.)
    pub fn environment_default() -> Self {
        let ci = std::env::var("CI")
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        if ci { Self::Require } else { Self::Warn }
    }
}

/// Resolve the effective policy: CLI flag → `MGC_LOCK_POLICY` env →
/// `mgc.toml [lock].policy` → environment default.
/// Thứ tự resolve: cờ CLI → env → `mgc.toml [lock]` → mặc định môi trường.
pub fn resolve_policy(flag: Option<&str>, project_root: Option<&Path>) -> LockPolicyMode {
    if let Some(mode) = flag.and_then(LockPolicyMode::parse) {
        return mode;
    }
    if let Ok(raw) = std::env::var("MGC_LOCK_POLICY")
        && let Some(mode) = LockPolicyMode::parse(&raw)
    {
        return mode;
    }
    if let Some(root) = project_root
        && let Ok(content) = std::fs::read_to_string(root.join("mgc.toml"))
        && let Ok(value) = toml::from_str::<toml::Value>(&content)
        && let Some(policy) = value
            .get("lock")
            .and_then(|l| l.get("policy"))
            .and_then(|p| p.as_str())
        && let Some(mode) = LockPolicyMode::parse(policy)
    {
        return mode;
    }
    LockPolicyMode::environment_default()
}

/// Structural verification report: digest math + signature math, NO
/// policy. Policy enforcement is a separate explicit step below.
/// Báo cáo verify cấu trúc: toán digest + chữ ký, KHÔNG policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4VerifyReport {
    /// Payload digest recomputed and equal to `metadata.lockfile_hash`.
    pub digest_ok: bool,
    /// A syntactically valid signature block is present.
    pub signed: bool,
    /// The signature verifies against the embedded public key.
    pub signature_ok: bool,
    /// Key id from the signature block, if any.
    pub key_id: Option<String>,
}

/// Verify digest + signature math (no trust decision).
/// Xác minh toán digest + chữ ký (không quyết định tin cậy).
pub fn verify_v4_math(lock: &LockfileV4) -> LockfileResult<V4VerifyReport> {
    let recomputed = payload_digest(&lock.payload());
    let digest_ok = recomputed == lock.metadata.lockfile_hash;
    let Some(signature) = lock.metadata.signature.as_ref() else {
        return Ok(V4VerifyReport {
            digest_ok,
            signed: false,
            signature_ok: false,
            key_id: None,
        });
    };
    if signature.algorithm != "ed25519" {
        return Err(LockfileError::VerificationFailed(format!(
            "unsupported signature algorithm '{}'",
            signature.algorithm
        )));
    }
    if signature.digest != lock.metadata.lockfile_hash {
        return Err(LockfileError::VerificationFailed(
            "signature digest does not match lockfile_hash".to_string(),
        ));
    }
    let public_key =
        mgc_crypto::ed25519_signer::Ed25519PublicKey::from_base64(&signature.public_key)?;
    let raw = signature
        .signature
        .strip_prefix("ed25519-")
        .unwrap_or(&signature.signature);
    let parsed = mgc_crypto::ed25519_signer::Ed25519Signature::from_base64(raw)?;
    let digest_bytes = blake3_digest_bytes(&signature.digest)?;
    let signature_ok =
        mgc_crypto::ed25519_signer::verify_signature(&public_key, &digest_bytes, &parsed).is_ok();
    Ok(V4VerifyReport {
        digest_ok,
        signed: true,
        signature_ok,
        key_id: Some(signature.key_id.clone()),
    })
}

/// Decode a `blake3-<base64>` digest to raw bytes.
/// Giải digest `blake3-<base64>` ra byte thô.
fn blake3_digest_bytes(digest: &str) -> LockfileResult<Vec<u8>> {
    use base64::Engine;
    let raw = digest
        .strip_prefix("blake3-")
        .ok_or_else(|| LockfileError::VerificationFailed(format!("malformed digest '{digest}'")))?;
    base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|e| LockfileError::VerificationFailed(format!("malformed digest: {e}")))
}

/// Enforce the signature policy on a verified report.
/// - tampered digest → ALWAYS fail (any mode).
/// - unsigned + require → fail; unsigned + warn → caller warns (Ok);
///   unsigned + off → Ok silently.
/// - bad signature → ALWAYS fail.
/// - foreign key + require → fail; foreign key + warn/off → Ok.
///
/// Áp chính sách chữ ký lên báo cáo đã verify.
pub fn enforce_policy(
    report: &V4VerifyReport,
    policy: LockPolicyMode,
    trust_keys: &[String],
) -> LockfileResult<()> {
    if !report.digest_ok {
        return Err(LockfileError::TamperedLockfile(
            "payload digest does not match metadata.lockfile_hash".to_string(),
        ));
    }
    if !report.signed {
        return match policy {
            LockPolicyMode::Require => Err(LockfileError::VerificationFailed(
                "unsigned lockfile rejected by policy `require`".to_string(),
            )),
            LockPolicyMode::Warn | LockPolicyMode::Off => Ok(()),
        };
    }
    if !report.signature_ok {
        return Err(LockfileError::InvalidSignatureFile(
            "ed25519 signature does not verify".to_string(),
        ));
    }
    let trusted = report
        .key_id
        .as_ref()
        .is_some_and(|id| trust_keys.iter().any(|k| k == id));
    if !trusted && policy == LockPolicyMode::Require {
        return Err(LockfileError::UntrustedKey(
            report.key_id.clone().unwrap_or_default(),
        ));
    }
    Ok(())
}

/// Verify a v4 lockfile on disk end to end (parse + math + policy).
/// Xác minh lockfile v4 trên đĩa trọn vẹn (parse + toán + policy).
pub fn verify_v4_file(
    path: &Path,
    policy: LockPolicyMode,
    trust_keys: &[String],
) -> LockfileResult<V4VerifyReport> {
    let content = std::fs::read_to_string(path)?;
    let lock: LockfileV4 = toml::from_str(&content)
        .map_err(|e| LockfileError::ParseError(format!("v4 TOML parse failed: {e}")))?;
    if lock.version != crate::v4::LOCKFILE_SCHEMA_V4 {
        return Err(LockfileError::ParseError(format!(
            "expected v4 lockfile, got version '{}'",
            lock.version
        )));
    }
    let report = verify_v4_math(&lock)?;
    enforce_policy(&report, policy, trust_keys)?;
    Ok(report)
}

/// Sign a canonical payload digest with a keypair (returns the digest
/// string + the inline signature block; timestamps stay out-of-payload).
/// Ký digest payload canonical bằng keypair.
pub fn sign_payload_digest(
    digest: &str,
    key_pair: &mgc_crypto::keyring::KeyPair,
) -> LockfileResult<crate::v4::SignatureBlock> {
    let signer = key_pair.signer().map_err(LockfileError::CryptoError)?;
    let digest_bytes = blake3_digest_bytes(digest)?;
    let signature = signer.sign(&digest_bytes);
    let signature_str = format!("ed25519-{}", signature.to_base64());
    Ok(crate::v4::SignatureBlock {
        algorithm: "ed25519".to_string(),
        key_id: key_pair.key_id.clone(),
        public_key: signer.public_key().to_base64(),
        digest: digest.to_string(),
        signed_at: chrono::Utc::now().to_rfc3339(),
        signature: signature_str,
    })
}
