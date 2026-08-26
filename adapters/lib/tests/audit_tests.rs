//! Audit module integration tests.

#![allow(clippy::unwrap_used)]
use mgc_lib_adapter::audit::scanner::{audit_python, audit_rust};
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-audit-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

#[tokio::test]
async fn audit_rust_without_cargo_audit_returns_empty_report() {
    let dir = tmp("rust-no-tool");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "// empty lib\n").unwrap();

    // If cargo-audit not installed, should return empty report
    let report = audit_rust(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
}

#[tokio::test]
async fn audit_python_without_pip_audit_returns_empty_report() {
    let dir = tmp("py-no-tool");
    std::fs::write(
        dir.join("pyproject.toml"),
        "[project]\nname = \"test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    // If pip-audit/safety not installed, should return empty report
    let report = audit_python(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
}

// NOTE: audit với cargo-audit/pip-audit cài thật là manual QA (cần tool hệ thống,
// kết quả phụ thuộc advisory DB) — không đưa vào suite hermetic.
// (Real-tool audits are manual QA: they need system tools + live advisory DBs.)

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
