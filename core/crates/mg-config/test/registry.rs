#![allow(clippy::unwrap_used)]
//! Integration tests for Registry config — test riêng tại test/ (RULE §5)
use mg_config::registry::Registry;

#[test]
fn new_sets_name_url_and_default_priority() {
    let reg = Registry::new("my-reg".into(), "https://example.com".into());
    assert_eq!(reg.name, "my-reg");
    assert_eq!(reg.url, "https://example.com");
    assert_eq!(reg.priority, 0);
    assert_eq!(reg.token, None);
    assert_eq!(reg.username, None);
    assert_eq!(reg.password, None);
    assert_eq!(reg.auth_type, None);
}

#[test]
fn deserializes_without_optional_auth_fields() {
    let reg: Registry = serde_json::from_str(r#"{"name":"r","url":"https://x/"}"#).unwrap();
    assert_eq!(reg.token, None);
    assert_eq!(reg.priority, 0);
}

#[test]
fn deserializes_auth_fields() {
    let reg: Registry = serde_json::from_str(
        r#"{"name":"r","url":"https://x/","priority":5,"token":"t","username":"u","password":"p"}"#,
    )
    .unwrap();
    assert_eq!(reg.priority, 5);
    assert_eq!(reg.token.as_deref(), Some("t"));
    assert_eq!(reg.username.as_deref(), Some("u"));
    assert_eq!(reg.password.as_deref(), Some("p"));
    assert_eq!(reg.auth_type, None);
}

#[test]
fn deserializes_auth_type() {
    let reg: Registry =
        serde_json::from_str(r#"{"name":"r","url":"https://x/","token":"t","auth_type":"token"}"#)
            .unwrap();
    assert_eq!(reg.auth_type.as_deref(), Some("token"));
    let reg: Registry =
        serde_json::from_str(r#"{"name":"r","url":"https://x/","auth_type":"basic"}"#).unwrap();
    assert_eq!(reg.auth_type.as_deref(), Some("basic"));
}
