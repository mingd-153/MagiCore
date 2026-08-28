#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Manifest parsing tests for app adapter.

use mgc_app_adapter::manifest::parse_manifest;
use mgc_app_adapter::AppLanguage;

fn tmp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-app-manifest-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

#[test]
fn parse_flutter_pubspec_with_dependencies() {
    let dir = tmp("flutter");
    std::fs::write(
        dir.join("pubspec.yaml"),
        "name: myapp\ndependencies:\n  http: ^1.0.0\n  provider: ^6.0.0\n",
    )
    .unwrap();

    let manifest = parse_manifest(AppLanguage::Flutter, &dir).unwrap();
    assert_eq!(manifest.name, "myapp");
    assert!(manifest.find_dep("http").is_some());
    assert!(manifest.find_dep("provider").is_some());
}

#[test]
fn parse_flutter_pubspec_empty_deps() {
    let dir = tmp("flutter-empty");
    std::fs::write(dir.join("pubspec.yaml"), "name: empty\n").unwrap();

    let manifest = parse_manifest(AppLanguage::Flutter, &dir).unwrap();
    assert_eq!(manifest.name, "empty");
}

#[test]
fn parse_kotlin_gradle_returns_manifest() {
    let dir = tmp("kotlin");
    std::fs::write(dir.join("build.gradle"), "// gradle build\n").unwrap();

    let manifest = parse_manifest(AppLanguage::Kotlin, &dir).unwrap();
    assert!(!manifest.name.is_empty());
}

#[test]
fn parse_swift_package_returns_manifest() {
    let dir = tmp("swift");
    std::fs::write(dir.join("Package.swift"), "// swift package\n").unwrap();

    let manifest = parse_manifest(AppLanguage::Swift, &dir).unwrap();
    assert!(!manifest.name.is_empty());
}

#[test]
fn parse_react_native_package_json() {
    let dir = tmp("rn");
    std::fs::write(
        dir.join("package.json"),
        r#"{"name":"myapp","dependencies":{"react-native":"0.72.0"}}"#,
    )
    .unwrap();

    let manifest = parse_manifest(AppLanguage::ReactNative, &dir).unwrap();
    assert_eq!(manifest.name, "myapp");
}

#[test]
fn parse_multi_detects_flutter() {
    let dir = tmp("multi-flutter");
    std::fs::write(dir.join("pubspec.yaml"), "name: multiapp\n").unwrap();

    let manifest = parse_manifest(AppLanguage::Multi, &dir).unwrap();
    assert_eq!(manifest.name, "multiapp");
}
