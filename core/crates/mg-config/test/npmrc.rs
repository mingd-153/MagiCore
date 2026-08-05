//! Integration tests for npmrc parser — test riêng đặt tại test/ (RULE §5)
use mg_config::npmrc::NpmRc;

#[test]
fn parses_registry_and_scopes() {
    let rc = NpmRc::parse(
        "registry=https://registry.npmjs.org/\n@myscope:registry=https://mg.example.com/npm\n",
    )
    .unwrap();
    assert_eq!(rc.registry.as_deref(), Some("https://registry.npmjs.org/"));
    assert_eq!(
        rc.scope_registries.get("@myscope").map(String::as_str),
        Some("https://mg.example.com/npm")
    );
}

#[test]
fn parses_auth_token_and_basic_auth() {
    let rc = NpmRc::parse(
        "//registry.npmjs.org/:_authToken=abc123\n//registry.npmjs.org/:username=user\n//registry.npmjs.org/:_password=ZGVtbw==\n",
    )
    .unwrap();
    assert_eq!(rc.token_for("registry.npmjs.org").map(String::as_str), Some("abc123"));
    let (user, pass) = rc.basic_auth.get("registry.npmjs.org").unwrap();
    assert_eq!(user, "user");
    assert_eq!(pass, "ZGVtbw==");
}

#[test]
fn expands_env_vars() {
    std::env::set_var("MG_TEST_TOKEN", "tok123");
    let rc = NpmRc::parse("//registry.npmjs.org/:_authToken=${MG_TEST_TOKEN}\n").unwrap();
    assert_eq!(rc.token_for("registry.npmjs.org").map(String::as_str), Some("tok123"));
    std::env::remove_var("MG_TEST_TOKEN");
}

#[test]
fn ignores_comments_and_unknown_keys() {
    let rc = NpmRc::parse("# comment\n; semicolon comment\ncache=/tmp\nregistry=https://x/\n")
        .unwrap();
    assert_eq!(rc.registry.as_deref(), Some("https://x/"));
    assert!(rc.auth_tokens.is_empty());
}

#[test]
fn registry_for_scope_prefers_scope() {
    let rc = NpmRc::parse("registry=https://npmjs.org/\n@a:registry=https://priv/\n").unwrap();
    assert_eq!(rc.registry_for(Some("@a")).as_deref(), Some("https://priv/"));
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
    assert_eq!(rc.token_for("registry.npmjs.org").map(String::as_str), Some("tok"));
    assert_eq!(rc.token_for("registry.npmjs.org/").map(String::as_str), Some("tok"));
}
