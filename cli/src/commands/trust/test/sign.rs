//! Lock-signing concurrency regressions.

use super::*;
use std::time::Duration;

#[test]
fn signing_refuses_to_race_a_live_project_install_before_reading_lock() {
    let temp = tempfile::tempdir().unwrap();
    let mut project = mgc_config::project::ProjectConfig::new("sign-test", "web");
    project.lock = Some(mgc_config::project::LockConfig {
        acquire_timeout_ms: Some(1),
        ..Default::default()
    });
    project.save(temp.path()).unwrap();
    let lock_path = temp.path().join("mgc.lock");
    std::fs::write(&lock_path, "not a valid lockfile").unwrap();
    let before = std::fs::read(&lock_path).unwrap();
    let _held =
        mgc_lockfile::project_lock::ProjectWriteLock::acquire(temp.path(), Duration::from_secs(1))
            .unwrap();

    let result = execute(lock_path.to_str().unwrap(), None);

    assert!(result.is_err(), "signing must refuse a live install lock");
    assert!(
        result.unwrap_err().to_string().contains("writer lock"),
        "lock contention must be reported before parsing malformed lock bytes"
    );
    assert_eq!(std::fs::read(lock_path).unwrap(), before);
    assert!(!temp.path().join("mgc.lock.sig").exists());
}

#[test]
fn signing_v4_writes_inline_signature_atomically_without_legacy_sidecar() {
    let temp = tempfile::tempdir().unwrap();
    let project = mgc_config::project::ProjectConfig::new("sign-v4-test", "web");
    project.save(temp.path()).unwrap();
    let lock_path = temp.path().join("mgc.lock");
    let mut document = mgc_lockfile::canonical::LockfileV4::new("mgc-test");
    document.metadata.generated_at = "2026-10-01T00:00:00Z".to_string();
    std::fs::write(
        &lock_path,
        mgc_lockfile::canonical::write_v4_document(&document).unwrap(),
    )
    .unwrap();
    let key = mgc_crypto::keyring::KeyPair::generate().unwrap();
    let project_lock =
        mgc_lockfile::project_lock::ProjectWriteLock::acquire(temp.path(), Duration::from_secs(1))
            .unwrap();

    sign_with_key_locked(&lock_path, &key, &project_lock).unwrap();

    let status =
        mgc_lockfile::verify_lockfile_with_trust(&lock_path, std::slice::from_ref(&key.key_id))
            .unwrap();
    assert_eq!(status, mgc_lockfile::VerificationStatus::Valid);
    assert!(
        !lock_path.with_extension("lock.sig").exists(),
        "v4 signatures belong inline in mgc.lock"
    );
    let parsed = mgc_lockfile::load_lock_document(&lock_path).unwrap();
    let mgc_lockfile::LockDocument::V4(parsed) = parsed else {
        panic!("v4 signing must preserve the v4 document schema");
    };
    assert_eq!(
        parsed.metadata.signature.as_ref().unwrap().key_id,
        key.key_id
    );
}

#[test]
fn signing_v4_refuses_legacy_sidecar_without_mutating_lock() {
    let temp = tempfile::tempdir().unwrap();
    let lock_path = temp.path().join("mgc.lock");
    let mut document = mgc_lockfile::canonical::LockfileV4::new("mgc-test");
    document.metadata.generated_at = "2026-10-01T00:00:00Z".to_string();
    let before = mgc_lockfile::canonical::write_v4_document(&document).unwrap();
    std::fs::write(&lock_path, &before).unwrap();
    let sidecar = lock_path.with_extension("lock.sig");
    std::fs::write(&sidecar, "legacy sidecar").unwrap();
    let key = mgc_crypto::keyring::KeyPair::generate().unwrap();
    let project_lock =
        mgc_lockfile::project_lock::ProjectWriteLock::acquire(temp.path(), Duration::from_secs(1))
            .unwrap();

    let result = sign_with_key_locked(&lock_path, &key, &project_lock);

    assert!(result.is_err());
    assert_eq!(std::fs::read(&lock_path).unwrap(), before.as_bytes());
    assert_eq!(std::fs::read_to_string(sidecar).unwrap(), "legacy sidecar");
}

#[test]
fn signing_v4_refuses_to_bless_an_invalid_existing_signature() {
    let temp = tempfile::tempdir().unwrap();
    let lock_path = temp.path().join("mgc.lock");
    let key = mgc_crypto::keyring::KeyPair::generate().unwrap();
    let digest = mgc_lockfile::canonical::payload_digest(
        &mgc_lockfile::canonical::LockfileV4::new("mgc-test").payload(),
    );
    let signature = mgc_lockfile::policy::sign_payload_digest(&digest, &key).unwrap();
    let mut document = mgc_lockfile::canonical::LockfileV4::new("mgc-test");
    document.metadata.generated_at = "2026-10-01T00:00:00Z".to_string();
    document.metadata.lockfile_hash = digest;
    document.metadata.signature = Some(signature);
    document.metadata.signature.as_mut().unwrap().signature = "ed25519-invalid".to_string();
    let before = mgc_lockfile::canonical::write_v4_document(&document).unwrap();
    std::fs::write(&lock_path, &before).unwrap();
    let project_lock =
        mgc_lockfile::project_lock::ProjectWriteLock::acquire(temp.path(), Duration::from_secs(1))
            .unwrap();

    let result = sign_with_key_locked(&lock_path, &key, &project_lock);

    assert!(result.is_err());
    assert_eq!(std::fs::read(&lock_path).unwrap(), before.as_bytes());
}
