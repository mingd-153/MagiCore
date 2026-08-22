//! Lockfile writer with signing
//! Writer lockfile với ký

use crate::{Lockfile, LockfileError, LockfileResult, SignatureFile, SignerInfo};
use mg_crypto::blake3_signer::Blake3Hasher;
use mg_crypto::keyring::{KeyPair, Keyring};
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
    // Write lockfile first (without signer info)
    write_lockfile(lockfile, lockfile_path)?;
    
    // Compute lockfile hash
    let lockfile_bytes = std::fs::read(lockfile_path)?;
    let lockfile_hash = Blake3Hasher::hash_bytes(&lockfile_bytes);
    let lockfile_hash_str = format!("blake3-{}", lockfile_hash.to_base64());
    
    // Sign the hash
    let signer = key_pair.signer()?;
    let signature = signer.sign(lockfile_hash.0.as_ref());
    let signature_str = format!("ed25519-{}", signature.to_base64());
    
    // Update lockfile with signer info
    lockfile.metadata.lockfile_hash = lockfile_hash_str.clone();
    lockfile.metadata.signer = Some(SignerInfo {
        key_id: key_pair.key_id.clone(),
        public_key: signer.public_key().to_base64(),
        signed_at: chrono::Utc::now().to_rfc3339(),
    });
    
    // Rewrite lockfile with signer info
    write_lockfile(lockfile, lockfile_path)?;
    
    // Recompute hash after adding signer info
    let lockfile_bytes = std::fs::read(lockfile_path)?;
    let lockfile_hash = Blake3Hasher::hash_bytes(&lockfile_bytes);
    let lockfile_hash_str = format!("blake3-{}", lockfile_hash.to_base64());
    
    // Re-sign with updated hash
    let signature = signer.sign(lockfile_hash.0.as_ref());
    let signature_str = format!("ed25519-{}", signature.to_base64());
    
    // Create signature file
    let sig_file = SignatureFile::new(lockfile_hash_str, signature_str, key_pair.key_id.clone());
    
    // Write signature file
    let sig_path = lockfile_path.with_extension("lock.sig");
    std::fs::write(sig_path, sig_file.to_string())?;
    
    Ok(())
}

/// Sign lockfile with default key from keyring — Ký lockfile với key mặc định từ keyring
pub fn sign_lockfile_with_default_key(
    lockfile: &mut Lockfile,
    lockfile_path: &Path,
) -> LockfileResult<()> {
    // Load or init keyring
    let keyring = Keyring::init_if_not_exists()
        .map_err(|e| LockfileError::CryptoError(e))?;
    
    // Get default key
    let key_pair = keyring
        .default_key()
        .ok_or_else(|| LockfileError::VerificationFailed("no default key in keyring".to_string()))?;
    
    sign_and_write_lockfile(lockfile, lockfile_path, key_pair)
}
