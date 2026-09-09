//! `output/sarif.rs` — SARIF 2.1.0 output for GitHub Security ingest
//! (Tech Lead 2026-09-09). Only findings from an AVAILABLE scan are
//! emitted — an unverified report never invents SARIF results.
//! Output SARIF 2.1.0 cho GitHub Security ingest — chỉ finding từ scan
//! Available mới được emit; report chưa xác thực không bịa kết quả.

use mgc_types::adapter::{AuditReport, ScannerStatus, VulnerabilitySeverity};
use serde::Serialize;

pub const SARIF_SCHEMA: &str = "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json";
pub const SARIF_VERSION: &str = "2.1.0";

#[derive(Serialize)]
pub struct SarifLog {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub version: String,
    pub runs: Vec<SarifRun>,
}

#[derive(Serialize)]
pub struct SarifRun {
    pub tool: SarifTool,
    pub results: Vec<SarifResult>,
}

#[derive(Serialize)]
pub struct SarifTool {
    pub driver: SarifDriver,
}

#[derive(Serialize)]
pub struct SarifDriver {
    pub name: String,
    pub version: String,
    pub information_uri: String,
}

#[derive(Serialize)]
pub struct SarifResult {
    /// SARIF level: error for Critical/High, warning for Medium/Low, note
    /// for Info — maps cleanly onto GitHub Security categories.
    /// Cấp SARIF: error cho Critical/High, warning Medium/Low, note Info.
    pub level: &'static str,
    pub message: SarifMessage,
    pub rule_id: String,
    /// Logical location = the affected package (ecosystem:name@version).
    /// Vị trí logic = package dính lỗi (ecosystem:name@version).
    pub locations: Vec<SarifLocation>,
}

#[derive(Serialize)]
pub struct SarifMessage {
    pub text: String,
}

#[derive(Serialize)]
pub struct SarifLocation {
    pub logical_locations: Vec<SarifLogicalLocation>,
}

#[derive(Serialize)]
pub struct SarifLogicalLocation {
    pub name: String,
}

/// Build a SARIF log from an audit report. Non-Available states yield a
/// run with zero results PLUS a note in the driver name — honest, never
/// a fabricated empty-clean.
/// Dựng SARIF log từ report. Trạng thái non-Available cho run 0 result
/// kèm ghi chú trong tên driver — trung thực, không bịa sạch rỗng.
pub fn to_sarif(report: &AuditReport) -> SarifLog {
    let verified = matches!(report.scanner_status, ScannerStatus::Available);
    let driver_name = if verified {
        "mgc-audit".to_string()
    } else {
        "mgc-audit (UNVERIFIED — scanner did not complete)".to_string()
    };

    let results = report
        .vulnerabilities
        .iter()
        .map(|v| SarifResult {
            level: match v.severity_level {
                VulnerabilitySeverity::Critical | VulnerabilitySeverity::High => "error",
                VulnerabilitySeverity::Medium | VulnerabilitySeverity::Low => "warning",
                VulnerabilitySeverity::Info => "note",
            },
            message: SarifMessage {
                text: format!(
                    "{} in {} (scanner: {}, evidence: {})",
                    v.title,
                    v.package,
                    v.scanner.as_deref().unwrap_or("unknown"),
                    v.evidence_at.as_deref().unwrap_or("not-stamped"),
                ),
            },
            rule_id: v.cve.clone(),
            locations: vec![SarifLocation {
                logical_locations: vec![SarifLogicalLocation {
                    name: format!(
                        "{}:{}@{}",
                        v.ecosystem.as_deref().unwrap_or("unknown"),
                        v.package.name().as_str(),
                        v.package.version(),
                    ),
                }],
            }],
        })
        .collect();

    SarifLog {
        schema: SARIF_SCHEMA.to_string(),
        version: SARIF_VERSION.to_string(),
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: driver_name,
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    information_uri: "https://github.com/mingd-153/MagiCore".to_string(),
                },
            },
            results,
        }],
    }
}
