#![allow(clippy::unwrap_used)]
//! Core-identity attestation tests — hermetic temp HOME, no real-home writes.
//! (Test chứng thực identity core — HOME tạm hermetic, không đụng HOME thật.)

//! Env mutation is serialized process-wide: every test holds the guard
//! below while a temp HOME is installed, so parallel tests can never
//! observe a torn HOME. Drop restores the previous HOME first.
//! (Đổi env được tuần tự hóa toàn process: mỗi test giữ guard trong lúc
//! HOME tạm được cài, nên test song song không bao giờ thấy HOME rách.
//! Drop khôi phục HOME cũ trước.)
#![allow(unsafe_code)]

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use mgc_config::attestation::{AnchorMode, AttestationStatus, attest, canonical_message, verify};

static HOME_GUARD: Mutex<()> = Mutex::new(());

/// Hermetic HOME + project + identities for one test.
struct Hermetic {
    _lock: MutexGuard<'static, ()>,
    _home: tempfile::TempDir,
    _work: tempfile::TempDir,
    previous_home: Option<std::ffi::OsString>,
    pub keyring: PathBuf,
    pub identities: PathBuf,
    pub root: PathBuf,
}

impl Drop for Hermetic {
    fn drop(&mut self) {
        match self.previous_home.take() {
            Some(home) => unsafe { std::env::set_var("HOME", home) },
            None => unsafe { std::env::remove_var("HOME") },
        }
    }
}

fn hermetic() -> Hermetic {
    let lock = HOME_GUARD.lock().unwrap();
    let home = tempfile::tempdir().unwrap();
    // Canonicalize BEFORE installing HOME: Keyring::save compares the
    // canonical candidate path against $HOME, and tempdirs may sit under
    // symlinked prefixes (/var → /private/var on macOS). The keyring
    // path must be built from the SAME canonical root, or the prefix
    // check compares two different spellings of one directory.
    let home_canon = home.path().canonicalize().unwrap();
    let previous_home = std::env::var_os("HOME");
    unsafe { std::env::set_var("HOME", &home_canon) };
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("proj");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("mgc.toml"),
        "name = \"demo\"\necosystem = \"web\"\n",
    )
    .unwrap();
    std::fs::write(root.join(".mgc.core"), "web\n").unwrap();
    let keyring = home_canon
        .join(".magicore")
        .join("keys")
        .join("keyring.json");
    let identities = work.path().join("identities");
    Hermetic {
        _lock: lock,
        _home: home,
        _work: work,
        previous_home,
        keyring,
        identities,
        root,
    }
}

struct ScopedEnvVar {
    name: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl ScopedEnvVar {
    fn set(name: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(name);
        unsafe { std::env::set_var(name, value) };
        Self { name, previous }
    }
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => unsafe { std::env::set_var(self.name, value) },
            None => unsafe { std::env::remove_var(self.name) },
        }
    }
}

#[test]
fn anchor_mode_parses_and_defaults_to_warn() {
    let _env = hermetic();
    assert_eq!(AnchorMode::parse("off"), Some(AnchorMode::Off));
    assert_eq!(AnchorMode::parse("warn"), Some(AnchorMode::Warn));
    assert_eq!(AnchorMode::parse("require"), Some(AnchorMode::Require));
    assert_eq!(AnchorMode::parse("nope"), None);
}

#[test]
fn canonical_message_is_domain_separated_and_deterministic() {
    let _env = hermetic();
    let first = canonical_message("web", "abc");
    assert!(first.starts_with("mgc-core-attestation/v1\n"));
    assert_eq!(first, canonical_message("web", "abc"));
    assert_ne!(first, canonical_message("ai", "abc"));
    assert_ne!(first, canonical_message("web", "abd"));
}

#[test]
fn attest_then_verify_is_valid_and_idempotent() {
    let env = hermetic();
    let first = attest(
        &env.root,
        "web",
        &env.keyring,
        &env.identities,
        None,
        false,
        false,
    )
    .unwrap();
    assert_eq!(first.core, "web");
    assert!(!first.signature.is_empty());
    assert!(matches!(
        verify(&env.root, &env.identities).unwrap(),
        AttestationStatus::Valid
    ));
    let second = attest(
        &env.root,
        "web",
        &env.keyring,
        &env.identities,
        None,
        false,
        false,
    )
    .unwrap();
    assert_eq!(first.signature, second.signature);
}

#[test]
fn missing_slot_verifies_absent() {
    let env = hermetic();
    assert!(matches!(
        verify(&env.root, &env.identities).unwrap(),
        AttestationStatus::Absent
    ));
}

#[test]
fn edited_config_fails_with_mismatch() {
    let env = hermetic();
    attest(
        &env.root,
        "web",
        &env.keyring,
        &env.identities,
        None,
        false,
        false,
    )
    .unwrap();
    std::fs::write(
        env.root.join("mgc.toml"),
        "name = \"demo\"\necosystem = \"ai\"\n",
    )
    .unwrap();
    assert!(matches!(
        verify(&env.root, &env.identities).unwrap(),
        AttestationStatus::Mismatch(_)
    ));
}

