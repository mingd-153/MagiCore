//! mg trust sign — Sign lockfile
//! mg trust sign — Ký lockfile

use mg_crypto::keyring::Keyring;
use mg_lockfile::{load_lockfile, sign_and_write_lockfile};
use std::path::Path;

/// Execute `mg trust sign` — Thực thi `mg trust sign`
pub fn execute(lockfile_path: &str, key_id: Option<&str>) -> anyhow::Result<()> {
    let path = Path::new(lockfile_path);
    
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
            .ok_or_else(|| anyhow::anyhow!("No default key — run 'mg trust init'"))?
    };
    
    // Load lockfile
    let mut lockfile = load_lockfile(path)?;
    
    println!("Signing lockfile: {}", lockfile_path);
    println!("  Using key: {}", key_pair.key_id);
    
    // Sign lockfile
    sign_and_write_lockfile(&mut lockfile, path, key_pair)?;
    
    let sig_path = path.with_extension("lock.sig");
    println!("✓ Lockfile signed");
    println!("  Lockfile: {}", path.display());
    println!("  Signature: {}", sig_path.display());
    println!("\nCommit both files:");
    println!("  git add {} {}", lockfile_path, sig_path.display());
    
    Ok(())
}
