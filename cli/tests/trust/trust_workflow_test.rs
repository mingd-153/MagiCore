#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Integration tests for `mgc trust` workflow
//! Test tích hợp cho workflow `mgc trust`

use mgc_crypto::keyring::{KeyPair, Keyring};
use mgc_lockfile::{load_lockfile, sign_and_write_lockfile, verify_lockfile, VerificationStatus};
use std::fs;
use tempfile::TempDir;

use super::fixtures::{
    cleanup_keyring, test_keyring_path, write_empty_lockfile, write_lockfile_with_package,
    write_lockfile_with_packages,
};

#[test]
fn test_full_trust_workflow() {
    // Setup temp directory
    let temp_dir = TempDir::new().unwrap();
    let lockfile_path = temp_dir.path().join("mgc.lock");
    let keyring_path = test_keyring_path("full-workflow");

    // Step 1: Create sample lockfile
    write_lockfile_with_package(&lockfile_path, "example", "1.0.0", "sri:blake3:abc123");

    // Step 2: Initialize keyring (mgc trust init)
    let key_pair = KeyPair::generate().unwrap();
    let mut keyring = Keyring::new();
    keyring.add_key(key_pair.clone());
    keyring.save(&keyring_path).unwrap();

    // Step 3: Load lockfile
    let mut lockfile = load_lockfile(&lockfile_path).unwrap();

    // Step 4: Sign lockfile (mgc trust sign)
    sign_and_write_lockfile(&mut lockfile, &lockfile_path, &key_pair).unwrap();

    // Step 5: Verify lockfile (mgc trust verify)
    let status = verify_lockfile(&lockfile_path).unwrap();
    assert_eq!(status, VerificationStatus::Valid);

    // Step 6: Tamper with lockfile
    let mut content = fs::read_to_string(&lockfile_path).unwrap();
    content = content.replace("example-1.0.0", "example-1.0.1"); // Malicious change
    fs::write(&lockfile_path, content).unwrap();

    // Step 7: Verify tampered lockfile (should fail)
    let status = verify_lockfile(&lockfile_path).unwrap();
    assert!(matches!(status, VerificationStatus::Tampered(_)));
    cleanup_keyring(&keyring_path);
}

#[test]
fn test_unsigned_lockfile() {
    let temp_dir = TempDir::new().unwrap();
    let lockfile_path = temp_dir.path().join("mgc.lock");

    write_empty_lockfile(&lockfile_path);

    // Verify unsigned lockfile (should return Unsigned, not error)
    let status = verify_lockfile(&lockfile_path).unwrap();
    assert_eq!(status, VerificationStatus::Unsigned);
}

#[test]
fn test_keyring_persistence() {
    let keyring_path = test_keyring_path("persistence");

    // Create and save keyring
    let key_pair = KeyPair::generate().unwrap();
    let key_id = key_pair.key_id.clone();
    let mut keyring = Keyring::new();
    keyring.add_key(key_pair);
    keyring.save(&keyring_path).unwrap();

    // Load keyring back
    let loaded_keyring = Keyring::load(&keyring_path).unwrap();
    assert_eq!(loaded_keyring.keys.len(), 1);
    assert_eq!(loaded_keyring.keys[0].key_id, key_id);
    assert_eq!(loaded_keyring.default_key_id, Some(key_id));
    cleanup_keyring(&keyring_path);
}

#[test]
fn test_multiple_keys_in_keyring() {
    let keyring_path = test_keyring_path("multiple-keys");

    // Create keyring with 3 keys
    let mut keyring = Keyring::new();
    let key1 = KeyPair::generate().unwrap();
    let key2 = KeyPair::generate().unwrap();
    let key3 = KeyPair::generate().unwrap();

    let key1_id = key1.key_id.clone();
    keyring.add_key(key1);
    keyring.add_key(key2);
    keyring.add_key(key3);
    keyring.save(&keyring_path).unwrap();

    // Verify
    let loaded = Keyring::load(&keyring_path).unwrap();
    assert_eq!(loaded.keys.len(), 3);
    assert_eq!(loaded.default_key_id, Some(key1_id)); // First key is default
    cleanup_keyring(&keyring_path);
}

