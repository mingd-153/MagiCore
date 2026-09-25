//! Tamper detection tests
//! Tests phát hiện tamper

use mgc_crypto::keyring::KeyPair;
use mgc_lockfile::{
    Lockfile, Package, VerificationStatus, load_and_verify_lockfile, sign_and_write_lockfile,
    verify_lockfile, write_lockfile,
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
    std::fs::write(
        &lockfile_path,
        mgc_lockfile::serialize_lockfile(&lockfile).unwrap(),
    )
    .unwrap();

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
fn ordinary_writer_refuses_signed_lock_without_changing_either_artifact() {
    let dir = tempdir().unwrap();
    let lockfile_path = dir.path().join("mgc.lock");
    let mut original = Lockfile::new();
    original.add_package(Package::new(
        "trusted-package".to_string(),
        "1.0.0".to_string(),
        "https://registry.example/trusted-package.tgz".to_string(),
        "blake3-trusted".to_string(),
    ));
    let key = KeyPair::generate().unwrap();
    sign_and_write_lockfile(&mut original, &lockfile_path, &key).unwrap();
    let old_lock = std::fs::read(&lockfile_path).unwrap();
    let sig_path = lockfile_path.with_extension("lock.sig");
    let old_sig = std::fs::read(&sig_path).unwrap();

    let mut attempted = original;
    attempted.add_package(Package::new(
        "new-package".to_string(),
        "2.0.0".to_string(),
        "https://registry.example/new-package.tgz".to_string(),
        "blake3-new".to_string(),
    ));
    let error = write_lockfile(&attempted, &lockfile_path).unwrap_err();

    assert!(error.to_string().contains("signed mgc.lock"));
    assert_eq!(std::fs::read(&lockfile_path).unwrap(), old_lock);
    assert_eq!(std::fs::read(&sig_path).unwrap(), old_sig);
    assert_eq!(
        verify_lockfile(&lockfile_path).unwrap(),
        VerificationStatus::Valid
    );
}

#[test]
fn ordinary_writer_refuses_signed_metadata_when_signature_sidecar_is_missing() {
    let dir = tempdir().unwrap();
    let lockfile_path = dir.path().join("mgc.lock");
    let mut original = Lockfile::new();
    original.add_package(Package::new(
        "signed-package".to_string(),
        "1.0.0".to_string(),
        "https://registry.example/signed-package.tgz".to_string(),
        "blake3-signed".to_string(),
    ));
    let key = KeyPair::generate().unwrap();
    sign_and_write_lockfile(&mut original, &lockfile_path, &key).unwrap();
    let sig_path = lockfile_path.with_extension("lock.sig");
    std::fs::remove_file(sig_path).unwrap();
    let old_lock = std::fs::read(&lockfile_path).unwrap();

    let mut attempted = original;
    attempted.add_package(Package::new(
        "new-package".to_string(),
        "2.0.0".to_string(),
        "https://registry.example/new-package.tgz".to_string(),
        "blake3-new".to_string(),
    ));
    let error = write_lockfile(&attempted, &lockfile_path).unwrap_err();

    assert!(error.to_string().contains("signed mgc.lock"));
    assert_eq!(std::fs::read(&lockfile_path).unwrap(), old_lock);
}

#[test]
fn ordinary_writer_refuses_inline_v4_signature_without_sidecar() {
    let dir = tempdir().unwrap();
    let lockfile_path = dir.path().join("mgc.lock");
    let key_pair = KeyPair::generate().unwrap();
    let mut lock = mgc_lockfile::canonical::LockfileV4::new("mgc/test");
    lock.metadata.lockfile_hash = mgc_lockfile::canonical::payload_digest(&lock.payload());
    lock.metadata.signature = Some(
        mgc_lockfile::policy::sign_payload_digest(&lock.metadata.lockfile_hash, &key_pair).unwrap(),
    );
    let bytes = mgc_lockfile::canonical::write_v4_document(&lock).unwrap();
    std::fs::write(&lockfile_path, &bytes).unwrap();

    let error = mgc_lockfile::ensure_lockfile_mutation_allowed(&lockfile_path).unwrap_err();
    assert!(matches!(
        error,
        mgc_lockfile::LockfileError::SignedLockMutation(_)
    ));
    assert_eq!(std::fs::read(lockfile_path).unwrap(), bytes.as_bytes());
}

#[cfg(unix)]
#[test]
fn ordinary_writer_refuses_symlink_lockfile_path() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let target = dir.path().join("outside.txt");
    std::fs::write(&target, b"do not overwrite").unwrap();
    let lockfile_path = dir.path().join("mgc.lock");
    symlink(&target, &lockfile_path).unwrap();

    let error = write_lockfile(&Lockfile::new(), &lockfile_path).unwrap_err();

    assert!(error.to_string().contains("non-regular lockfile path"));
    assert_eq!(std::fs::read(target).unwrap(), b"do not overwrite");
}

#[test]
fn signature_sidecar_key_id_must_match_the_actual_signer() {
    let dir = tempdir().unwrap();
    let lockfile_path = dir.path().join("mgc.lock");
    let mut lockfile = Lockfile::new();
    lockfile.add_package(Package::new(
        "signed-package".to_string(),
        "1.0.0".to_string(),
        "https://registry.example/signed-package.tgz".to_string(),
        "blake3-signed".to_string(),
    ));
    let key = KeyPair::generate().unwrap();
    sign_and_write_lockfile(&mut lockfile, &lockfile_path, &key).unwrap();

    let sig_path = lockfile_path.with_extension("lock.sig");
    let mut signature: mgc_lockfile::SignatureFile =
        std::fs::read_to_string(&sig_path).unwrap().parse().unwrap();
    signature.key_id = "0000000000000000".to_string();
    std::fs::write(&sig_path, signature.to_string()).unwrap();

    assert!(matches!(
        verify_lockfile(&lockfile_path).unwrap(),
        VerificationStatus::InvalidSignature(_)
    ));
}

#[test]
fn project_trust_roots_distinguish_valid_signature_from_trusted_signer() {
    let dir = tempdir().unwrap();
    let lockfile_path = dir.path().join("mgc.lock");
    let mut lockfile = Lockfile::new();
    lockfile.add_package(Package::new(
        "signed-package".to_string(),
        "1.0.0".to_string(),
        "https://registry.example/signed-package.tgz".to_string(),
        "blake3-signed".to_string(),
    ));
    let key = KeyPair::generate().unwrap();
    sign_and_write_lockfile(&mut lockfile, &lockfile_path, &key).unwrap();

    assert_eq!(
        mgc_lockfile::verify_lockfile_with_trust(
            &lockfile_path,
            std::slice::from_ref(&key.key_id),
        )
        .unwrap(),
        VerificationStatus::Valid
    );
    assert_eq!(
        mgc_lockfile::verify_lockfile_with_trust(&lockfile_path, &[]).unwrap(),
        VerificationStatus::UntrustedKey(key.key_id)
    );
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
