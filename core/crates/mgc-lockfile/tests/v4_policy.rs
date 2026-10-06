//! v4 trust policy + verification tests (design §3): modes, precedence,
//! tamper fails always, unsigned/foreign-key per policy, sign roundtrip.
//! (Test chính sách tin cậy + xác minh v4.)

use mgc_lockfile::EcosystemTag;
use mgc_lockfile::LockfileError;
use mgc_lockfile::canonical::{LockfileV4, PackageV4, payload_digest};
use mgc_lockfile::policy::{
    LockPolicyMode, enforce_policy, resolve_policy, sign_payload_digest, verify_v4_file,
    verify_v4_math,
};
use mgc_lockfile::schema::{CrossEdge, WorkspaceTopology};
use mgc_lockfile::v4::{PackageKey, VariantKey, canonical_name, canonical_version};

fn unsigned_doc() -> LockfileV4 {
    let mut lock = LockfileV4::new("mgc/1.2.0-test");
    lock.metadata.generated_at = "2026-09-17T00:00:00Z".to_string();
    lock.metadata.lockfile_hash = payload_digest(&lock.payload());
    lock
}

fn with_python_package(lock: &mut LockfileV4, source_id: &str, variant: VariantKey) {
    lock.packages.push(PackageV4 {
        key: PackageKey {
            ecosystem: EcosystemTag::Python,
            name: canonical_name(EcosystemTag::Python, "Requests"),
            version: canonical_version("2.31.0"),
            source_id: source_id.to_string(),
            variant,
        },
        edges: Vec::new(),
        artifact: None,
        provenance: None,
        toolchain: None,
        scripts_policy: None,
        store_ref: None,
    });
}

fn refresh_digest(lock: &mut LockfileV4) {
    lock.metadata.lockfile_hash = payload_digest(&lock.payload());
}

#[test]
fn shared_lock_verifier_dispatches_v4_inline_signature_and_trust() {
    use mgc_lockfile::{VerificationStatus, canonical::write_v4_document};

    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("mgc.lock");

    let unsigned = unsigned_doc();
    std::fs::write(&lock_path, write_v4_document(&unsigned).unwrap()).unwrap();
    assert_eq!(
        mgc_lockfile::verify_lockfile_with_trust(&lock_path, &[]).unwrap(),
        VerificationStatus::Unsigned
    );

    let key = mgc_crypto::keyring::KeyPair::generate().unwrap();
    let mut signed = unsigned_doc();
    signed.metadata.signature =
        Some(sign_payload_digest(&signed.metadata.lockfile_hash, &key).unwrap());
    std::fs::write(&lock_path, write_v4_document(&signed).unwrap()).unwrap();
    assert_eq!(
        mgc_lockfile::verify_lockfile(&lock_path).unwrap(),
        VerificationStatus::Valid
    );
    assert_eq!(
        mgc_lockfile::verify_lockfile_with_trust(&lock_path, std::slice::from_ref(&key.key_id))
            .unwrap(),
        VerificationStatus::Valid
    );
    assert_eq!(
        mgc_lockfile::verify_lockfile_with_trust(&lock_path, &[]).unwrap(),
        VerificationStatus::UntrustedKey(key.key_id.clone())
    );

    signed.metadata.signature.as_mut().unwrap().signature = "ed25519-not-base64".to_string();
    std::fs::write(&lock_path, write_v4_document(&signed).unwrap()).unwrap();
    assert!(matches!(
        mgc_lockfile::verify_lockfile_with_trust(&lock_path, std::slice::from_ref(&key.key_id))
            .unwrap(),
        VerificationStatus::InvalidSignature(_)
    ));

    signed.metadata.signature =
        Some(sign_payload_digest(&signed.metadata.lockfile_hash, &key).unwrap());
    signed.metadata.generator.push_str("-tampered");
    std::fs::write(&lock_path, write_v4_document(&signed).unwrap()).unwrap();
    assert!(matches!(
        mgc_lockfile::verify_lockfile_with_trust(&lock_path, std::slice::from_ref(&key.key_id))
            .unwrap(),
        VerificationStatus::Tampered(_)
    ));
}

