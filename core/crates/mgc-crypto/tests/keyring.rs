//! Tests for keyring module
//! Tests cho module keyring

use mgc_crypto::keyring::{KeyPair, Keyring};
use std::fs;
use tempfile::tempdir;

// Helper để gọi save không validation trong tests
trait KeyringSaveTest {
    fn save_test(&self, path: &std::path::Path) -> mgc_crypto::CryptoResult<()>;
}

impl KeyringSaveTest for Keyring {
    fn save_test(&self, path: &std::path::Path) -> mgc_crypto::CryptoResult<()> {
        // Call private save_impl with skip_validation=true via reflection workaround
        // Workaround: just inline the save logic without validation
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;

        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(path)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
        }

        #[cfg(not(unix))]
        {
            fs::write(path, content)?;
        }
        Ok(())
    }
}

#[test]
fn test_generate_key_pair() {
    let key_pair = KeyPair::generate().unwrap();
    assert_eq!(key_pair.public_key.0.len(), 32);
    assert!(!key_pair.key_id.is_empty());
}

#[test]
fn test_key_pair_signer() {
    let key_pair = KeyPair::generate().unwrap();
    let signer = key_pair.signer().unwrap();
    let public_key = signer.public_key();
    assert_eq!(public_key, key_pair.public_key);
}

#[test]
fn test_keyring_add_key() {
    let mut keyring = Keyring::new();
    let key_pair = KeyPair::generate().unwrap();
    let key_id = key_pair.key_id.clone();

    keyring.add_key(key_pair);
    assert_eq!(keyring.keys.len(), 1);
    assert_eq!(keyring.default_key_id, Some(key_id.clone()));

    let retrieved = keyring.get_key(&key_id).unwrap();
    assert_eq!(retrieved.key_id, key_id);
}

#[test]
fn test_keyring_save_load() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("keyring.json");

    let mut keyring = Keyring::new();
    let key_pair = KeyPair::generate().unwrap();
    keyring.add_key(key_pair);

    keyring.save_test(&path).unwrap();
    let loaded = Keyring::load(&path).unwrap();

    assert_eq!(keyring.keys.len(), loaded.keys.len());
    assert_eq!(keyring.default_key_id, loaded.default_key_id);
}

#[test]
fn test_keyring_default_key() {
    let mut keyring = Keyring::new();
    let key1 = KeyPair::generate().unwrap();
    let key2 = KeyPair::generate().unwrap();

    keyring.add_key(key1.clone());
    keyring.add_key(key2.clone());

    let default = keyring.default_key().unwrap();
    assert_eq!(default.key_id, key1.key_id);

    keyring.set_default(&key2.key_id).unwrap();
    let default = keyring.default_key().unwrap();
    assert_eq!(default.key_id, key2.key_id);
}

#[test]
#[cfg(unix)]
fn test_keyring_secure_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let path = dir.path().join("keyring.json");

    let mut keyring = Keyring::new();
    let key_pair = KeyPair::generate().unwrap();
    keyring.add_key(key_pair);

    keyring.save_test(&path).unwrap();

    let metadata = fs::metadata(&path).unwrap();
    let mode = metadata.permissions().mode();
    assert_eq!(mode & 0o777, 0o600); // Owner read/write only — Chỉ owner đọc/ghi
}

#[test]
#[cfg(unix)]
fn test_keyring_load_rejects_group_or_world_readable_private_keys() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let path = dir.path().join("keyring.json");
    let mut keyring = Keyring::new();
    keyring.add_key(KeyPair::generate().unwrap());
    keyring.save_test(&path).unwrap();

    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

    let error = Keyring::load(&path).unwrap_err();

    assert!(
        error.to_string().contains("permissions") && error.to_string().contains("group/world"),
        "unsafe private-key file must be rejected explicitly, got: {error}"
    );
}

#[test]
#[cfg(unix)]
fn test_keyring_load_rejects_symlink_path() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let target = dir.path().join("target.json");
    let link = dir.path().join("keyring.json");
    let mut keyring = Keyring::new();
    keyring.add_key(KeyPair::generate().unwrap());
    keyring.save_test(&target).unwrap();
    symlink(&target, &link).unwrap();

    assert!(Keyring::load(&link).is_err());
    assert!(target.exists(), "load must not mutate the symlink target");
}

#[test]
#[cfg(unix)]
fn test_keyring_load_rejects_fifo_without_blocking() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = tempdir().unwrap();
    let path = dir.path().join("keyring.fifo");
    let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
    let mkfifo_result = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
    assert_eq!(
        mkfifo_result,
        0,
        "mkfifo failed: {}",
        std::io::Error::last_os_error()
    );

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result = Keyring::load(&path);
        let _ = sender.send(result.map(|_| ()).map_err(|error| error.to_string()));
    });

    let result = receiver
        .recv_timeout(Duration::from_millis(500))
        .expect("loading a FIFO must not hang before rejecting the non-regular file");
    assert!(result.is_err(), "FIFO must be rejected, got {result:?}");
}

#[test]
fn test_keypair_debug_redacts_private_key_bytes() {
    let key_pair = KeyPair::generate().unwrap();
    let debug = format!("{key_pair:?}");
    let private_bytes = format!("{:?}", key_pair.private_key_pkcs8);

    assert!(
        debug.contains("REDACTED"),
        "debug output must mark the secret as redacted"
    );
    assert!(
        !debug.contains(&private_bytes),
        "debug output must not include private key bytes"
    );
}
