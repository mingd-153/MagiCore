//! Integration tests for mg-app-adapter — sát với src/lib.rs
//! Kiểm thử: detect_language (all 6 paths), adapter_for, PackageAdapter trait methods.

use mg_app_adapter::{adapter_for, detect_language, AppAdapter, AppLanguage};
use mg_types::adapter::{AddOptions, PackageAdapter};
use mg_types::PackageName;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mg-app-itg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

// ── detect_language — tất cả 6 loại marker ─────────────────────────────────

#[test]
fn detect_flutter_via_pubspec_yaml() {
    let dir = tmp("flutter");
    std::fs::write(dir.join("pubspec.yaml"), "name: myapp\nversion: 1.0.0\n").unwrap();
    assert_eq!(detect_language(&dir), Some(AppLanguage::Flutter));
}

#[test]
fn detect_kotlin_via_build_gradle_kts() {
    let dir = tmp("kotlin-kts");
    std::fs::write(dir.join("build.gradle.kts"), "plugins { kotlin(\"jvm\") }\n").unwrap();
    assert_eq!(detect_language(&dir), Some(AppLanguage::Kotlin));
}

#[test]
fn detect_kotlin_via_build_gradle() {
    let dir = tmp("kotlin-groovy");
    std::fs::write(dir.join("build.gradle"), "apply plugin: 'kotlin'\n").unwrap();
    assert_eq!(detect_language(&dir), Some(AppLanguage::Kotlin));
}

#[test]
fn detect_swift_via_package_swift() {
    let dir = tmp("swift");
    std::fs::write(dir.join("Package.swift"), "// swift-tools-version:5.9\n").unwrap();
    assert_eq!(detect_language(&dir), Some(AppLanguage::Swift));
}

#[test]
fn detect_react_native_via_package_json_dep() {
    let dir = tmp("rn");
    std::fs::write(
        dir.join("package.json"),
        r#"{"name":"rn","dependencies":{"react-native":"0.74.0"}}"#,
    )
    .unwrap();
    assert_eq!(detect_language(&dir), Some(AppLanguage::ReactNative));
}

#[test]
fn detect_objc_via_bridge_header_and_impl_pair() {
    let dir = tmp("objc");
    std::fs::write(dir.join("ObjcBridge.h"), "@interface MGShared\n@end\n").unwrap();
    std::fs::write(dir.join("ObjcBridge.m"), "@implementation MGShared\n@end\n").unwrap();
    assert_eq!(detect_language(&dir), Some(AppLanguage::ObjC));
}

#[test]
fn detect_via_mg_toml_language_overrides_marker() {
    let dir = tmp("mg-override");
    // pubspec.yaml → Flutter, mg.toml override → Kotlin
    std::fs::write(dir.join("pubspec.yaml"), "name: x\n").unwrap();
    std::fs::write(dir.join("mg.toml"), "[app]\nlanguage = \"kotlin\"\n").unwrap();
    assert_eq!(detect_language(&dir), Some(AppLanguage::Kotlin));
}

#[test]
fn detect_multi_via_mg_toml() {
    let dir = tmp("multi");
    std::fs::write(
        dir.join("mg.toml"),
        "[app]\nlanguage = \"multi\"\nplatforms = [\"android\",\"ios\"]\n",
    )
    .unwrap();
    assert_eq!(detect_language(&dir), Some(AppLanguage::Multi));
}

#[test]
fn detect_returns_none_for_empty_dir() {
    let dir = tmp("empty");
    assert!(detect_language(&dir).is_none());
}

#[test]
fn detect_returns_none_for_unknown_mg_toml_language() {
    let dir = tmp("unknown-lang");
    std::fs::write(dir.join("mg.toml"), "[app]\nlanguage = \"xamarin\"\n").unwrap();
    // "xamarin" không được hỗ trợ → None
    assert!(detect_language(&dir).is_none());
}

// ── react_native: package.json không có "react-native" → không phải RN ──────

#[test]
fn package_json_without_react_native_is_not_detected_as_rn() {
    let dir = tmp("non-rn");
    std::fs::write(
        dir.join("package.json"),
        r#"{"name":"web","dependencies":{"react":"^18.0.0"}}"#,
    )
    .unwrap();
    // react thuần web — không phải RN
    assert!(detect_language(&dir).is_none());
}

// ── adapter_for ────────────────────────────────────────────────────────────

#[test]
fn adapter_for_returns_some_with_pubspec_marker() {
    let dir = tmp("af-flutter");
    std::fs::write(dir.join("pubspec.yaml"), "name: a\n").unwrap();
    assert!(adapter_for(&dir).is_some());
}

