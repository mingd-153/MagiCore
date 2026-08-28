//! `audit/scanner.rs` — Vulnerability scanning for Rust/Python.
//! Uses cargo-audit for Rust, pip-audit/safety for Python.

use mgc_types::adapter::{AuditReport, Vulnerability};
use mgc_types::{MgError, MgResult};
use std::path::Path;

/// Audit Rust dependencies using cargo-audit.
/// Audit dependencies Rust dùng cargo-audit.
///
/// Requires cargo-audit to be installed: `cargo install cargo-audit`
pub async fn audit_rust(project_root: &Path) -> MgResult<AuditReport> {
    // Check if cargo-audit is available
    // Kiểm tra cargo-audit có sẵn không
    if which::which("cargo-audit").is_err() {
        return Ok(AuditReport {
            packages_audited: 0,
            vulnerability_count: 0,
            vulnerabilities: vec![],
        });
    }

    let args = vec!["audit".to_string(), "--json".to_string()];

    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };

    let result = mgc_exec::run::run("cargo", &args, &exec_opts)
        .map_err(|e| MgError::Other(format!("cargo audit failed: {}", e)))?;

    // cargo-audit exits with non-zero if vulnerabilities found
    // cargo-audit thoát với non-zero nếu tìm thấy lỗ hổng
    if result.exit_code != 0 && result.exit_code != 1 {
        return Err(MgError::Other(format!(
            "cargo audit exited with code {}",
            result.exit_code
        )));
    }

    // Parse JSON output (not implemented yet - would need to capture stdout)
    // Parse JSON output (chưa implement - cần capture stdout)
    // For now, return empty report
    // Hiện tại trả về report rỗng
    Ok(AuditReport {
        packages_audited: 0,
        vulnerability_count: 0,
        vulnerabilities: vec![],
    })
}

/// Audit Python dependencies using pip-audit or safety.
/// Audit dependencies Python dùng pip-audit hoặc safety.
///
/// Prefers pip-audit (official PyPA tool) over safety.
/// Ưu tiên pip-audit (công cụ chính thức PyPA) hơn safety.
pub async fn audit_python(project_root: &Path) -> MgResult<AuditReport> {
    // Try pip-audit first, fallback to safety
    // Thử pip-audit trước, fallback sang safety
    let tool = if which::which("pip-audit").is_ok() {
        "pip-audit"
    } else if which::which("safety").is_ok() {
        "safety"
    } else {
        return Ok(AuditReport {
            packages_audited: 0,
            vulnerability_count: 0,
            vulnerabilities: vec![],
        });
    };

    let args = if tool == "pip-audit" {
        vec!["--format".to_string(), "json".to_string()]
    } else {
        vec!["check".to_string(), "--json".to_string()]
    };

    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };

    let result = mgc_exec::run::run(tool, &args, &exec_opts)
        .map_err(|e| MgError::Other(format!("{} failed: {}", tool, e)))?;

    // pip-audit exits with non-zero if vulnerabilities found
    // pip-audit thoát với non-zero nếu tìm thấy lỗ hổng
    if result.exit_code != 0 && result.exit_code != 1 {
        return Err(MgError::Other(format!(
            "{} exited with code {}",
            tool, result.exit_code
        )));
    }

    // Parse JSON output (not implemented yet - would need to capture stdout)
    // Parse JSON output (chưa implement - cần capture stdout)
    Ok(AuditReport {
        packages_audited: 0,
        vulnerability_count: 0,
        vulnerabilities: vec![],
    })
}

/// Parse cargo-audit JSON output.
/// Parse JSON output của cargo-audit.
///
/// Format: https://github.com/rustsec/rustsec/blob/main/cargo-audit/README.md#json-output
fn _parse_cargo_audit_json(_json: &str) -> MgResult<Vec<Vulnerability>> {
    // TODO: implement JSON parsing
    // Format example:
    // {
    //   "vulnerabilities": {
    //     "found": true,
    //     "count": 1,
    //     "list": [...]
    //   }
    // }
    Ok(vec![])
}

/// Parse pip-audit JSON output.
/// Parse JSON output của pip-audit.
///
/// Format: https://pypi.org/project/pip-audit/
fn _parse_pip_audit_json(_json: &str) -> MgResult<Vec<Vulnerability>> {
    // TODO: implement JSON parsing
    // Format example:
    // {
    //   "vulnerabilities": [
    //     {
    //       "name": "package-name",
    //       "version": "1.0.0",
    //       "id": "PYSEC-2024-1234",
    //       ...
    //     }
    //   ]
    // }
    Ok(vec![])
}
