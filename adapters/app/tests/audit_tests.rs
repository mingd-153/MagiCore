#![cfg_attr(test, allow(clippy::unwrap_used))]
//! Audit module tests for app adapter.

use mgc_app_adapter::audit::scanner::*;

fn tmp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-app-audit-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

#[tokio::test]
async fn audit_flutter_without_flutter_returns_empty() {
    let dir = tmp("flutter-no-tool");
    std::fs::write(dir.join("pubspec.yaml"), "name: test\n").unwrap();

    let report = audit_flutter(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
}

#[tokio::test]
async fn audit_kotlin_without_gradle_returns_empty() {
    let dir = tmp("kotlin-no-tool");
    std::fs::write(dir.join("build.gradle"), "// empty\n").unwrap();

    let report = audit_kotlin(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
}

#[tokio::test]
async fn audit_swift_not_implemented_returns_empty() {
    let dir = tmp("swift-audit");
    std::fs::write(dir.join("Package.swift"), "// swift package\n").unwrap();

    let report = audit_swift(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
}

#[tokio::test]
async fn audit_cocoapods_not_implemented_returns_empty() {
    let dir = tmp("cocoapods-audit");

    let report = audit_cocoapods(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
}

#[tokio::test]
async fn audit_multi_detects_flutter() {
    let dir = tmp("multi-flutter");
    std::fs::write(dir.join("pubspec.yaml"), "name: test\n").unwrap();

    let report = audit_multi(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
}
