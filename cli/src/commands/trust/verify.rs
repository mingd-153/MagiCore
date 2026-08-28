//! mgc trust verify — Verify lockfile signature
//! mgc trust verify — Xác minh chữ ký lockfile

use mgc_lockfile::{verification_status_message, verify_lockfile, VerificationStatus};
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
    let status = verify_lockfile(path)?;
    let message = verification_status_message(&status);

    println!("{}", message);

    // Exit with error if tampered or invalid
    match status {
        VerificationStatus::Valid => {
            println!("\n✓ Lockfile is valid and signed");
            Ok(())
        }
        VerificationStatus::Unsigned => {
            println!("\n⚠ Lockfile is not signed");
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
