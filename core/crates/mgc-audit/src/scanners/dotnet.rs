//! `scanners/dotnet.rs` — .NET dependency CVE scanner via OSV.dev
//! (`NuGet` ecosystem — verified live 2026-09-10: `Newtonsoft.Json@
//! 12.0.2` → GHSA-5crp-9r3c-p9vr; `13.0.1` clean). P2 2026-09-10
//! matrix row "Lib .NET dotnet list package --vulnerable" — this lane
//! does not need the dotnet CLI: the NuGet lockfile is machine-written
//! JSON with a stable shape.
//!
//! Source read: `packages.lock.json` (written by
//! `dotnet restore --locked-mode` / RestoreLockedMode) — the `dependencies`
//! maps per-target-framework; direct + transitive both pinned. Pins are
//! deduplicated across TFMs (one package audited once).
//!
//! Scanner CVE dependency .NET qua OSV.dev (ecosystem `NuGet`). Đọc
//! packages.lock.json (JSON máy ghi, shape ổn định) — map dependencies
//! theo target-framework, ghim direct + transitive, khử trùng giữa TFM.

use crate::scanners::osv::{OsvPin, audit_osv_pins};
use mgc_types::adapter::{AuditReport, ScannerStatus};
use mgc_types::{MgError, MgResult};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

/// `packages.lock.json`: top-level `dependencies` object keyed by TFM,
/// each holding a `dependencies` map of name → {version, type}.
/// packages.lock.json: object `dependencies` cấp gốc theo TFM, mỗi TFM
/// giữ map dependencies name → {version, type}.
#[derive(Debug, Deserialize)]
struct NuGetLock {
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, TfmDependencies>,
}

#[derive(Debug, Deserialize)]
struct TfmDependencies {
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, NuGetEntry>,
}

#[derive(Debug, Deserialize)]
struct NuGetEntry {
    #[serde(default)]
    #[allow(dead_code)]
    r#type: String,
    #[serde(default)]
    version: Option<String>,
}

/// Read packages.lock.json into OSV pins (`NuGet` ecosystem). Entries
/// with a missing/empty version are skipped honestly. Pins are
/// deduplicated across target frameworks.
/// Đọc packages.lock.json thành ghim OSV (ecosystem `NuGet`). Entry
/// thiếu/rỗng version thì skip trung thực. Ghim khử trùng giữa các TFM.
pub fn read_packages_lock(raw: &str) -> MgResult<(Vec<OsvPin>, Vec<String>)> {
    let lock: NuGetLock = serde_json::from_str(raw)
        .map_err(|e| MgError::Other(format!("invalid packages.lock.json: {e}")))?;
    let mut pins: Vec<OsvPin> = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = BTreeSet::new();
    for tfm_deps in lock.dependencies.values() {
        for (name, entry) in &tfm_deps.dependencies {
            let Some(version) = entry.version.as_deref().filter(|v| !v.is_empty()) else {
                skipped.push(format!("{name}: no version pin in packages.lock.json"));
                continue;
            };
            if !seen.insert(format!("{name}@{version}")) {
                continue; // Same pin under another TFM — audited once.
                // Ghim trùng ở TFM khác — audit một lần.
            }
            pins.push(OsvPin {
                name: name.clone(),
                version: version.to_string(),
                ecosystem: "NuGet",
            });
        }
    }
    Ok((pins, skipped))
}

/// .NET audit: packages.lock.json → OSV `NuGet` queries. Without the
/// lockfile the honest unsupported state stays (with real remediation).
/// Audit .NET: packages.lock.json → truy vấn OSV `NuGet`. Thiếu
/// lockfile giữ trạng thái unsupported trung thực (kèm hướng dẫn thật).
pub async fn audit_dotnet(project_root: &Path) -> MgResult<AuditReport> {
    let lock_path = project_root.join("packages.lock.json");
    if !lock_path.is_file() {
        let has_csproj = std::fs::read_dir(project_root)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|e| e.file_name().to_string_lossy().ends_with(".csproj"))
            })
            .unwrap_or(false);
        return Ok(AuditReport::unsupported_ecosystem(if has_csproj {
            "lib/dotnet (csproj without packages.lock.json — run `dotnet restore --locked-mode` first)"
        } else {
            "lib/dotnet (no .csproj detected)"
        }));
    }
    let raw = std::fs::read_to_string(&lock_path)
        .map_err(|e| MgError::Other(format!("read packages.lock.json: {e}")))?;
    let (pins, skipped) = read_packages_lock(&raw)?;
    if pins.is_empty() && skipped.is_empty() {
        return Ok(AuditReport::clean(0));
    }
    if pins.is_empty() {
        return Ok(AuditReport {
            packages_audited: 0,
            vulnerability_count: 0,
            vulnerabilities: vec![],
            scanner_status: ScannerStatus::Partial {
                scanned: 0,
                skipped: skipped.len(),
                reasons: skipped,
            },
        });
    }
    let mut report = audit_osv_pins(&pins).await?;
    if !skipped.is_empty() {
        report.scanner_status = ScannerStatus::Partial {
            scanned: pins.len(),
            skipped: skipped.len(),
            reasons: skipped,
        };
    }
    Ok(report)
}
