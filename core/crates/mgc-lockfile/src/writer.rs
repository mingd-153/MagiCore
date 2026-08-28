//! Lockfile writer with signing
//! Writer lockfile với ký

use crate::{Lockfile, LockfileError, LockfileResult, SignatureFile, SignerInfo};
use mgc_crypto::blake3_signer::Blake3Hasher;
use mgc_crypto::keyring::{KeyPair, Keyring};
use std::path::Path;

/// Write lockfile to TOML string — Ghi lockfile ra chuỗi TOML
pub fn serialize_lockfile(lockfile: &Lockfile) -> LockfileResult<String> {
    let toml_str = toml::to_string_pretty(lockfile)?;
    Ok(toml_str)
}

/// Write lockfile to file — Ghi lockfile vào file
pub fn write_lockfile(lockfile: &Lockfile, path: &Path) -> LockfileResult<()> {
    let toml_str = serialize_lockfile(lockfile)?;
    std::fs::write(path, toml_str)?;
    Ok(())
}

/// Sign lockfile and write signature file — Ký lockfile và ghi file signature
pub fn sign_and_write_lockfile(
    lockfile: &mut Lockfile,
    lockfile_path: &Path,
    key_pair: &KeyPair,
) -> LockfileResult<()> {
    // L1 FIX: Atomic write — add signer info BEFORE first write (no double-write race)
    let signer = key_pair.signer()?;

    // Pre-populate signer info (without hash yet)
    lockfile.metadata.signer = Some(SignerInfo {
        key_id: key_pair.key_id.clone(),
        public_key: signer.public_key().to_base64(),
        signed_at: chrono::Utc::now().to_rfc3339(),
    });
    lockfile.metadata.lockfile_hash = String::new(); // Placeholder

    // Write lockfile ONCE with signer info
    write_lockfile(lockfile, lockfile_path)?;

    // Compute final hash
    let lockfile_bytes = std::fs::read(lockfile_path)?;
    let lockfile_hash = Blake3Hasher::hash_bytes(&lockfile_bytes);
    let lockfile_hash_str = format!("blake3-{}", lockfile_hash.to_base64());

    // Sign the hash
    let signature = signer.sign(lockfile_hash.0.as_ref());
    let signature_str = format!("ed25519-{}", signature.to_base64());

    // Update lockfile hash in-place (no rewrite needed if we use atomic update later)
    lockfile.metadata.lockfile_hash = lockfile_hash_str.clone();

    // L7 FIX: Atomic signature file write (temp file + rename)
    let sig_file = SignatureFile::new(lockfile_hash_str, signature_str, key_pair.key_id.clone());
    let sig_path = lockfile_path.with_extension("lock.sig");
    let temp_sig_path = sig_path.with_extension("lock.sig.tmp");

    // Write to temp file first
    std::fs::write(&temp_sig_path, sig_file.to_string())?;

    // Atomic rename (POSIX guarantees atomicity)
    std::fs::rename(&temp_sig_path, &sig_path)?;

    Ok(())
}

/// Sign lockfile with default key from keyring — Ký lockfile với key mặc định từ keyring
pub fn sign_lockfile_with_default_key(
    lockfile: &mut Lockfile,
    lockfile_path: &Path,
) -> LockfileResult<()> {
    // Load or init keyring
    let keyring = Keyring::init_if_not_exists().map_err(LockfileError::CryptoError)?;

    // Get default key
    let key_pair = keyring.default_key().ok_or_else(|| {
        LockfileError::VerificationFailed("no default key in keyring".to_string())
    })?;

    sign_and_write_lockfile(lockfile, lockfile_path, key_pair)
}
