//! Native Rust advisory scanner over Cargo.lock — no cargo-audit process.
//! Quét advisory Rust native từ Cargo.lock — không spawn cargo-audit.

use crate::scanners::osv::{OsvPin, audit_osv_pins};
use mgc_types::adapter::{AuditReport, ScannerStatus};
use mgc_types::{MgError, MgResult};
use std::collections::BTreeSet;
use std::path::Path;

const CRATES_IO_GIT_INDEX: &str = "registry+https://github.com/rust-lang/crates.io-index";
const CRATES_IO_SPARSE_INDEX: &str = "sparse+https://index.crates.io/";
const CRATES_IO_REGISTRY_INDEX: &str = "registry+https://index.crates.io/";

/// Read locked crates.io packages; report other explicit sources as skipped.
/// Đọc package crates.io đã khóa; ghi nhận source tường minh khác là skipped.
pub fn read_cargo_lock_pins(raw: &str) -> MgResult<(Vec<OsvPin>, Vec<String>)> {
    let document: toml::Value = toml::from_str(raw)
        .map_err(|error| MgError::Other(format!("invalid Cargo.lock TOML: {error}")))?;
    let packages = match document.get("package") {
        Some(value) => value.as_array().ok_or_else(|| {
            MgError::Other("Cargo.lock package field is not an array".to_string())
        })?,
        None => return Ok((Vec::new(), Vec::new())),
    };

    let mut pins = BTreeSet::new();
    let mut skipped = Vec::new();
    for package in packages {
        let name = package
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| MgError::Other("Cargo.lock package has no valid name".to_string()))?;
        let version = package
            .get("version")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| {
                MgError::Other(format!("Cargo.lock package '{name}' has no valid version"))
            })?;
        let source = package.get("source").and_then(toml::Value::as_str);

        match source {
            Some(CRATES_IO_GIT_INDEX | CRATES_IO_SPARSE_INDEX | CRATES_IO_REGISTRY_INDEX) => {
                pins.insert((name.to_string(), version.to_string()));
            }
            Some(other) => skipped.push(format!(
                "Cargo.lock package '{name}@{version}' uses unsupported source '{other}'"
            )),
            // Workspace/path packages have no registry source; their own
            // code is not a third-party advisory pin.
            // Package workspace/path không có registry source; mã nguồn
            // dự án không phải pin advisory bên thứ ba.
            None => {}
        }
    }

    let pins = pins
        .into_iter()
        .map(|(name, version)| OsvPin {
            name,
            version,
            ecosystem: "crates.io",
        })
        .collect();
    Ok((pins, skipped))
}

/// Audit locked crates.io dependencies through MGC's OSV client.
/// Dùng OSV client của MGC để audit dependency crates.io đã khóa.
pub async fn audit_rust(project_root: &Path) -> MgResult<AuditReport> {
    let lock_path = project_root.join("Cargo.lock");
    let raw = std::fs::read_to_string(&lock_path).map_err(|error| {
        MgError::Other(format!(
            "cannot read Cargo.lock for native Rust audit at {}: {error}",
            lock_path.display()
        ))
    })?;
    let (pins, skipped) = read_cargo_lock_pins(&raw)?;
    let mut report = audit_osv_pins(&pins).await?;
    if !skipped.is_empty() {
        report.scanner_status = ScannerStatus::Partial {
            scanned: report.packages_audited,
            skipped: skipped.len(),
            reasons: skipped,
        };
    }
    Ok(report)
}
