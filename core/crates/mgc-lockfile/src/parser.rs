//! Lockfile parser with signature verification
//! Parser lockfile với xác minh chữ ký

use crate::{LOCKFILE_SCHEMA_VERSION, Lockfile, LockfileError, LockfileResult, SignatureFile};
use mgc_crypto::blake3_signer::Blake3Hasher;
use mgc_crypto::ed25519_signer::{Ed25519PublicKey, Ed25519Signature, verify_signature};
use std::path::Path;
use std::{fs::OpenOptions, io::Read};

const MAX_LOCKFILE_SIZE: u64 = 10 * 1024 * 1024;
const MAX_SIGNATURE_FILE_SIZE: u64 = 64 * 1024;

/// Parsed lock document without discarding schema-specific identity.
/// Tài liệu lock đã parse, giữ nguyên identity theo schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockDocument {
    Legacy(Lockfile),
    V4(crate::canonical::LockfileV4),
}

/// Read a regular file without following links, bounded by a caller-supplied byte limit.
/// Đọc file thường, từ chối đi theo link và giới hạn theo số byte caller cung cấp.
pub fn read_bounded_regular_file(
    path: &Path,
    max_size: u64,
    label: &str,
) -> LockfileResult<Vec<u8>> {
    let read_limit = max_size
        .checked_add(1)
        .ok_or_else(|| LockfileError::ParseError(format!("{label} size limit is too large")))?;
    let max_capacity = usize::try_from(max_size)
        .ok()
        .filter(|capacity| *capacity < usize::MAX)
        .ok_or_else(|| {
            LockfileError::ParseError(format!("{label} size limit exceeds addressable memory"))
        })?;
    let path_metadata = std::fs::symlink_metadata(path)?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
        return Err(LockfileError::ParseError(format!(
            "{label} '{}' must be a regular non-symlink file",
            path.display()
        )));
    }
    if path_metadata.len() > max_size {
        return Err(LockfileError::ParseError(format!(
            "{label} too large: {} bytes (max {})",
            path_metadata.len(),
            max_size
        )));
    }

    let mut options = OpenOptions::new();
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
    let file = options.open(path)?;
    let opened_metadata = file.metadata()?;
    if !opened_metadata.is_file() {
        return Err(LockfileError::ParseError(format!(
            "{label} '{}' is not a regular file",
            path.display()
        )));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if opened_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(LockfileError::ParseError(format!(
                "{label} '{}' must not be a reparse point",
                path.display()
            )));
        }
    }
    if opened_metadata.len() > max_size {
        return Err(LockfileError::ParseError(format!(
            "{label} too large: {} bytes (max {})",
            opened_metadata.len(),
            max_size
        )));
    }

    let capacity = usize::try_from(opened_metadata.len())
        .ok()
        .filter(|capacity| *capacity <= max_capacity)
        .ok_or_else(|| {
            LockfileError::ParseError(format!("{label} size exceeds addressable memory"))
        })?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(read_limit).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_size {
        return Err(LockfileError::ParseError(format!(
            "{label} too large: more than {max_size} bytes"
        )));
    }
    Ok(bytes)
}

/// Read the canonical MGC lockfile bytes with a fixed size limit and a
/// no-follow regular-file check. Use this for every production read of
/// `mgc.lock`, including consumers that need a legacy-format fallback.
/// (Đọc mgc.lock có giới hạn cố định, từ chối symlink và file không thường.)
pub fn read_lockfile_bytes(path: &Path) -> LockfileResult<Vec<u8>> {
    read_bounded_regular_file(path, MAX_LOCKFILE_SIZE, "lockfile")
}

/// Read a lock signature sidecar with no-follow checks and its smaller limit.
pub fn read_signature_file_bytes(path: &Path) -> LockfileResult<Vec<u8>> {
    read_bounded_regular_file(path, MAX_SIGNATURE_FILE_SIZE, "signature file")
}

/// Parse lockfile from TOML string — Parse lockfile từ chuỗi TOML
pub fn parse_lockfile(toml_str: &str) -> LockfileResult<Lockfile> {
    let value: toml::Value = toml::from_str(toml_str)
        .map_err(|e| LockfileError::ParseError(format!("TOML parse failed: {}", e)))?;
    parse_legacy_value(value)
}

/// Deserialize a legacy schema from an already parsed TOML value.
/// Parse schema legacy từ TOML value đã được đọc một lần.
fn parse_legacy_value(value: toml::Value) -> LockfileResult<Lockfile> {
    let lockfile: Lockfile = value
        .try_into()
        .map_err(|error| LockfileError::ParseError(format!("TOML parse failed: {error}")))?;
    // Validate version: v2 stays readable (new fields fall back to their
    // serde defaults), v3 is the canonical write target.
    // Kiểm tra version: v2 vẫn đọc được (field mới về mặc định serde),
    // v3 là đích ghi canonical.
    if lockfile.version != "2" && lockfile.version != LOCKFILE_SCHEMA_VERSION {
        return Err(LockfileError::ParseError(format!(
            "unsupported lockfile version: {}",
            lockfile.version
        )));
    }

    Ok(lockfile)
}

