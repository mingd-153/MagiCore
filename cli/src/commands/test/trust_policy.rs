#![allow(clippy::unwrap_used)]
// Trust-policy contract tests — kiểm tra hợp đồng policy khóa tin cậy.

use super::*;
use mgc_crypto::keyring::KeyPair;
use mgc_lockfile::{Lockfile, Package, sign_and_write_lockfile};
use tempfile::tempdir;

#[test]
fn strict_project_policy_requires_a_matching_trust_root() {
    let dir = tempdir().unwrap();
    let lock_path = dir.path().join("mgc.lock");
    let mut lock = Lockfile::new();
    lock.add_package(Package::new(
        "fixture".to_string(),
        "1.0.0".to_string(),
        "https://registry.example/fixture.tgz".to_string(),
        "blake3-fixture".to_string(),
    ));
    let key = KeyPair::generate().unwrap();
    sign_and_write_lockfile(&mut lock, &lock_path, &key).unwrap();

    std::fs::write(
        dir.path().join("mgc.toml"),
        "name = \"fixture\"\necosystem = \"web\"\n[lock]\npolicy = \"require\"\n[trust]\nkeys = [\"0000000000000000\"]\n",
    )
    .unwrap();
    assert_eq!(
        verify_project_lockfile(&lock_path).unwrap(),
        mgc_lockfile::VerificationStatus::UntrustedKey(key.key_id.clone())
    );
    assert!(enforce_policy(&lock_path, PolicyMode::Strict).is_err());

    std::fs::write(
        dir.path().join("mgc.toml"),
        format!(
            "name = \"fixture\"\necosystem = \"web\"\n[lock]\npolicy = \"require\"\n[trust]\nkeys = [\"{}\"]\n",
            key.key_id
        ),
    )
    .unwrap();
    enforce_policy(&lock_path, PolicyMode::Strict).unwrap();
}

#[test]
fn warn_project_policy_accepts_but_identifies_untrusted_signer() {
    let dir = tempdir().unwrap();
    let lock_path = dir.path().join("mgc.lock");
    let mut lock = Lockfile::new();
    lock.add_package(Package::new(
        "fixture".to_string(),
        "1.0.0".to_string(),
        "https://registry.example/fixture.tgz".to_string(),
        "blake3-fixture".to_string(),
    ));
    let key = KeyPair::generate().unwrap();
    sign_and_write_lockfile(&mut lock, &lock_path, &key).unwrap();
    std::fs::write(
        dir.path().join("mgc.toml"),
        "name = \"fixture\"\necosystem = \"web\"\n[lock]\npolicy = \"warn\"\n[trust]\nkeys = []\n",
    )
    .unwrap();

    assert_eq!(
        verify_project_lockfile(&lock_path).unwrap(),
        mgc_lockfile::VerificationStatus::UntrustedKey(key.key_id)
    );
    enforce_policy(&lock_path, PolicyMode::Warn).unwrap();
}

#[test]
fn neither_project_nor_environment_can_downgrade_ci() {
    assert_eq!(
        resolve_effective_mode(Some("off"), None, true),
        PolicyMode::Strict
    );
    assert_eq!(
        resolve_effective_mode(None, Some("warn"), true),
        PolicyMode::Strict
    );
    assert_eq!(
        resolve_effective_mode(None, Some("off"), true),
        PolicyMode::Strict
    );
    assert_eq!(
        resolve_effective_mode(Some("warn"), Some("strict"), false),
        PolicyMode::Strict
    );
    assert_eq!(
        resolve_effective_mode(Some("require"), None, false),
        PolicyMode::Strict
    );
    assert_eq!(
        resolve_effective_mode(Some("off"), None, false),
        PolicyMode::Audit
    );
    assert_eq!(resolve_effective_mode(None, None, false), PolicyMode::Warn);
}
