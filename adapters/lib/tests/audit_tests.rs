#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Audit module integration tests.

use mgc_lib_adapter::audit::scanner::{audit_python, audit_rust};
use mgc_types::adapter::ScannerStatus;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-audit-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

#[tokio::test]
async fn audit_rust_requires_a_lockfile_instead_of_reporting_clean() {
    let dir = tmp("rust-no-tool");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "// empty lib\n").unwrap();

    // No dependency graph means no audit result; absence is not clean.
    // Không có graph dependency thì không có kết quả audit; thiếu không có nghĩa là sạch.
    assert!(audit_rust(&dir).await.is_err());
}

#[tokio::test]
async fn audit_rust_empty_lock_is_a_native_clean_result_without_external_tools() {
    let dir = tmp("rust-empty-lock");
    std::fs::write(dir.join("Cargo.lock"), "version = 4\n").unwrap();
    let report = audit_rust(&dir).await.unwrap();
    assert_eq!(report.packages_audited, 0);
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(report.scanner_status, ScannerStatus::Available));
}

#[tokio::test]
async fn audit_python_without_a_resolved_lock_is_unverified_not_clean() {
    let dir = tmp("py-no-tool");
    std::fs::write(
        dir.join("pyproject.toml"),
        "[project]\nname = \"test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    // A project manifest without a resolved graph cannot prove a clean audit.
    // Manifest không có graph đã resolve không thể chứng minh audit sạch.
    let report = audit_python(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(
        report.scanner_status,
        ScannerStatus::Failed { .. }
    ));
}

// NOTE: audit với cargo-audit cài thật được cover bởi parser fixture test
// (fixture captured từ `cargo audit --json` thật — RUSTSEC-2023-0071);
// chạy tool thật vẫn là manual QA vì phụ thuộc advisory DB sống.
// (Real-tool audits stay manual QA: they depend on live advisory DBs; the
// parser itself is regression-tested with a captured real fixture.)

#[test]
fn audit_rust_with_invalid_project_dir() {
    let dir = tmp("rust-invalid");
    // No Cargo.toml - audit should handle gracefully
    let result = tokio_test::block_on(audit_rust(&dir));
    // Should either return empty report or error - both acceptable
    let _ = result;
}

#[test]
fn audit_python_with_invalid_project_dir() {
    let dir = tmp("py-invalid");
    // No pyproject.toml - audit should handle gracefully
    let result = tokio_test::block_on(audit_python(&dir));
    let _ = result;
}