#[test]
fn adapter_for_returns_none_without_any_marker() {
    let dir = tmp("af-none");
    assert!(adapter_for(&dir).is_none());
}

// ── AppLanguage helpers ────────────────────────────────────────────────────

#[test]
fn applanguage_as_str_values() {
    assert_eq!(AppLanguage::Flutter.as_str(), "flutter");
    assert_eq!(AppLanguage::Kotlin.as_str(), "kotlin");
    assert_eq!(AppLanguage::Swift.as_str(), "swift");
    assert_eq!(AppLanguage::ReactNative.as_str(), "react-native");
    assert_eq!(AppLanguage::ObjC.as_str(), "objc");
    assert_eq!(AppLanguage::Multi.as_str(), "multi");
}

// ── PackageAdapter trait ───────────────────────────────────────────────────

#[test]
fn adapter_name_and_ecosystem() {
    let a = AppAdapter {
        language: AppLanguage::Flutter,
    };
    assert_eq!(a.name(), "app");
    assert_eq!(format!("{:?}", a.ecosystem()), "App");
}

#[test]
fn can_handle_returns_true_for_pubspec_project() {
    let dir = tmp("ch-true");
    std::fs::write(dir.join("pubspec.yaml"), "name: a\n").unwrap();
    let a = AppAdapter {
        language: AppLanguage::Flutter,
    };
    assert!(a.can_handle(&dir));
}

#[test]
fn can_handle_returns_false_for_empty_dir() {
    let dir = tmp("ch-false");
    let a = AppAdapter {
        language: AppLanguage::Flutter,
    };
    assert!(!a.can_handle(&dir));
}

#[tokio::test]
async fn parse_manifest_derives_name_from_dir() {
    let dir = tmp("my-flutter-app");
    std::fs::write(dir.join("pubspec.yaml"), "name: a\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let manifest = a.parse_manifest(&dir).await.unwrap();
    assert!(manifest.name.contains("my-flutter-app"));
}

#[tokio::test]
async fn resolve_returns_empty_graph() {
    let dir = tmp("resolve");
    std::fs::write(dir.join("pubspec.yaml"), "name: a\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let manifest = a.parse_manifest(&dir).await.unwrap();
    let graph = a.resolve(&manifest).await.unwrap();
    assert!(graph.packages.is_empty());
}

#[tokio::test]
async fn install_returns_ok_delegating_to_tooling() {
    let dir = tmp("install-ok");
    std::fs::write(dir.join("pubspec.yaml"), "name: a\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let manifest = a.parse_manifest(&dir).await.unwrap();
    let graph = a.resolve(&manifest).await.unwrap();
    // App install trả Ok — delegating ra flutter/gradle
    let result = a.install(&graph, &dir, Default::default()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn add_fails_closed_directs_to_tooling() {
    let dir = tmp("add-fail");
    std::fs::write(dir.join("pubspec.yaml"), "name: a\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let name = PackageName::new("http").unwrap();
    let err = a.add(&dir, &name, None, AddOptions::default()).await.unwrap_err();
    let msg = err.to_string();
    // Error message phải hướng dẫn user dùng mg install thay vì add trực tiếp
    assert!(
        msg.contains("flutter") || msg.contains("gradle") || msg.contains("mg install"),
        "error must mention tooling: {msg}"
    );
}

#[tokio::test]
async fn remove_fails_closed_directs_to_tooling() {
    let dir = tmp("remove-fail");
    std::fs::write(dir.join("pubspec.yaml"), "name: a\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let name = PackageName::new("http").unwrap();
    assert!(a.remove(&dir, &name).await.is_err());
}

#[tokio::test]
async fn update_fails_closed_directs_to_tooling() {
    let dir = tmp("update-fail");
    std::fs::write(dir.join("pubspec.yaml"), "name: a\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let name = PackageName::new("http").unwrap();
    assert!(a.update(&dir, Some(&name)).await.is_err());
}

#[tokio::test]
async fn audit_returns_clean_for_empty_project() {
    let dir = tmp("audit");
    std::fs::write(dir.join("pubspec.yaml"), "name: a\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let report = a.audit(&dir).await.unwrap();
    assert_eq!(report.vulnerabilities.len(), 0);
}

#[tokio::test]
async fn list_returns_empty_for_no_deps() {
    let dir = tmp("list");
    std::fs::write(dir.join("pubspec.yaml"), "name: a\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let pkgs = a.list(&dir).await.unwrap();
    assert!(pkgs.is_empty());
}