#[test]
fn signed_v4_workspace_and_optimizer_tampering_is_detected() {
    use mgc_lockfile::{VerificationStatus, canonical::write_v4_document};

    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("mgc.lock");
    let key = mgc_crypto::keyring::KeyPair::generate().unwrap();
    let mut signed = unsigned_doc();
    signed.workspace = Some(WorkspaceTopology {
        members: vec!["web".into(), "ai".into()],
        cross_core_edges: vec![CrossEdge {
            from_core: "web".into(),
            to_core: "ai".into(),
            package: "shared-model@1.0.0".into(),
        }],
    });
    signed.optimizer_profile = Some("balanced".into());
    refresh_digest(&mut signed);
    signed.metadata.signature =
        Some(sign_payload_digest(&signed.metadata.lockfile_hash, &key).unwrap());
    std::fs::write(&lock_path, write_v4_document(&signed).unwrap()).unwrap();
    assert_eq!(
        mgc_lockfile::verify_lockfile(&lock_path).unwrap(),
        VerificationStatus::Valid
    );

    let mut workspace_tampered = signed.clone();
    workspace_tampered
        .workspace
        .as_mut()
        .unwrap()
        .members
        .push("game".into());
    std::fs::write(&lock_path, write_v4_document(&workspace_tampered).unwrap()).unwrap();
    assert!(matches!(
        mgc_lockfile::verify_lockfile(&lock_path).unwrap(),
        VerificationStatus::Tampered(_)
    ));

    let mut optimizer_tampered = signed;
    optimizer_tampered.optimizer_profile = Some("high-performance".into());
    std::fs::write(&lock_path, write_v4_document(&optimizer_tampered).unwrap()).unwrap();
    assert!(matches!(
        mgc_lockfile::verify_lockfile(&lock_path).unwrap(),
        VerificationStatus::Tampered(_)
    ));
}

#[test]
fn shared_lock_verifier_rejects_legacy_sidecar_for_v4() {
    use mgc_lockfile::{VerificationStatus, canonical::write_v4_document};

    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("mgc.lock");
    let mut lock = unsigned_doc();
    lock.metadata.signature = Some(
        sign_payload_digest(
            &lock.metadata.lockfile_hash,
            &mgc_crypto::keyring::KeyPair::generate().unwrap(),
        )
        .unwrap(),
    );
    std::fs::write(&lock_path, write_v4_document(&lock).unwrap()).unwrap();
    std::fs::write(dir.path().join("mgc.lock.sig"), "stale legacy sidecar").unwrap();

    let status = mgc_lockfile::verify_lockfile(&lock_path).unwrap();
    assert!(matches!(status, VerificationStatus::InvalidSignature(_)));
}

#[test]
fn v4_verifier_rejects_root_pin_without_a_locked_package() {
    let mut lock = unsigned_doc();
    lock.root_dependencies_by_owner.insert(
        "lib".to_string(),
        vec!["mgc-root-v1:python:requests@2.31.0".to_string()],
    );
    refresh_digest(&mut lock);

    let error = verify_v4_math(&lock).unwrap_err();

    assert!(error.to_string().contains("root pin"));
}

#[test]
fn v4_verifier_rejects_root_pin_ambiguous_across_sources() {
    let mut lock = unsigned_doc();
    with_python_package(&mut lock, "internal", VariantKey::default());
    with_python_package(&mut lock, "public", VariantKey::default());
    lock.root_dependencies_by_owner.insert(
        "lib".to_string(),
        vec!["mgc-root-v1:python:requests@2.31.0".to_string()],
    );
    refresh_digest(&mut lock);

    let error = verify_v4_math(&lock).unwrap_err();

    assert!(error.to_string().contains("ambiguous"));
}

#[test]
fn v4_verifier_rejects_root_pin_ambiguous_across_variants() {
    let mut lock = unsigned_doc();
    with_python_package(&mut lock, "pypi", VariantKey::default());
    with_python_package(
        &mut lock,
        "pypi",
        VariantKey {
            peer_context: Some("peer-set-a".to_string()),
            ..VariantKey::default()
        },
    );
    lock.root_dependencies_by_owner.insert(
        "lib".to_string(),
        vec!["mgc-root-v1:python:requests@2.31.0".to_string()],
    );
    refresh_digest(&mut lock);

    let error = verify_v4_math(&lock).unwrap_err();

    assert!(error.to_string().contains("ambiguous"));
}

#[test]
fn v4_verifier_accepts_one_unambiguous_root_pin_match() {
    let mut lock = unsigned_doc();
    with_python_package(&mut lock, "pypi", VariantKey::default());
    lock.root_dependencies_by_owner.insert(
        "lib".to_string(),
        vec!["mgc-root-v1:python:requests@2.31.0".to_string()],
    );
    refresh_digest(&mut lock);

    let report = verify_v4_math(&lock).unwrap();

    assert!(report.digest_ok);
}

