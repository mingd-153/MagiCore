#![allow(clippy::unwrap_used)]
// Tests mutate env vars single-threaded per test process (edition 2024 unsafe rule).
// Test đổi env var đơn luồng theo từng process test (luật unsafe edition 2024).
#![allow(unsafe_code)]
//! Integration tests for npmrc parser — test riêng đặt tại test/ (RULE §5)
use mgc_config::npmrc::NpmRc;

#[test]
fn parses_registry_and_scopes() {
    let rc = NpmRc::parse(
        "registry=https://registry.npmjs.org/\n@myscope:registry=https://mgc.example.com/npm\n",
    )
    .unwrap();
    assert_eq!(rc.registry.as_deref(), Some("https://registry.npmjs.org/"));
    assert_eq!(
        rc.scope_registries.get("@myscope").map(String::as_str),
        Some("https://mgc.example.com/npm")
    );
}

#[test]
fn parses_auth_token_and_basic_auth() {
    let rc = NpmRc::parse(
        "//registry.npmjs.org/:_authToken=abc123\n//registry.npmjs.org/:username=user\n//registry.npmjs.org/:_password=ZGVtbw==\n",
    )
    .unwrap();
    assert_eq!(
        rc.token_for("registry.npmjs.org").map(String::as_str),
        Some("abc123")
    );
    let (user, pass) = rc.basic_auth.get("registry.npmjs.org").unwrap();
    assert_eq!(user, "user");
    assert_eq!(pass, "ZGVtbw==");
}

#[test]
fn expands_env_vars() {
    unsafe { std::env::set_var("MGC_TEST_TOKEN", "tok123") };
    let rc = NpmRc::parse("//registry.npmjs.org/:_authToken=${MGC_TEST_TOKEN}\n").unwrap();
    assert_eq!(
        rc.token_for("registry.npmjs.org").map(String::as_str),
        Some("tok123")
    );
    unsafe { std::env::remove_var("MGC_TEST_TOKEN") };
}

#[test]
fn ignores_comments_and_unknown_keys() {
    let rc =
        NpmRc::parse("# comment\n; semicolon comment\ncache=/tmp\nregistry=https://x/\n").unwrap();
    assert_eq!(rc.registry.as_deref(), Some("https://x/"));
    assert!(rc.auth_tokens.is_empty());
}

#[test]
fn registry_for_scope_prefers_scope() {
    let rc = NpmRc::parse("registry=https://npmjs.org/\n@a:registry=https://priv/\n").unwrap();
    assert_eq!(
        rc.registry_for(Some("@a")).as_deref(),
        Some("https://priv/")
    );
    assert_eq!(rc.registry_for(None).as_deref(), Some("https://npmjs.org/"));
}

#[test]
fn basic_auth_password_before_username() {
    let rc = NpmRc::parse("//h/:_password=ZGVtbw==\n//h/:username=u\n").unwrap();
    let (user, pass) = rc.basic_auth.get("h").unwrap();
    assert_eq!(user, "u");
    assert_eq!(pass, "ZGVtbw==");
}

#[test]
fn host_normalization_drops_slashes() {
    let rc = NpmRc::parse("//registry.npmjs.org/:_authToken=tok\n").unwrap();
    assert_eq!(
        rc.token_for("registry.npmjs.org").map(String::as_str),
        Some("tok")
    );
    assert_eq!(
        rc.token_for("registry.npmjs.org/").map(String::as_str),
        Some("tok")
    );
}

#[test]
fn debug_output_never_contains_tokens() {
    let rc = NpmRc::parse("//h/:_authToken=SECRET\n//h/:_password=U2VjcmV0\n").unwrap();
    let debug = format!("{rc:?}");
    assert!(!debug.contains("SECRET"), "token leaked: {debug}");
    assert!(!debug.contains("U2VjcmV0"), "password leaked: {debug}");
    let registry = mgc_config::registry::Registry {
        token: Some("TOK".to_string()),
        password: Some("PW".to_string()),
        ..mgc_config::registry::Registry::new("r".to_string(), "https://r".to_string())
    };
    let debug = format!("{registry:?}");
    assert!(!debug.contains("TOK"), "token leaked: {debug}");
    assert!(!debug.contains("PW"), "password leaked: {debug}");
    assert!(
        debug.contains("[REDACTED]"),
        "redaction marker missing: {debug}"
    );
}

#[test]
fn save_auth_token_refuses_symlink_and_sets_strict_perms() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("real-npmrc");
    std::fs::write(&target, "registry=https://x/\n").unwrap();
    let link = dir.path().join(".npmrc");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    #[cfg(unix)]
    {
        let error = NpmRc::save_auth_token(&link, "example.com", "TOK").unwrap_err();
        assert!(error.to_string().contains("symlink"), "{error:#}");
        // Target untouched — no token written through the link.
        assert!(!std::fs::read_to_string(&target).unwrap().contains("TOK"));
    }
    let plain = dir.path().join("plain-npmrc");
    NpmRc::save_auth_token(&plain, "example.com", "TOK").unwrap();
    let content = std::fs::read_to_string(&plain).unwrap();
    assert!(content.contains("//example.com/:_authToken=TOK"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&plain).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "token file must be owner-only, got {mode:o}");
    }
}

#[test]
fn project_load_refuses_symlinked_npmrc() {
    let dir = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let target = external.path().join("npmrc");
    std::fs::write(&target, "registry=https://x/\n").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, dir.path().join(".npmrc")).unwrap();
        let error = NpmRc::load(dir.path()).unwrap_err();
        assert!(error.to_string().contains("symlink"), "{error:#}");
    }
}
