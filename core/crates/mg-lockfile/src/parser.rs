//! Lockfile parser with signature verification
//! Parser lockfile với xác minh chữ ký

use crate::{Lockfile, LockfileError, LockfileResult, SignatureFile};
use mg_crypto::blake3_signer::Blake3Hasher;
use mg_crypto::ed25519_signer::{verify_signature, Ed25519PublicKey, Ed25519Signature};
use std::path::Path;

/// Parse lockfile from TOML string — Parse lockfile từ chuỗi TOML
pub fn parse_lockfile(toml_str: &str) -> LockfileResult<Lockfile> {
    let lockfile: Lockfile = toml::from_str(toml_str)
        .map_err(|e| LockfileError::ParseError(format!("TOML parse failed: {}", e)))?;
    
    // Validate version
    if lockfile.version != "2" {
        return Err(LockfileError::ParseError(format!(
            "unsupported lockfile version: {}",
            lockfile.version
        )));
    }
    
    Ok(lockfile)
}

/// Load lockfile from file — Load lockfile từ file
pub fn load_lockfile(path: &Path) -> LockfileResult<Lockfile> {
    // L3 FIX: Limit lockfile size to prevent DoS (max 10MB)
    const MAX_LOCKFILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB
    
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_LOCKFILE_SIZE {
        return Err(LockfileError::ParseError(format!(
            "lockfile too large: {} bytes (max {})",
            metadata.len(),
            MAX_LOCKFILE_SIZE
        )));
    }
    
    let content = std::fs::read_to_string(path)?;
    parse_lockfile(&content)
}

/// Load and verify lockfile with signature — Load và verify lockfile với chữ ký
pub fn load_and_verify_lockfile(
    lockfile_path: &Path,
    signature_path: &Path,
) -> LockfileResult<Lockfile> {
    // Load lockfile
    let lockfile_bytes = std::fs::read(lockfile_path)?;
    // L8 FIX: Graceful UTF-8 handling (không panic)
    let lockfile_str = std::str::from_utf8(&lockfile_bytes)
        .map_err(|e| LockfileError::ParseError(format!("invalid UTF-8: {}", e)))?;
    let lockfile = parse_lockfile(lockfile_str)?;
    
    // Check if signature file exists
    if !signature_path.exists() {
        return Err(LockfileError::VerificationFailed(
            "signature file not found".to_string(),
        ));
    }
    
    // Load signature file
    let sig_content = std::fs::read_to_string(signature_path)?;
    let sig_file: SignatureFile = sig_content.parse()
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
    let signer = lockfile
        .metadata
        .signer
        .as_ref()
        .ok_or_else(|| LockfileError::VerificationFailed("no signer info in lockfile".to_string()))?;
    
    // Parse public key
    let public_key = Ed25519PublicKey::from_base64(&signer.public_key)?;
    
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
pub fn is_lockfile_signed(lockfile_path: &Path) -> bool {
    let sig_path = signature_path_for(lockfile_path);
    sig_path.exists()
}

/// Get signature path for lockfile — Lấy đường dẫn signature cho lockfile
pub fn signature_path_for(lockfile_path: &Path) -> std::path::PathBuf {
    lockfile_path.with_extension("lock.sig")
}