#[test]
fn v4_verifier_resolves_legacy_npm_protocol_prefix() {
    let mut lock = unsigned_doc();
    lock.packages.push(PackageV4 {
        key: PackageKey {
            ecosystem: EcosystemTag::Web,
            name: "lodash".to_string(),
            version: "4.17.20".to_string(),
            source_id: "npm".to_string(),
            variant: VariantKey::default(),
        },
        edges: Vec::new(),
        artifact: None,
        provenance: None,
        toolchain: None,
        scripts_policy: None,
        store_ref: None,
    });
    lock.root_dependencies
        .push("npm:lodash@4.17.20".to_string());
    refresh_digest(&mut lock);

    assert!(verify_v4_math(&lock).unwrap().digest_ok);
}

#[test]
fn v4_verifier_rejects_malformed_qualified_root_pin() {
    let mut lock = unsigned_doc();
    lock.root_dependencies_by_owner.insert(
        "lib".to_string(),
        vec!["mgc-root-v1:unknown:requests@2.31.0".to_string()],
    );
    refresh_digest(&mut lock);

    assert!(verify_v4_math(&lock).is_err());
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
fn ci_policy_cannot_be_downgraded_by_flag_env_or_project_config() {
    let _guard = SERIAL.lock().unwrap();
    let previous_ci = std::env::var_os("CI");
    let previous_policy = std::env::var_os("MGC_LOCK_POLICY");
    unsafe_env_set("CI", "true");
    unsafe_env_set("MGC_LOCK_POLICY", "off");
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("mgc.toml"), "[lock]\npolicy = 'off'\n").unwrap();

    let resolved = resolve_policy(Some("off"), Some(root.path()));

    restore_env("CI", previous_ci);
    restore_env("MGC_LOCK_POLICY", previous_policy);
    assert_eq!(resolved, LockPolicyMode::Require);
}

#[cfg(unix)]
#[test]
fn project_lock_policy_ignores_external_config_symlink() {
    let _guard = SERIAL.lock().unwrap();
    let previous_ci = std::env::var_os("CI");
    let previous_policy = std::env::var_os("MGC_LOCK_POLICY");
    unsafe_env_remove("CI");
    unsafe_env_remove("MGC_LOCK_POLICY");
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    std::fs::write(
        external.path().join("mgc.toml"),
        "[lock]\npolicy = 'require'\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        external.path().join("mgc.toml"),
        root.path().join("mgc.toml"),
    )
    .unwrap();

    let resolved = resolve_policy(None, Some(root.path()));
    restore_env("CI", previous_ci);
    restore_env("MGC_LOCK_POLICY", previous_policy);
    assert_eq!(resolved, LockPolicyMode::Warn);
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
fn v4_signature_cannot_claim_another_keys_trusted_id() {
    let key_pair = mgc_crypto::keyring::KeyPair::generate().unwrap();
    let mut lock = unsigned_doc();
    let digest = lock.metadata.lockfile_hash.clone();
    let mut block = sign_payload_digest(&digest, &key_pair).unwrap();
    block.key_id = "deadbeefdeadbeef".to_string();
    lock.metadata.signature = Some(block);

    assert!(verify_v4_math(&lock).is_err());
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

#[cfg(unix)]
#[test]
fn v4_file_verifier_refuses_symlinked_lockfiles() {
    let dir = tempfile::tempdir().unwrap();
    let real_lock = dir.path().join("real.lock");
    let linked_lock = dir.path().join("mgc.lock");
    let lock = unsigned_doc();
    std::fs::write(
        &real_lock,
        mgc_lockfile::canonical::write_v4_document(&lock).unwrap(),
    )
    .unwrap();
    std::os::unix::fs::symlink(&real_lock, &linked_lock).unwrap();

    let result = verify_v4_file(&linked_lock, LockPolicyMode::Warn, &[]);

    assert!(
        result.is_err(),
        "verifier must not follow a lockfile symlink"
    );
}

#[test]
fn v4_file_verifier_rejects_oversized_lockfiles() {
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("mgc.lock");
    let mut lock = unsigned_doc();
    lock.metadata.generator = "x".repeat(10 * 1024 * 1024 + 1);
    lock.metadata.lockfile_hash = payload_digest(&lock.payload());
    std::fs::write(
        &lock_path,
        mgc_lockfile::canonical::write_v4_document(&lock).unwrap(),
    )
    .unwrap();

    let result = verify_v4_file(&lock_path, LockPolicyMode::Warn, &[]);

    assert!(
        result.is_err(),
        "verifier must enforce the lockfile size cap"
    );
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

fn restore_env(key: &str, value: Option<std::ffi::OsString>) {
    // SAFETY: test-only, called while holding SERIAL to restore process env.
    // (AN TOÀN: chỉ trong test, đang giữ SERIAL để khôi phục env tiến trình.)
    #[allow(unsafe_code)]
    unsafe {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }
}
