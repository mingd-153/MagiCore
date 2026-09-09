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

/// Hermetic guard: true when the scanner tool IS installed — tests that
/// assert "unavailable without tool" must skip in that environment.
/// Guard hermetic: true khi scanner ĐÃ cài — test assert "unavailable khi
/// thiếu tool" phải bỏ qua trong môi trường đó.
fn tool_installed(tool: &str) -> bool {
    std::process::Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn audit_rust_without_cargo_audit_is_unavailable_not_clean() {
    if tool_installed("cargo-audit") {
        return;
    }
    let dir = tmp("rust-no-tool");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "// empty lib\n").unwrap();

    // Missing scanner must be ToolMissing — never a fake clean report.
    // Thiếu scanner phải ToolMissing — không bao giờ báo sạch giả.
    let report = audit_rust(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(
        report.scanner_status,
        ScannerStatus::ToolMissing { .. }
    ));
}

#[tokio::test]
async fn audit_python_without_pip_audit_is_unavailable_not_clean() {
    if tool_installed("pip-audit") {
        return;
    }
    let dir = tmp("py-no-tool");
    std::fs::write(
        dir.join("pyproject.toml"),
        "[project]\nname = \"test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    // Missing scanner must be ToolMissing — never a fake clean report.
    // Thiếu scanner phải ToolMissing — không bao giờ báo sạch giả.
    let report = audit_python(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(
        report.scanner_status,
        ScannerStatus::ToolMissing { .. }
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
