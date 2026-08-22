//! mg trust verify — Verify lockfile signature
//! mg trust verify — Xác minh chữ ký lockfile

use mg_lockfile::{verify_lockfile, verification_status_message, VerificationStatus};
use std::path::Path;

/// Execute `mg trust verify` — Thực thi `mg trust verify`
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
            println!("  Run 'mg trust sign' to sign it");
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
