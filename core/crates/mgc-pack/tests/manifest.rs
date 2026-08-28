#![allow(clippy::unwrap_used)]
//! Integration tests for manifest sanitize — test riêng tại test/ (RULE §5)
use mgc_pack::manifest::{dep_fields, sanitize};
use serde_json::Value;

fn manifest(v: &str) -> Value {
    serde_json::from_str(v).unwrap()
}

#[test]
fn removes_private_and_publish_config() {
    let m = manifest(
        r#"{"name":"x","version":"1.0.0","private":true,"publishConfig":{"registry":"r"},"main":"index.js"}"#,
    );
    assert!(
        sanitize(m, "", "").is_err(),
        "private: true must block publish"
    );
}

#[test]
fn strips_publish_config_but_keeps_main() {
    let m = manifest(
        r#"{"name":"x","version":"1.0.0","publishConfig":{"registry":"r"},"main":"index.js"}"#,
    );
    let s = sanitize(m, "", "").unwrap();
    assert!(s.manifest.get("publishConfig").is_none());
    assert_eq!(s.manifest["main"], "index.js");
}

#[test]
fn overrides_name_and_version() {
    let m = manifest(r#"{"name":"old","version":"1.0.0"}"#);
    let s = sanitize(m, "new-name", "2.0.0").unwrap();
    assert_eq!(s.manifest["name"], "new-name");
    assert_eq!(s.manifest["version"], "2.0.0");
    assert_eq!(s.name, "new-name");
    assert_eq!(s.version, "2.0.0");
}

#[test]
fn missing_name_or_version_bails() {
    assert!(sanitize(manifest(r#"{"version":"1.0.0"}"#), "", "").is_err());
    assert!(sanitize(manifest(r#"{"name":"x"}"#), "", "").is_err());
}

#[test]
fn keeps_dep_fields() {
    let m = manifest(
        r#"{"name":"x","version":"1.0.0","dependencies":{"a":"1"},"devDependencies":{"b":"2"},"peerDependencies":{"c":"3"},"optionalDependencies":{"d":"4"}}"#,
    );
    let s = sanitize(m, "", "").unwrap();
    let deps = dep_fields(&s.manifest);
    assert_eq!(deps.len(), 4);
    assert_eq!(deps["dependencies"]["a"], "1");
}

#[test]
fn sanitize_preserves_exports_field() {
    let m = manifest(r#"{"name":"x","version":"1.0.0","exports":{".":"./index.js"}}"#);
    let s = sanitize(m, "", "").unwrap();
    assert_eq!(s.manifest["exports"]["."], "./index.js");
}
