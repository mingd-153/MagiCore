//! v4 trust policy + verification tests (design §3): modes, precedence,
//! tamper fails always, unsigned/foreign-key per policy, sign roundtrip.
//! (Test chính sách tin cậy + xác minh v4.)

use mgc_lockfile::LockfileError;
use mgc_lockfile::canonical::{LockfileV4, payload_digest};
use mgc_lockfile::policy::{
    LockPolicyMode, enforce_policy, resolve_policy, sign_payload_digest, verify_v4_math,
};

fn unsigned_doc() -> LockfileV4 {
    let mut lock = LockfileV4::new("mgc/1.2.0-test");
    lock.metadata.generated_at = "2026-09-17T00:00:00Z".to_string();
    lock.metadata.lockfile_hash = payload_digest(&lock.payload());
    lock
}

#[test]
fn policy_modes_parse_case_insensitively() {
    assert_eq!(LockPolicyMode::parse("off"), Some(LockPolicyMode::Off));
    assert_eq!(LockPolicyMode::parse("WARN"), Some(LockPolicyMode::Warn));
    assert_eq!(
        LockPolicyMode::parse(" Require "),
        Some(LockPolicyMode::Require)
    );
    assert_eq!(LockPolicyMode::parse("sometimes"), None);
}

#[test]
fn flag_beats_everything_in_precedence() {
    // Flag wins even when the environment disagrees — set the env inside
    // a serial guard (see SERIAL below) to avoid cross-test races.
    let _guard = SERIAL.lock().unwrap();
    unsafe_env_set("MGC_LOCK_POLICY", "off");
    assert_eq!(
        resolve_policy(Some("require"), None),
        LockPolicyMode::Require
    );
    unsafe_env_remove("MGC_LOCK_POLICY");
}

#[test]
fn tampered_digest_fails_in_every_mode() {
    let mut lock = unsigned_doc();
    lock.metadata.lockfile_hash = "blake3-tampered".to_string();
    let report = verify_v4_math(&lock).unwrap();
    assert!(!report.digest_ok);
    for policy in [
        LockPolicyMode::Off,
        LockPolicyMode::Warn,
        LockPolicyMode::Require,
    ] {
        assert!(enforce_policy(&report, policy, &[]).is_err());
    }
}

#[test]
fn unsigned_lock_per_policy() {
    let lock = unsigned_doc();
    let report = verify_v4_math(&lock).unwrap();
    assert!(report.digest_ok);
    assert!(!report.signed);
    assert!(enforce_policy(&report, LockPolicyMode::Off, &[]).is_ok());
    assert!(enforce_policy(&report, LockPolicyMode::Warn, &[]).is_ok());
    assert!(enforce_policy(&report, LockPolicyMode::Require, &[]).is_err());
}

#[test]
fn signed_lock_verifies_and_trust_roots_gate_require() {
    let key_pair = mgc_crypto::keyring::KeyPair::generate().unwrap();
    let mut lock = unsigned_doc();
    let digest = lock.metadata.lockfile_hash.clone();
    let block = sign_payload_digest(&digest, &key_pair).unwrap();
    assert_eq!(block.digest, digest);
    assert_eq!(block.algorithm, "ed25519");
    lock.metadata.signature = Some(block);
    let report = verify_v4_math(&lock).unwrap();
    assert!(report.digest_ok && report.signed && report.signature_ok);
    // Trusted key passes require; foreign key fails require but passes
    // warn (advisory, never silent).
    assert!(enforce_policy(&report, LockPolicyMode::Require, &[key_pair.key_id]).is_ok());
    assert!(matches!(
        enforce_policy(
            &report,
            LockPolicyMode::Require,
            &["deadbeefdeadbeef".to_string()]
        ),
        Err(LockfileError::UntrustedKey(_))
    ));
    assert!(enforce_policy(&report, LockPolicyMode::Warn, &[]).is_ok());
}

#[test]
fn signature_over_wrong_digest_fails() {
    let key_pair = mgc_crypto::keyring::KeyPair::generate().unwrap();
    let mut lock = unsigned_doc();
    // Well-formed but unrelated digest: math verifies against the block,
    // yet the block digest disagrees with the payload hash.
    let other = payload_digest(&{
        let mut altered = lock.clone();
        altered.metadata.generator = "mgc/other".to_string();
        altered.payload()
    });
    assert_ne!(other, lock.metadata.lockfile_hash);
    let block = sign_payload_digest(&other, &key_pair).unwrap();
    lock.metadata.signature = Some(block);
    // The block digest disagrees with the file hash: incoherent file,
    // rejected at math stage in every mode (no report to enforce).
    assert!(verify_v4_math(&lock).is_err());
}

#[test]
fn malformed_digest_is_rejected_at_sign_time() {
    let key_pair = mgc_crypto::keyring::KeyPair::generate().unwrap();
    assert!(sign_payload_digest("blake3-!!!not-base64!!!", &key_pair).is_err());
    assert!(sign_payload_digest("no-prefix-at-all", &key_pair).is_err());
}

// Serializes the env-touching tests in this file (process-global env is
// shared by parallel test threads).
// (Tuần tự hóa các test đụng env trong file này.)
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn unsafe_env_set(key: &str, value: &str) {
    // SAFETY: test-only, called only while holding SERIAL, restored by
    // the caller before release. Edition 2024 made this unsafe; the
    // mutex makes it data-race-free.
    // (AN TOÀN: chỉ trong test, giữ SERIAL, caller phục hồi trước khi
    // nhả.)
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var(key, value);
    }
}

fn unsafe_env_remove(key: &str) {
    #[allow(unsafe_code)]
    unsafe {
        std::env::remove_var(key);
    }
}
