#![allow(clippy::unwrap_used)]
//! Integration tests for UserConfig — test riêng tại test/ (RULE §5)
use mgc_config::user::UserConfig;

#[test]
fn default_values() {
    let cfg = UserConfig::default();
    assert_eq!(cfg.name, None);
    assert_eq!(cfg.email, None);
}

#[test]
fn serializes_and_deserializes() {
    let cfg = UserConfig {
        name: Some("user".into()),
        email: Some("u@example.com".into()),
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let back: UserConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.name.as_deref(), Some("user"));
    assert_eq!(back.email.as_deref(), Some("u@example.com"));
}