#[test]
fn edited_core_field_breaks_the_signature() {
    let env = hermetic();
    attest(
        &env.root,
        "web",
        &env.keyring,
        &env.identities,
        None,
        false,
        false,
    )
    .unwrap();
    let slot = mgc_config::attestation::attestation_dir(&env.identities, &env.root);
    let path = slot.join(mgc_config::attestation::ATTESTATION_FILE);
    let text = std::fs::read_to_string(&path).unwrap();
    let forged: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(forged["core"], "web");
    let mut map = forged.as_object().unwrap().clone();
    map.insert(
        "core".to_string(),
        serde_json::Value::String("ai".to_string()),
    );
    std::fs::write(&path, serde_json::to_string_pretty(&map).unwrap()).unwrap();
    assert!(matches!(
        verify(&env.root, &env.identities).unwrap(),
        AttestationStatus::Mismatch(_)
    ));
}

#[test]
fn edited_live_core_marker_fails_attestation_verification() {
    let env = hermetic();
    attest(
        &env.root,
        "web",
        &env.keyring,
        &env.identities,
        None,
        false,
        false,
    )
    .unwrap();
    std::fs::write(env.root.join(".mgc.core"), "ai\n").unwrap();

    assert!(matches!(
        verify(&env.root, &env.identities).unwrap(),
        AttestationStatus::Mismatch(_)
    ));
}

#[test]
fn attestation_cannot_sign_a_core_different_from_the_live_marker() {
    let env = hermetic();
    assert!(
        attest(
            &env.root,
            "ai",
            &env.keyring,
            &env.identities,
            None,
            false,
            false,
        )
        .is_err()
    );
}

#[test]
fn mismatched_attestation_requires_explicit_re_attest() {
    let env = hermetic();
    let first = attest(
        &env.root,
        "web",
        &env.keyring,
        &env.identities,
        None,
        false,
        false,
    )
    .unwrap();
    std::fs::write(
        env.root.join("mgc.toml"),
        "name = \"renamed\"\necosystem = \"web\"\n",
    )
    .unwrap();

    assert!(
        attest(
            &env.root,
            "web",
            &env.keyring,
            &env.identities,
            None,
            false,
            false,
        )
        .is_err()
    );

    let replaced = attest(
        &env.root,
        "web",
        &env.keyring,
        &env.identities,
        None,
        true,
        false,
    )
    .unwrap();
    assert_ne!(first.config_digest, replaced.config_digest);
    assert!(matches!(
        verify(&env.root, &env.identities).unwrap(),
        AttestationStatus::Valid
    ));
}

#[test]
fn rotate_keeps_old_signatures_verifiable() {
    let env = hermetic();
    let first = attest(
        &env.root,
        "web",
        &env.keyring,
        &env.identities,
        None,
        false,
        false,
    )
    .unwrap();
    let rotated = attest(
        &env.root,
        "web",
        &env.keyring,
        &env.identities,
        None,
        false,
        true,
    )
    .unwrap();
    assert_ne!(first.key_id, rotated.key_id);
    assert_eq!(rotated.prev_keys.len(), 1);
    assert_eq!(rotated.prev_keys[0].key_id, first.key_id);
    assert!(matches!(
        verify(&env.root, &env.identities).unwrap(),
        AttestationStatus::Valid
    ));
}

#[test]
fn unknown_core_is_refused() {
    let env = hermetic();
    assert!(
        attest(
            &env.root,
            "nope",
            &env.keyring,
            &env.identities,
            None,
            false,
            false
        )
        .is_err()
    );
}

#[test]
fn canonical_cloud_alias_attests_as_clo() {
    let env = hermetic();
    std::fs::write(
        env.root.join("mgc.toml"),
        "name = \"demo\"\necosystem = \"cloud\"\n",
    )
    .unwrap();
    std::fs::write(env.root.join(".mgc.core"), "cloud\n").unwrap();
    let attestation = attest(
        &env.root,
        "cloud",
        &env.keyring,
        &env.identities,
        None,
        false,
        false,
    )
    .unwrap();
    assert_eq!(attestation.core, "clo");
}

#[test]
fn require_mode_fails_closed_when_attestation_state_is_corrupt() {
    let env = hermetic();
    let paths = mgc_platform::paths::GlobalPaths::new().unwrap();
    attest(
        &env.root,
        "web",
        &env.keyring,
        &paths.identities,
        None,
        false,
        false,
    )
    .unwrap();
    let attestation_path = mgc_config::attestation::attestation_dir(&paths.identities, &env.root)
        .join(mgc_config::attestation::ATTESTATION_FILE);
    std::fs::write(&attestation_path, "{").unwrap();
    let _mode = ScopedEnvVar::set("MGC_TRUST_ANCHOR", "require");

    let error = mgc_config::attestation::enforce_live_attestation(&env.root, "web")
        .expect_err("require mode must reject corrupt attestation state");
    assert!(error.to_string().contains("cannot verify core attestation"));
}

#[test]
fn require_mode_fails_closed_when_attestation_is_missing() {
    let env = hermetic();
    let _mode = ScopedEnvVar::set("MGC_TRUST_ANCHOR", "require");

    let error = mgc_config::attestation::enforce_live_attestation(&env.root, "web")
        .expect_err("require mode must reject a missing attestation");
    assert!(error.to_string().contains("no core attestation"));
}
