//! mgc trust verify — Verify lockfile signature
//! mgc trust verify — Xác minh chữ ký lockfile

use mgc_lockfile::{VerificationStatus, verification_status_message};
use std::path::Path;

/// Execute `mgc trust verify` — Thực thi `mgc trust verify`
pub fn execute(lockfile_path: &str) -> anyhow::Result<()> {
    let path = Path::new(lockfile_path);

    // Check if lockfile exists
    if !path.exists() {
        anyhow::bail!("Lockfile not found: {}", lockfile_path);
    }

    println!("Verifying lockfile: {}", lockfile_path);

    // Verify lockfile
    let status = super::policy::verify_project_lockfile(path)?;
    let message = verification_status_message(&status);

    println!("{}", message);

    // Exit with error if tampered or invalid
    match status {
        VerificationStatus::Valid => {
            println!("\n✓ Lockfile is valid and signed");
            Ok(())
        }
        VerificationStatus::UntrustedKey(key_id) => {
            println!(
                "\nWARN: Signature is cryptographically valid, but signer '{key_id}' is not trusted by this project"
            );
            Ok(())
        }
        VerificationStatus::Unsigned => {
            println!("\nWARN: Lockfile is not signed");
            println!("  Run 'mgc trust sign' to sign it");
            Ok(())
        }
        VerificationStatus::Tampered(msg) => {
            anyhow::bail!("Lockfile tampered: {}", msg);
        }
        VerificationStatus::InvalidSignature(msg) => {
            anyhow::bail!("Invalid signature: {}", msg);
        }
    }
}
