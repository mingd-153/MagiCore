use super::{KeyPair, Keyring};
use std::fs;
use tempfile::tempdir;

#[test]
fn save_replaces_existing_keyring_and_rotates_backup_portably() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("keyring.json");
    let backup = path.with_extension("json.bak");
    let mut original = Keyring::new();
    original.add_key(KeyPair::generate().unwrap());
    let original_id = original.default_key_id.clone();
    original.save_impl(&path, true).unwrap();

    let mut replacement = Keyring::new();
    replacement.add_key(KeyPair::generate().unwrap());
    replacement.save_impl(&path, true).unwrap();

    assert_eq!(
        Keyring::load(&path).unwrap().default_key_id,
        replacement.default_key_id
    );
    assert_eq!(Keyring::load(&backup).unwrap().default_key_id, original_id);
}

#[test]
#[cfg(unix)]
fn save_publishes_private_keyring_and_backup_without_leaking_temp_files() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let path = directory.path().join("keyring.json");
    let backup = path.with_extension("json.bak");
    let mut original = Keyring::new();
    original.add_key(KeyPair::generate().unwrap());
    original.save_impl(&path, true).unwrap();
    let original_id = original.default_key_id.clone();

    let mut replacement = Keyring::new();
    replacement.add_key(KeyPair::generate().unwrap());
    replacement.save_impl(&path, true).unwrap();

    assert_eq!(
        Keyring::load(&path).unwrap().default_key_id,
        replacement.default_key_id
    );
    assert_eq!(Keyring::load(&backup).unwrap().default_key_id, original_id);
    for candidate in [path, backup] {
        assert_eq!(
            fs::metadata(candidate).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert!(directory.path().read_dir().unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
}

#[test]
#[cfg(unix)]
fn save_rejects_insecure_existing_permissions_without_mutating_keyring() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let path = directory.path().join("keyring.json");
    let mut first = Keyring::new();
    first.add_key(KeyPair::generate().unwrap());
    first.save_impl(&path, true).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

    let mut replacement = Keyring::new();
    replacement.add_key(KeyPair::generate().unwrap());
    let original = fs::read(&path).unwrap();
    assert!(replacement.save_impl(&path, true).is_err());

    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );
}

#[test]
#[cfg(unix)]
fn save_refuses_keyring_symlink_without_writing_through_it() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().unwrap();
    let target = directory.path().join("target.json");
    let path = directory.path().join("keyring.json");
    fs::write(&target, b"do not overwrite").unwrap();
    symlink(&target, &path).unwrap();
    let mut keyring = Keyring::new();
    keyring.add_key(KeyPair::generate().unwrap());

    assert!(keyring.save_impl(&path, true).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"do not overwrite");
    assert!(
        fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
#[cfg(unix)]
fn save_refuses_backup_symlink_without_writing_through_it() {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::fs::symlink;

    let directory = tempdir().unwrap();
    let path = directory.path().join("keyring.json");
    let target = directory.path().join("outside.json");
    let backup = path.with_extension("json.bak");
    fs::write(&target, b"do not overwrite").unwrap();
    let mut original = Keyring::new();
    original.add_key(KeyPair::generate().unwrap());
    original.save_impl(&path, true).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&target, &backup).unwrap();

    let mut replacement = Keyring::new();
    replacement.add_key(KeyPair::generate().unwrap());
    assert!(replacement.save_impl(&path, true).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"do not overwrite");
    assert!(
        fs::symlink_metadata(&backup)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        Keyring::load(&path).unwrap().default_key_id,
        original.default_key_id
    );
}