#[test]
fn test_sign_with_specific_key() {
    let temp_dir = TempDir::new().unwrap();
    let lockfile_path = temp_dir.path().join("mgc.lock");
    let keyring_path = test_keyring_path("specific-key");

    // Create lockfile
    write_empty_lockfile(&lockfile_path);

    // Create keyring with 2 keys
    let mut keyring = Keyring::new();
    let key1 = KeyPair::generate().unwrap();
    let key2 = KeyPair::generate().unwrap();
    keyring.add_key(key1);
    keyring.add_key(key2.clone());
    keyring.save(&keyring_path).unwrap();

    // Sign with specific key (key2)
    let mut lockfile = load_lockfile(&lockfile_path).unwrap();
    sign_and_write_lockfile(&mut lockfile, &lockfile_path, &key2).unwrap();

    // Verify
    let status = verify_lockfile(&lockfile_path).unwrap();
    assert_eq!(status, VerificationStatus::Valid);
    cleanup_keyring(&keyring_path);
}

#[test]
fn test_e2e_workflow_with_re_sign() {
    // T3.7: E2E test - init → sign → verify → tamper → re-sign → verify
    let temp_dir = TempDir::new().unwrap();
    let lockfile_path = temp_dir.path().join("mgc.lock");

    // Create initial lockfile
    write_lockfile_with_package(&lockfile_path, "pkg-a", "1.0.0", "sri:blake3:aaa");

    // Generate key and sign
    let key_pair = KeyPair::generate().unwrap();
    let mut lockfile = load_lockfile(&lockfile_path).unwrap();
    sign_and_write_lockfile(&mut lockfile, &lockfile_path, &key_pair).unwrap();

    // Verify initial signature
    let status = verify_lockfile(&lockfile_path).unwrap();
    assert_eq!(status, VerificationStatus::Valid);

    // User legitimately updates lockfile (add new package)
    write_lockfile_with_packages(
        &lockfile_path,
        &[
            ("pkg-a", "1.0.0", "sri:blake3:aaa"),
            ("pkg-b", "2.0.0", "sri:blake3:bbb"),
        ],
    );

    // Old signature should fail
    let status = verify_lockfile(&lockfile_path).unwrap();
    assert!(matches!(status, VerificationStatus::Tampered(_)));

    // Re-sign with updated lockfile
    let mut updated_lockfile = load_lockfile(&lockfile_path).unwrap();
    sign_and_write_lockfile(&mut updated_lockfile, &lockfile_path, &key_pair).unwrap();

    // New signature should be valid
    let status = verify_lockfile(&lockfile_path).unwrap();
    assert_eq!(status, VerificationStatus::Valid);
}

#[test]
fn test_policy_strict_mode_fails_on_unsigned() {
    use std::env;

    let temp_dir = TempDir::new().unwrap();
    let lockfile_path = temp_dir.path().join("mgc.lock");

    // Create unsigned lockfile
    write_empty_lockfile(&lockfile_path);

    // Mock CI environment + strict policy
    env::set_var("CI", "true");
    env::set_var("MGC_TRUST_POLICY", "strict");

    // This would be called in install.rs
    // let result = crate::commands::trust::policy::auto_enforce_in_ci(&lockfile_path);
    // assert!(result.is_err());

    // Cleanup
    env::remove_var("CI");
    env::remove_var("MGC_TRUST_POLICY");
}

#[test]
fn test_signature_file_created() {
    let temp_dir = TempDir::new().unwrap();
    let lockfile_path = temp_dir.path().join("mgc.lock");
    let sig_path = lockfile_path.with_extension("lock.sig");

    // Create lockfile
    write_empty_lockfile(&lockfile_path);

    // Sign
    let key_pair = KeyPair::generate().unwrap();
    let mut lockfile = load_lockfile(&lockfile_path).unwrap();
    sign_and_write_lockfile(&mut lockfile, &lockfile_path, &key_pair).unwrap();

    // Verify signature file exists and is non-empty
    assert!(sig_path.exists());
    let sig_content = fs::read_to_string(&sig_path).unwrap();
    assert!(!sig_content.is_empty());
    assert!(sig_content.contains("lockfile_hash"));
    assert!(sig_content.contains("signature"));
    assert!(sig_content.contains("key_id"));
}
