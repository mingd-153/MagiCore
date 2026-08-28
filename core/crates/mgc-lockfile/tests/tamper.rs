//! Tamper detection tests
//! Tests phát hiện tamper

use mgc_crypto::keyring::KeyPair;
use mgc_lockfile::{
    load_and_verify_lockfile, sign_and_write_lockfile, verify_lockfile, write_lockfile, Lockfile,
    Package, VerificationStatus,
};
use tempfile::tempdir;

#[test]
fn test_sign_and_verify_roundtrip() {
    let dir = tempdir().unwrap();
    let lockfile_path = dir.path().join("mgc.lock");

    // Create lockfile
    let mut lockfile = Lockfile::new();
    lockfile.add_package(Package::new(
        "react".to_string(),
        "18.2.0".to_string(),
        "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
        "blake3-abc123".to_string(),
    ));

    // Generate key and sign
    let key_pair = KeyPair::generate().unwrap();
    sign_and_write_lockfile(&mut lockfile, &lockfile_path, &key_pair).unwrap();

    // Verify
    let sig_path = lockfile_path.with_extension("lock.sig");
    let loaded = load_and_verify_lockfile(&lockfile_path, &sig_path).unwrap();

    assert_eq!(loaded.packages.len(), 1);
    assert_eq!(loaded.packages[0].name, "react");
}

#[test]
fn test_tamper_detection_manual_edit() {
    let dir = tempdir().unwrap();
    let lockfile_path = dir.path().join("mgc.lock");

    // Create and sign lockfile
    let mut lockfile = Lockfile::new();
    lockfile.add_package(Package::new(
        "react".to_string(),
        "18.2.0".to_string(),
        "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
        "blake3-abc123".to_string(),
    ));

    let key_pair = KeyPair::generate().unwrap();
    sign_and_write_lockfile(&mut lockfile, &lockfile_path, &key_pair).unwrap();

    // Tamper: add malicious package
    lockfile.add_package(Package::new(
        "evil-backdoor".to_string(),
        "1.0.0".to_string(),
        "https://evil.com/backdoor.tgz".to_string(),
        "blake3-evil".to_string(),
    ));

    // Write tampered lockfile (without re-signing)
    write_lockfile(&lockfile, &lockfile_path).unwrap();

    // Verify should fail
    let status = verify_lockfile(&lockfile_path).unwrap();
    assert!(matches!(status, VerificationStatus::Tampered(_)));
}

#[test]
fn test_unsigned_lockfile_warning() {
    let dir = tempdir().unwrap();
    let lockfile_path = dir.path().join("mgc.lock");

    // Create lockfile without signing
    let lockfile = Lockfile::new();
    write_lockfile(&lockfile, &lockfile_path).unwrap();

    // Verify should return Unsigned
    let status = verify_lockfile(&lockfile_path).unwrap();
    assert_eq!(status, VerificationStatus::Unsigned);
}

#[test]
fn test_invalid_signature() {
    let dir = tempdir().unwrap();
    let lockfile_path = dir.path().join("mgc.lock");
    let sig_path = lockfile_path.with_extension("lock.sig");

    // Create and sign lockfile
    let mut lockfile = Lockfile::new();
    lockfile.add_package(Package::new(
        "react".to_string(),
        "18.2.0".to_string(),
        "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
        "blake3-abc123".to_string(),
    ));

    let key_pair = KeyPair::generate().unwrap();
    sign_and_write_lockfile(&mut lockfile, &lockfile_path, &key_pair).unwrap();

    // Corrupt signature file (but keep valid format with wrong signature)
    std::fs::write(
        &sig_path,
        "lockfile_hash = \"blake3-corrupted\"\n\
         signature = \"ed25519-corrupted\"\n\
         key_id = \"fake\"\n\
         signed_at = \"2026-08-21T00:00:00Z\"\n",
    )
    .unwrap();

    // Verify should fail (hash mismatch)
    let status = verify_lockfile(&lockfile_path).unwrap();
    assert!(matches!(status, VerificationStatus::Tampered(_)));
}

#[test]
fn test_tamper_hash_mismatch() {
    let dir = tempdir().unwrap();
    let lockfile_path = dir.path().join("mgc.lock");

    // Create and sign lockfile
    let mut lockfile = Lockfile::new();
    lockfile.add_package(Package::new(
        "react".to_string(),
        "18.2.0".to_string(),
        "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
        "blake3-abc123".to_string(),
    ));

    let key_pair = KeyPair::generate().unwrap();
    sign_and_write_lockfile(&mut lockfile, &lockfile_path, &key_pair).unwrap();

    // Modify lockfile content (simulate manual edit)
    let mut content = std::fs::read_to_string(&lockfile_path).unwrap();
    content.push_str("\n# tampered comment\n");
    std::fs::write(&lockfile_path, content).unwrap();

    // Verify should detect tamper
    let status = verify_lockfile(&lockfile_path).unwrap();
    assert!(matches!(status, VerificationStatus::Tampered(_)));
}
