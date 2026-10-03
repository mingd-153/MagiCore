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
use crate::v4::{canonical_name, canonical_version};
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

/// Resolve policy; CI is always strict and cannot be downgraded by project or env config.
/// Ngoài CI: cờ CLI → env → `mgc.toml [lock]` → mặc định môi trường.
pub fn resolve_policy(flag: Option<&str>, project_root: Option<&Path>) -> LockPolicyMode {
    // CI is a release trust boundary; untrusted project config and ambient env
    // must not turn signature enforcement off.
    // CI là ranh giới tin cậy phát hành; config project/env không được hạ policy.
    let ci = std::env::var("CI")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false);
    if ci {
        return LockPolicyMode::Require;
    }

    if let Some(mode) = flag.and_then(LockPolicyMode::parse) {
        return mode;
    }
    if let Ok(raw) = std::env::var("MGC_LOCK_POLICY")
        && let Some(mode) = LockPolicyMode::parse(&raw)
    {
        return mode;
    }
    if let Some(root) = project_root
        && let Some(content) = read_project_config_no_follow(&root.join("mgc.toml"))
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

/// Read project lock policy without following a symlink/reparse point.
/// Đọc lock policy project mà không theo symlink/reparse point.
fn read_project_config_no_follow(path: &Path) -> Option<String> {
    use std::io::Read;

    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).ok()?;
    let opened = file.metadata().ok()?;
    if !opened.is_file() {
        return None;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if opened.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return None;
        }
    }
    const MAX_CONFIG_BYTES: u64 = 10 * 1024 * 1024;
    if opened.len() > MAX_CONFIG_BYTES {
        return None;
    }
    let mut content = String::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_string(&mut content)
        .ok()?;
    (content.len() as u64 <= MAX_CONFIG_BYTES).then_some(content)
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
    validate_v4_root_references(lock)?;
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
    let public_key_hash = mgc_crypto::blake3_signer::Blake3Hasher::hash_bytes(&public_key.0);
    let actual_key_id = hex::encode(&public_key_hash.0[..8]);
    if signature.key_id != actual_key_id {
        return Err(LockfileError::VerificationFailed(
            "signature key ID does not match the public-key fingerprint".to_string(),
        ));
    }
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

/// Require each declared root pin to identify exactly one locked instance.
/// Bắt mỗi root pin phải trỏ chính xác một instance trong lock.
fn validate_v4_root_references(lock: &LockfileV4) -> LockfileResult<()> {
    for (owner, roots) in &lock.root_dependencies_by_owner {
        for root in roots {
            validate_root_reference(lock, root, Some(owner))?;
        }
    }
    for root in &lock.root_dependencies {
        validate_root_reference(lock, root, None)?;
    }
    Ok(())
}

/// Resolve one root identity without guessing across sources or variants.
/// Phân giải một root mà không đoán giữa các source hoặc variant.
fn validate_root_reference(
    lock: &LockfileV4,
    raw: &str,
    owner: Option<&str>,
) -> LockfileResult<()> {
    let pin = crate::root_pin::parse_root_pin(raw);
    if raw.starts_with("mgc-root-v1:") && pin.ecosystem.is_none() {
        return Err(LockfileError::VerificationFailed(format!(
            "malformed qualified root pin '{raw}'"
        )));
    }
    let package_id = if pin.ecosystem.is_none() {
        pin.package_id
            .strip_prefix("npm:")
            .unwrap_or(pin.package_id)
    } else {
        pin.package_id
    };
    let Some((name, version)) = package_id.rsplit_once('@') else {
        return Err(LockfileError::VerificationFailed(format!(
            "root pin '{raw}' is missing a package version"
        )));
    };
    if name.is_empty() || version.is_empty() {
        return Err(LockfileError::VerificationFailed(format!(
            "root pin '{raw}' has an empty package name or version"
        )));
    }

    let matches = lock
        .packages
        .iter()
        .filter(|package| {
            pin.ecosystem
                .is_none_or(|ecosystem| package.key.ecosystem == ecosystem)
                && package.key.name == canonical_name(package.key.ecosystem, name)
                && package.key.version == canonical_version(version)
        })
        .count();
    let location = owner.map_or_else(
        || "root_dependencies".to_string(),
        |owner| format!("root_dependencies_by_owner.{owner}"),
    );
    match matches {
        1 => Ok(()),
        0 => Err(LockfileError::VerificationFailed(format!(
            "root pin '{raw}' in {location} does not reference a locked package"
        ))),
        _ => Err(LockfileError::VerificationFailed(format!(
            "root pin '{raw}' in {location} is ambiguous across locked package sources or variants"
        ))),
    }
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

/// SRI algorithms the web materializer enforces (must stay in sync with
/// `adapters/web/src/install/integrity.rs::verify_sri_integrity`).
/// Thuật toán SRI materializer web bắt buộc (phải đồng bộ với
/// `verify_sri_integrity`).
pub const INSTALL_SRI_ALGORITHMS: &[&str] = &["sha256", "sha512"];

/// Check an SRI string parses as `<algo>-<base64>` with a supported strong
/// algorithm and non-empty payload — fail-closed on weak/unknown/empty.
/// The materializer re-verifies bytes at install; this gate only rejects
/// values that could never verify (never fabricates a hash).
/// Kiểm tra chuỗi SRI đúng dạng với thuật toán mạnh được hỗ trợ —
/// fail-closed với yếu/không rõ/trống. Materializer verify lại byte khi
/// install; gate này chỉ loại giá trị không bao giờ verify được.
pub fn check_install_sri(sri: &str) -> LockfileResult<()> {
    let fail =
        |why: &str| LockfileError::VerificationFailed(format!("unusable SRI integrity: {why}"));
    let Some((algorithm, payload)) = sri.split_once('-') else {
        return Err(fail("missing `<algo>-<base64>` shape"));
    };
    if !INSTALL_SRI_ALGORITHMS.contains(&algorithm) {
        return Err(fail(&format!(
            "algorithm '{algorithm}' is not enforced by the installer (sha256/sha512 only)"
        )));
    }
    if payload.is_empty()
        || !payload
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
    {
        return Err(fail("empty or non-base64 payload"));
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
    let bytes = crate::parser::read_lockfile_bytes(path)?;
    let content = std::str::from_utf8(&bytes).map_err(|error| {
        LockfileError::ParseError(format!("lockfile is not valid UTF-8: {error}"))
    })?;
    let lock: LockfileV4 = crate::canonical::parse_v4_document(content)?;
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
