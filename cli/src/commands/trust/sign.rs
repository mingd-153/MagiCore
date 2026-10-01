//! mgc trust sign — Sign lockfile
//! mgc trust sign — Ký lockfile

use mgc_crypto::keyring::Keyring;
use mgc_lockfile::{LockDocument, load_lock_document, sign_and_write_lockfile};
use std::path::Path;
use std::time::Duration;

const LOCK_TEMP_GRACE: Duration = Duration::from_secs(60);

/// Execute `mgc trust sign` — Thực thi `mgc trust sign`
pub fn execute(lockfile_path: &str, key_id: Option<&str>) -> anyhow::Result<()> {
    let path = Path::new(lockfile_path);

    // Serialize lock+signature writes with install/update and reject an
    // interrupted dependency transaction before loading the lock bytes.
    // (Tuần tự hóa ghi lock+signature với install/update trước khi đọc.)
    let project_root = project_root_for_lock(path)?;
    let _project_lock = mgc_lockfile::project_lock::ProjectWriteLock::acquire(
        &project_root,
        crate::commands::core::shared::writer_lock_timeout(&project_root),
    )
    .map_err(|error| {
        anyhow::anyhow!("trust sign cannot acquire the project writer lock: {error}")
    })?;
    crate::commands::core::shared::ensure_no_pending_remove_journal(&project_root, &_project_lock)?;

    // Check if lockfile exists
    if !path.exists() {
        anyhow::bail!("Lockfile not found: {}", lockfile_path);
    }

    // Load keyring
    let keyring = Keyring::init_if_not_exists()?;

    // Get key to use
    let key_pair = if let Some(id) = key_id {
        keyring
            .get_key(id)
            .ok_or_else(|| anyhow::anyhow!("Key not found: {}", id))?
    } else {
        keyring
            .default_key()
            .ok_or_else(|| anyhow::anyhow!("No default key — run 'mgc trust init'"))?
    };

    println!("Signing lockfile: {}", lockfile_path);
    println!("  Using key: {}", key_pair.key_id);

    let inline_signature = sign_with_key_locked(path, key_pair, &_project_lock)?;

    println!("✓ Lockfile signed");
    println!("  Lockfile: {}", path.display());
    if inline_signature {
        println!("  Signature: inline Ed25519 metadata");
        println!("\nCommit the lockfile:");
        println!("  git add {}", lockfile_path);
    } else {
        let sig_path = path.with_extension("lock.sig");
        println!("  Signature: {}", sig_path.display());
        println!("\nCommit both files:");
        println!("  git add {} {}", lockfile_path, sig_path.display());
    }

    Ok(())
}

fn sign_with_key_locked(
    path: &Path,
    key_pair: &mgc_crypto::keyring::KeyPair,
    project_lock: &mgc_lockfile::project_lock::ProjectWriteLock,
) -> anyhow::Result<bool> {
    match load_lock_document(path)? {
        LockDocument::Legacy(mut lockfile) => {
            sign_and_write_lockfile(&mut lockfile, path, key_pair)?;
            Ok(false)
        }
        LockDocument::V4(mut document) => {
            if mgc_lockfile::signature_file_presence(path)? {
                return Err(crate::error::lock_v4_legacy_signature_sidecar());
            }

            if document.metadata.signature.is_some() {
                let current = mgc_lockfile::policy::verify_v4_math(&document)?;
                if !current.digest_ok || !current.signature_ok {
                    return Err(crate::error::lock_v4_existing_signature_invalid());
                }
            }

            let digest = mgc_lockfile::canonical::payload_digest(&document.payload());
            let signature = mgc_lockfile::policy::sign_payload_digest(&digest, key_pair)?;
            document.metadata.lockfile_hash = digest;
            document.metadata.signature = Some(signature);
            mgc_lockfile::policy::verify_v4_math(&document)?;
            let bytes = mgc_lockfile::canonical::write_v4_document(&document)?;
            mgc_lockfile::atomic::atomic_write_locked(
                project_lock,
                path,
                bytes.as_bytes(),
                LOCK_TEMP_GRACE,
            )?;
            Ok(true)
        }
    }
}

fn project_root_for_lock(path: &Path) -> anyhow::Result<std::path::PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute
        .parent()
        .ok_or_else(|| anyhow::anyhow!("lockfile path has no parent directory"))?;
    Ok(
        mgc_config::project::ProjectConfig::find_project_root(parent)
            .unwrap_or_else(|| parent.to_path_buf()),
    )
}

#[cfg(test)]
#[path = "test/sign.rs"]
mod tests;
