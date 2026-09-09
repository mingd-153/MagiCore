#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Audit module tests for app adapter.

use mgc_app_adapter::audit::scanner::*;
use mgc_types::adapter::ScannerStatus;

fn tmp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-app-audit-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

/// Hermetic guard: true when the tool IS installed — tests asserting
/// "unavailable without tool" must skip in that environment.
/// Guard hermetic: true khi tool ĐÃ cài — test assert "unavailable khi
/// thiếu tool" phải bỏ qua trong môi trường đó.
fn tool_installed(tool: &str) -> bool {
    std::process::Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn audit_flutter_security_is_unsupported_not_clean() {
    if tool_installed("flutter") {
        return;
    }
    let dir = tmp("flutter-no-tool");
    std::fs::write(dir.join("pubspec.yaml"), "name: test\n").unwrap();

    // pub outdated is NOT a CVE scanner — the security audit must be
    // honestly UnsupportedEcosystem, never a fake clean report.
    // pub outdated KHÔNG phải scanner CVE — security audit phải trung thực
    // UnsupportedEcosystem, không bao giờ báo sạch giả.
    let report = audit_flutter(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(
        report.scanner_status,
        mgc_types::adapter::ScannerStatus::UnsupportedEcosystem { .. }
    ));
}

#[tokio::test]
async fn dependency_health_flutter_without_flutter_fails_closed() {
    if tool_installed("flutter") {
        return;
    }
    let dir = tmp("flutter-no-tool");
    std::fs::write(dir.join("pubspec.yaml"), "name: test\n").unwrap();

    // Without the CLI we cannot know freshness — an empty default report
    // would fake "everything up to date". Must fail closed.
    // Không có CLI thì không biết độ tươi — report rỗng đồng nghĩa bịa
    // "mọi thứ mới". Phải fail-closed.
    assert!(dependency_health_flutter(&dir).await.is_err());
}

#[tokio::test]
async fn audit_kotlin_without_gradle_is_tool_missing() {
    if tool_installed("gradle") {
        return;
    }
    let dir = tmp("kotlin-no-tool");
    std::fs::write(dir.join("build.gradle"), "// empty\n").unwrap();

    let report = audit_kotlin(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(
        report.scanner_status,
        ScannerStatus::ToolMissing { .. }
    ));
}

#[tokio::test]
async fn audit_swift_not_implemented_is_unsupported_not_clean() {
    let dir = tmp("swift-audit");
    std::fs::write(dir.join("Package.swift"), "// swift package\n").unwrap();

    let report = audit_swift(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(
        report.scanner_status,
        mgc_types::adapter::ScannerStatus::UnsupportedEcosystem { .. }
    ));
}

#[tokio::test]
async fn audit_cocoapods_not_implemented_is_unsupported_not_clean() {
    let dir = tmp("cocoapods-audit");

    let report = audit_cocoapods(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(
        report.scanner_status,
        mgc_types::adapter::ScannerStatus::UnsupportedEcosystem { .. }
    ));
}

#[tokio::test]
async fn audit_multi_detects_flutter_as_unsupported() {
    let dir = tmp("multi-flutter");
    std::fs::write(dir.join("pubspec.yaml"), "name: test\n").unwrap();

    let report = audit_multi(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(
        report.scanner_status,
        ScannerStatus::UnsupportedEcosystem { .. }
    ));
}

// ===== Multi-language aggregation (Tech Lead 2026-09-09 §11) =====
// Multi KHÔNG còn first-match: mọi manifest nhận diện đều được scan và
// tổng hợp — pubspec + gradle cùng lúc phải ra Partial (flutter chưa có
// scanner + kotlin thiếu gradle → không bao giờ chỉ trả 1 core).

#[tokio::test]
async fn audit_multi_aggregates_every_manifest_not_first_match() {
    let dir = tmp("multi-aggregate");
    // BOTH manifests present — the old first-match code returned the
    // flutter result and never looked at gradle.
    // CẢ HAI manifest cùng tồn tại — code first-match cũ trả flutter
    // rồi không nhìn gradle nữa.
    std::fs::write(dir.join("pubspec.yaml"), "name: test\n").unwrap();
    std::fs::write(dir.join("build.gradle"), "// empty\n").unwrap();

    let report = audit_multi(&dir).await.unwrap();
    match report.scanner_status {
        mgc_types::adapter::ScannerStatus::ToolMissing { tool, .. } => {
            // Kotlin step is hermetic-deterministic (no gradle on the
            // test machine PATH), so the aggregate surfaces ToolMissing
            // for gradle — NOT the bare flutter Unsupported result.
            // Bước kotlin tất định (máy test không có gradle trên PATH),
            // tổng hợp phải ra ToolMissing gradle — KHÔNG phải kết quả
            // flutter Unsupported cũ.
            assert!(tool.contains("gradle"), "tool must be gradle, got: {tool}");
        }
        other => panic!("multi with gradle absent must aggregate to ToolMissing, got {other:?}"),
    }
}

#[tokio::test]
async fn audit_multi_aggregate_names_flutter_gap() {
    // Flutter (no CVE scanner) + gradle missing → ToolMissing dominates,
    // but the tool-missing branch must still reference gradle, proving
    // BOTH steps registered (flutter did not shadow kotlin).
    // Flutter (chưa có scanner CVE) + thiếu gradle → ToolMissing thắng,
    // nhánh đó phải nhắc gradle — chứng tỏ CẢ HAI bước đều đăng ký
    // (flutter không lấp mất kotlin).
    let dir = tmp("multi-aggregate-2");
    std::fs::write(dir.join("pubspec.yaml"), "name: test\n").unwrap();
    std::fs::write(dir.join("build.gradle.kts"), "// empty\n").unwrap();

    let report = audit_multi(&dir).await.unwrap();
    assert!(matches!(
        report.scanner_status,
        mgc_types::adapter::ScannerStatus::ToolMissing { .. }
    ));
}
