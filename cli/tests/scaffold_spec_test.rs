//! Tests for scaffold spec parser (cli/src/scaffold/spec.rs)

#![allow(clippy::unwrap_used)]

use mgc::scaffold::spec::{artifact_name, parse_scaffold_spec, CoreKind, ScaffoldRef};

#[test]
fn test_parse_nextjs_latest() {
    let spec = parse_scaffold_spec(CoreKind::Web, "nextjs@latest").unwrap();
    assert_eq!(spec.name, "nextjs");
    assert_eq!(spec.normalized_name, "nextjs");
    assert_eq!(
        spec.requested_ref,
        ScaffoldRef::DistTag("latest".to_string())
    );
    assert_eq!(spec.core, CoreKind::Web);
}

#[test]
fn test_parse_nextjs_version() {
    let spec = parse_scaffold_spec(CoreKind::Web, "nextjs@15.5.0").unwrap();
    assert_eq!(spec.name, "nextjs");
    assert_eq!(spec.normalized_name, "nextjs");
    assert_eq!(
        spec.requested_ref,
        ScaffoldRef::Version("15.5.0".to_string())
    );
}

#[test]
fn test_parse_typo_laster_fails_with_suggestion() {
    let err = parse_scaffold_spec(CoreKind::Web, "nextjs@laster").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("laster"), "Error should mention typo 'laster'");
    assert!(
        msg.contains("latest"),
        "Error should suggest 'latest': {}",
        msg
    );
}

#[test]
fn test_parse_no_version_defaults() {
    let spec = parse_scaffold_spec(CoreKind::Web, "react-vite").unwrap();
    assert_eq!(spec.name, "react-vite");
    assert_eq!(spec.normalized_name, "react-vite");
    assert_eq!(spec.requested_ref, ScaffoldRef::Default);
}

#[test]
fn test_parse_fastapi_for_ai_core() {
    let spec = parse_scaffold_spec(CoreKind::Ai, "fastapi@0.115.0").unwrap();
    assert_eq!(spec.core, CoreKind::Ai);
    assert_eq!(spec.name, "fastapi");
    assert_eq!(
        spec.requested_ref,
        ScaffoldRef::Version("0.115.0".to_string())
    );
}

#[test]
fn test_parse_flutter_for_app_core() {
    let spec = parse_scaffold_spec(CoreKind::App, "flutter@stable").unwrap();
    assert_eq!(spec.core, CoreKind::App);
    assert_eq!(
        spec.requested_ref,
        ScaffoldRef::DistTag("stable".to_string())
    );
}

#[test]
fn test_parse_rust_lib() {
    let spec = parse_scaffold_spec(CoreKind::Lib, "rust@1.80").unwrap();
    assert_eq!(spec.core, CoreKind::Lib);
    assert_eq!(spec.name, "rust");
    assert_eq!(spec.requested_ref, ScaffoldRef::Version("1.80".to_string()));
}

#[test]
fn test_empty_name_fails() {
    let err = parse_scaffold_spec(CoreKind::Web, "@latest").unwrap_err();
    assert!(err.to_string().contains("cannot be empty"));
}

#[test]
fn test_empty_tag_falls_back_to_default() {
    let spec = parse_scaffold_spec(CoreKind::Web, "nextjs@").unwrap();
    assert_eq!(spec.requested_ref, ScaffoldRef::Default);
}

#[test]
fn test_artifact_name_web() {
    assert_eq!(
        artifact_name(CoreKind::Web, "nextjs"),
        "mgc-create-web-nextjs"
    );
}

#[test]
fn test_artifact_name_ai() {
    assert_eq!(
        artifact_name(CoreKind::Ai, "fastapi"),
        "mgc-create-ai-fastapi"
    );
}

#[test]
fn test_artifact_name_app() {
    assert_eq!(
        artifact_name(CoreKind::App, "flutter"),
        "mgc-create-app-flutter"
    );
}

#[test]
fn test_artifact_name_lib() {
    assert_eq!(artifact_name(CoreKind::Lib, "rust"), "mgc-create-lib-rust");
}

#[test]
fn test_normalize_underscore_to_kebab() {
    let spec = parse_scaffold_spec(CoreKind::App, "react_native").unwrap();
    assert_eq!(spec.normalized_name, "react-native");
}

#[test]
fn test_normalize_uppercase_to_lowercase() {
    let spec = parse_scaffold_spec(CoreKind::Web, "NextJS").unwrap();
    assert_eq!(spec.normalized_name, "nextjs");
}

#[test]
fn test_ref_validate_typo_stabl() {
    let ref_val = ScaffoldRef::DistTag("stabl".to_string());
    let suggestion = ref_val.suggest_if_typo();
    assert_eq!(suggestion, Some("stable".to_string()));
}

#[test]
fn test_ref_validate_typo_betta() {
    let ref_val = ScaffoldRef::DistTag("betta".to_string());
    let suggestion = ref_val.suggest_if_typo();
    assert_eq!(suggestion, Some("beta".to_string()));
}

#[test]
fn test_version_range_not_supported_yet() {
    let err = parse_scaffold_spec(CoreKind::Web, "nextjs@^15.0.0").unwrap_err();
    assert!(err.to_string().contains("ranges not supported"));
}

#[test]
fn test_core_kind_from_str() {
    assert_eq!(CoreKind::from_str_core("web"), Some(CoreKind::Web));
    assert_eq!(CoreKind::from_str_core("ai"), Some(CoreKind::Ai));
    assert_eq!(CoreKind::from_str_core("app"), Some(CoreKind::App));
    assert_eq!(CoreKind::from_str_core("lib"), Some(CoreKind::Lib));
    assert_eq!(CoreKind::from_str_core("invalid"), None);
}

#[test]
fn test_core_kind_as_str() {
    assert_eq!(CoreKind::Web.as_str(), "web");
    assert_eq!(CoreKind::Ai.as_str(), "ai");
    assert_eq!(CoreKind::App.as_str(), "app");
    assert_eq!(CoreKind::Lib.as_str(), "lib");
}