/// Dispatch TOML lockfiles by their declared schema version.
/// Do not deserialize v4 into the legacy `(name, version)` model.
/// Chọn parser theo version; không ép v4 vào model legacy.
pub fn parse_document(toml_str: &str) -> LockfileResult<LockDocument> {
    let value: toml::Value = toml::from_str(toml_str)
        .map_err(|error| LockfileError::ParseError(format!("TOML parse failed: {error}")))?;
    let version = value
        .get("version")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| LockfileError::ParseError("missing lockfile version".to_string()))?
        .to_string();

    match version.as_str() {
        "4" => {
            let lock: crate::canonical::LockfileV4 = value.try_into().map_err(|error| {
                LockfileError::ParseError(format!("TOML parse failed: {error}"))
            })?;
            if lock.version != crate::v4::LOCKFILE_SCHEMA_V4 {
                return Err(LockfileError::ParseError(format!(
                    "expected v4 lockfile, got version '{}'",
                    lock.version
                )));
            }
            Ok(LockDocument::V4(lock))
        }
        "2" | LOCKFILE_SCHEMA_VERSION => parse_legacy_value(value).map(LockDocument::Legacy),
        other => Err(LockfileError::ParseError(format!(
            "unsupported lockfile version: {other}"
        ))),
    }
}

/// Load a lock document while preserving the schema-specific representation.
/// Nạp lock và giữ nguyên model riêng của từng schema.
pub fn load_lock_document(path: &Path) -> LockfileResult<LockDocument> {
    let bytes = read_lockfile_bytes(path)?;
    let content = std::str::from_utf8(&bytes)
        .map_err(|error| LockfileError::ParseError(format!("invalid UTF-8: {error}")))?;
    parse_document(content)
}

/// Require the legacy API model without silently flattening v4 identity.
/// Chỉ nhận model legacy, không ép identity v4 vào schema cũ.
fn require_legacy_document(document: LockDocument) -> LockfileResult<Lockfile> {
    match document {
        LockDocument::Legacy(lockfile) => Ok(lockfile),
        LockDocument::V4(_) => Err(LockfileError::ParseError(
            "v4 lockfile requires the lossless LockDocument API; legacy Lockfile cannot preserve v4 package identities"
                .to_string(),
        )),
    }
}

/// Load lockfile from file — Load lockfile từ file
pub fn load_lockfile(path: &Path) -> LockfileResult<Lockfile> {
    require_legacy_document(load_lock_document(path)?)
}

/// Load and verify lockfile with signature — Load và verify lockfile với chữ ký
pub fn load_and_verify_lockfile(
    lockfile_path: &Path,
    signature_path: &Path,
) -> LockfileResult<Lockfile> {
    let lockfile_bytes = read_bounded_regular_file(lockfile_path, MAX_LOCKFILE_SIZE, "lockfile")?;
    let lockfile_str = std::str::from_utf8(&lockfile_bytes)
        .map_err(|e| LockfileError::ParseError(format!("invalid UTF-8: {}", e)))?;
    let lockfile = require_legacy_document(parse_document(lockfile_str)?)?;

    let signature_bytes = read_signature_file_bytes(signature_path)?;
    let sig_content = std::str::from_utf8(&signature_bytes)
        .map_err(|e| LockfileError::ParseError(format!("invalid signature UTF-8: {e}")))?;
    let sig_file: SignatureFile = sig_content
        .parse()
        .map_err(LockfileError::InvalidSignatureFile)?;

    // Verify lockfile hash
    let current_hash = Blake3Hasher::hash_bytes(&lockfile_bytes);
    let current_hash_str = format!("blake3-{}", current_hash.to_base64());

    if current_hash_str != sig_file.lockfile_hash {
        return Err(LockfileError::TamperedLockfile(format!(
            "hash mismatch: expected {}, got {}",
            sig_file.lockfile_hash, current_hash_str
        )));
    }

    // Verify signature
    let signer = lockfile.metadata.signer.as_ref().ok_or_else(|| {
        LockfileError::VerificationFailed("no signer info in lockfile".to_string())
    })?;

    // Parse public key
    let public_key = Ed25519PublicKey::from_base64(&signer.public_key)?;

    // Bind both human-readable key IDs to the actual public key. The lock
    // and sidecar are attacker-controlled inputs, so trusting either claimed
    // ID without recomputing the fingerprint would permit key-ID spoofing.
    // (Khóa ID trong cả hai file phải khớp fingerprint thật của public key.)
    let public_key_hash = Blake3Hasher::hash_bytes(&public_key.0);
    let actual_key_id = hex::encode(&public_key_hash.0[..8]);
    if signer.key_id != actual_key_id || sig_file.key_id != actual_key_id {
        return Err(LockfileError::VerificationFailed(
            "signer key ID does not match the public-key fingerprint".to_string(),
        ));
    }

    // Parse signature (strip "ed25519-" prefix if present)
    let sig_base64 = sig_file
        .signature
        .strip_prefix("ed25519-")
        .unwrap_or(&sig_file.signature);
    let signature = Ed25519Signature::from_base64(sig_base64)?;

    // Verify signature against lockfile hash
    verify_signature(&public_key, current_hash.0.as_ref(), &signature).map_err(|e| {
        LockfileError::VerificationFailed(format!("signature verification failed: {}", e))
    })?;

    Ok(lockfile)
}

/// Check if lockfile is signed (signature file exists) — Kiểm tra lockfile đã ký chưa
#[deprecated(note = "use signature_file_presence to distinguish missing from invalid paths")]
pub fn is_lockfile_signed(lockfile_path: &Path) -> bool {
    signature_file_presence(lockfile_path).unwrap_or(false)
}

/// Fallible signature-sidecar probe that distinguishes absence from an
/// invalid path, symlink, or metadata error.
pub fn signature_file_presence(lockfile_path: &Path) -> LockfileResult<bool> {
    let sig_path = signature_path_for(lockfile_path);
    match read_signature_file_bytes(&sig_path) {
        Ok(_) => Ok(true),
        Err(LockfileError::IoError(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

/// Get signature path for lockfile — Lấy đường dẫn signature cho lockfile
pub fn signature_path_for(lockfile_path: &Path) -> std::path::PathBuf {
    lockfile_path.with_extension("lock.sig")
}
