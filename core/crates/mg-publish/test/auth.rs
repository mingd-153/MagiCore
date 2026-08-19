//! Integration tests for auth resolution — test riêng tại test/ (RULE §5)
use mg_config::npmrc::NpmRc;
use mg_config::registry::Registry;
use mg_publish::auth::{resolve_auth, Auth};

// ponytail: env vars là global — serialize tests dùng env để hết race
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn with_env_lock<T>(f: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap();
    f()
}

#[test]
fn cli_token_wins() {
    let auth = resolve_auth(
        &NpmRc::parse("//registry.npmjs.org/:_authToken=npmrc").unwrap(),
        "https://registry.npmjs.org/",
        None,
        Some("cli-token"),
    )
    .unwrap();
    assert_eq!(auth.token.as_deref(), Some("cli-token"));
}

#[test]
fn env_token_used_when_set() {
    with_env_lock(|| {
        std::env::set_var("MG_NPM_TOKEN", "env-token");
        let auth = resolve_auth(
            &NpmRc::parse("//registry.npmjs.org/:_authToken=npmrc").unwrap(),
            "https://registry.npmjs.org/",
            None,
            None,
        )
        .unwrap();
        assert_eq!(auth.token.as_deref(), Some("env-token"));
        std::env::remove_var("MG_NPM_TOKEN");
        std::env::remove_var("NPM_TOKEN");
    })
}

#[test]
fn npmrc_token_matches_host() {
    with_env_lock(|| {
        std::env::remove_var("MG_NPM_TOKEN");
        std::env::remove_var("NPM_TOKEN");
        let auth = resolve_auth(
            &NpmRc::parse("//registry.npmjs.org/:_authToken=npmrc-token").unwrap(),
            "https://registry.npmjs.org/",
            None,
            None,
        )
        .unwrap();
        assert_eq!(auth.token.as_deref(), Some("npmrc-token"));
    })
}

#[test]
fn mg_toml_registry_token_fallback() {
    with_env_lock(|| {
        std::env::remove_var("MG_NPM_TOKEN");
        std::env::remove_var("NPM_TOKEN");
        let mut reg = Registry::new("r".into(), "https://x/".into());
        reg.token = Some("toml-token".into());
        let auth =
            resolve_auth(&NpmRc::parse("").unwrap(), "https://x/", Some(&reg), None).unwrap();
        assert_eq!(auth.token.as_deref(), Some("toml-token"));
    })
}

#[test]
fn auth_type_basic_forces_basic_auth() {
    with_env_lock(|| {
        std::env::remove_var("MG_NPM_TOKEN");
        std::env::remove_var("NPM_TOKEN");
        // có token nhưng auth_type = "basic" → bỏ qua token config, dùng basic
        let mut reg = Registry::new("r".into(), "https://x/".into());
        reg.token = Some("toml-token".into());
        reg.username = Some("u".into());
        reg.password = Some("p".into());
        reg.auth_type = Some("basic".into());
        let auth =
            resolve_auth(&NpmRc::parse("").unwrap(), "https://x/", Some(&reg), None).unwrap();
        assert_eq!(auth.token, None);
        assert_eq!(auth.username.as_deref(), Some("u"));
        assert_eq!(auth.header_value().as_deref(), Some("Basic dTpw"));
    })
}

#[test]
fn auth_type_basic_without_credentials_errors() {
    with_env_lock(|| {
        std::env::remove_var("MG_NPM_TOKEN");
        std::env::remove_var("NPM_TOKEN");
        let mut reg = Registry::new("r".into(), "https://x/".into());
        reg.token = Some("toml-token".into());
        reg.auth_type = Some("basic".into());
        let err =
            resolve_auth(&NpmRc::parse("").unwrap(), "https://x/", Some(&reg), None).unwrap_err();
        assert!(err.to_string().contains("auth_type"), "err: {err}");
    })
}

#[test]
fn auth_type_token_without_token_errors() {
    with_env_lock(|| {
        std::env::remove_var("MG_NPM_TOKEN");
        std::env::remove_var("NPM_TOKEN");
        let mut reg = Registry::new("r".into(), "https://x/".into());
        reg.username = Some("u".into());
        reg.password = Some("p".into());
        reg.auth_type = Some("token".into());
        let err =
            resolve_auth(&NpmRc::parse("").unwrap(), "https://x/", Some(&reg), None).unwrap_err();
        assert!(err.to_string().contains("auth_type"), "err: {err}");
    })
}

#[test]
fn basic_auth_from_npmrc() {
    with_env_lock(|| {
        std::env::remove_var("MG_NPM_TOKEN");
        std::env::remove_var("NPM_TOKEN");
        let auth = resolve_auth(
            &NpmRc::parse("//priv.example.com/:username=u\n//priv.example.com/:_password=pass")
                .unwrap(),
            "https://priv.example.com/npm",
            None,
            None,
        )
        .unwrap();
        assert_eq!(auth.username.as_deref(), Some("u"));
        assert_eq!(auth.header_value().as_deref(), Some("Basic dTpwYXNz"));
    })
}

#[test]
fn missing_auth_errors() {
    with_env_lock(|| {
        std::env::remove_var("MG_NPM_TOKEN");
        std::env::remove_var("NPM_TOKEN");
        assert!(resolve_auth(
            &NpmRc::parse("").unwrap(),
            "https://registry.npmjs.org/",
            None,
            None
        )
        .is_err());
    })
}

#[test]
fn header_prefers_bearer() {
    let auth = Auth {
        token: Some("t".into()),
        username: Some("u".into()),
        password: Some("p".into()),
    };
    assert_eq!(auth.header_value().as_deref(), Some("Bearer t"));
}

#[test]
fn header_basic_when_no_token() {
    let auth = Auth {
        token: None,
        username: Some("u".into()),
        password: Some("pass".into()),
    };
    assert_eq!(auth.header_value().as_deref(), Some("Basic dTpwYXNz"));
}

#[test]
fn header_none_when_no_auth() {
    let auth = Auth::default();
    assert_eq!(auth.header_value(), None);
}
