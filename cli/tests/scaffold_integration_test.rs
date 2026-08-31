//! Integration tests for scaffold system - early validation

use mgc::scaffold::spec::{parse_scaffold_spec, CoreKind};

#[test]
fn test_web_nextjs_laster_typo_fails_early() {
    let result = parse_scaffold_spec(CoreKind::Web, "nextjs@laster");
    assert!(result.is_err(), "Should fail on typo");

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("laster"),
        "Error should mention the typo: {}",
        err_msg
    );
    assert!(
        err_msg.contains("latest"),
        "Error should suggest 'latest': {}",
        err_msg
    );
}

#[test]
fn test_web_nextjs_latest_succeeds() {
    let spec = parse_scaffold_spec(CoreKind::Web, "nextjs@latest").unwrap();
    assert_eq!(spec.name, "nextjs");
    assert_eq!(spec.core, CoreKind::Web);
}

#[test]
fn test_ai_fastapi_laster_typo_fails() {
    let result = parse_scaffold_spec(CoreKind::Ai, "fastapi@laster");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("latest"));
}

#[test]
fn test_app_flutter_stabl_typo_suggests_stable() {
    let result = parse_scaffold_spec(CoreKind::App, "flutter@stabl");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("stable"));
}

#[test]
fn test_game_bevy_betta_typo_suggests_beta() {
    let result = parse_scaffold_spec(CoreKind::Game, "bevy@betta");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("beta"));
}

#[test]
fn test_multi_core_version_specs() {
    let web = parse_scaffold_spec(CoreKind::Web, "react@18.2.0").unwrap();
    assert_eq!(web.name, "react");

    let ai = parse_scaffold_spec(CoreKind::Ai, "pytorch@2.1.0").unwrap();
    assert_eq!(ai.name, "pytorch");

    let app = parse_scaffold_spec(CoreKind::App, "flutter@3.16.0").unwrap();
    assert_eq!(app.name, "flutter");

    let lib = parse_scaffold_spec(CoreKind::Lib, "rust@1.75.0").unwrap();
    assert_eq!(lib.name, "rust");
}

#[test]
fn test_empty_framework_name_fails() {
    let result = parse_scaffold_spec(CoreKind::Web, "@latest");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("cannot be empty"));
}

#[test]
fn test_version_range_not_supported() {
    let result = parse_scaffold_spec(CoreKind::Web, "nextjs@^15.0.0");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("ranges not supported"));
}

#[test]
fn test_normalize_uppercase_framework() {
    let spec = parse_scaffold_spec(CoreKind::Web, "NextJS@latest").unwrap();
    assert_eq!(spec.normalized_name, "nextjs");
}

#[test]
fn test_normalize_underscore_framework() {
    let spec = parse_scaffold_spec(CoreKind::App, "react_native@0.73.0").unwrap();
    assert_eq!(spec.normalized_name, "react-native");
}
