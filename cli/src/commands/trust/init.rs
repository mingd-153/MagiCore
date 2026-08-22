//! mg trust init — Initialize keyring
//! mg trust init — Khởi tạo keyring

use mg_crypto::keyring::{KeyPair, Keyring};

/// Execute `mg trust init` — Thực thi `mg trust init`
pub fn execute(force: bool) -> anyhow::Result<()> {
    let keyring_path = Keyring::default_path();
    
    // R1.2 FIX (AUDIT VÒNG 2): Try recover from backup if corrupted
    if keyring_path.exists() && !force {
        match Keyring::load(&keyring_path) {
            Ok(_) => {
                println!("✓ Keyring already initialized at: {}", keyring_path.display());
                println!("  Use --force to reinitialize");
                return Ok(());
            }
            Err(_) => {
                // Try backup recovery
                let backup = keyring_path.with_extension("json.bak");
                if backup.exists() {
                    println!("⚠ Keyring corrupted, attempting recovery from backup...");
                    if let Err(e) = std::fs::copy(&backup, &keyring_path) {
                        println!("✗ Backup recovery failed: {}", e);
                        println!("  Use --force to create new keyring");
                        return Ok(());
                    }
                    println!("✓ Recovered keyring from backup");
                    return Ok(());
                } else {
                    println!("⚠ Keyring corrupted and no backup found");
                    println!("  Use --force to create new keyring");
                    return Ok(());
                }
            }
        }
    }
    
    if force && keyring_path.exists() {
        println!("⚠ Reinitializing keyring (old keys will be lost)");
    }
    
    // Generate new key
    let key_pair = KeyPair::generate()?;
    let key_id = key_pair.key_id.clone();
    
    // Create keyring
    let mut keyring = Keyring::new();
    keyring.add_key(key_pair);
    
    // Save keyring
    keyring.save(&keyring_path)?;
    
    println!("✓ Keyring initialized");
    println!("  Location: {}", keyring_path.display());
    println!("  Default key: {}", key_id);
    
    // R3.1 FIX (AUDIT VÒNG 2): Detect existing signatures and prompt re-sign
    let lockfile_path = std::env::current_dir()
        .ok()
        .map(|p| p.join("mg.lock"))
        .unwrap_or_else(|| std::path::PathBuf::from("mg.lock"));
    let sig_path = lockfile_path.with_extension("lock.sig");
    
    if sig_path.exists() && force {
        println!("\n⚠ Existing lockfile signature detected");
        println!("  Old signature will be invalid with new key");
        println!("  Run 'mg trust sign' to re-sign with new key");
    }
    
    println!("\nNext steps:");
    println!("  • Run 'mg trust sign' to sign your lockfile");
    println!("  • Run 'mg trust list' to view keys");
    
    Ok(())
}
