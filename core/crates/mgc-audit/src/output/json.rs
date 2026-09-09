//! `output/json.rs` — Versioned JSON envelope for audit reports.
//! Schema version travels WITH the payload; ingest code branches on
//! `schema_version`, never on guesswork.
//! Envelope JSON có version cho report audit — code ingest phân nhánh
//! theo `schema_version`, không đoán mò.

use mgc_types::adapter::AuditReport;
use serde::Serialize;

/// Current envelope schema version — bump on breaking shape changes.
/// Version envelope hiện tại — tăng khi đổi shape gây vỡ.
pub const JSON_SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
pub struct JsonAuditEnvelope<'a> {
    pub schema_version: u32,
    /// Scanner state: available | partial | tool_missing |
    /// unsupported_ecosystem | failed (serde tag of ScannerStatus).
    /// Trạng thái scanner theo tag serde của ScannerStatus.
    pub scanner_status: &'a str,
    pub packages_audited: usize,
    pub vulnerability_count: usize,
    pub vulnerabilities: &'a [mgc_types::adapter::Vulnerability],
}

impl<'a> JsonAuditEnvelope<'a> {
    pub fn from_report(report: &'a AuditReport) -> Self {
        Self {
            schema_version: JSON_SCHEMA_VERSION,
            scanner_status: status_slug(report),
            packages_audited: report.packages_audited,
            vulnerability_count: report.vulnerability_count,
            vulnerabilities: &report.vulnerabilities,
        }
    }

    /// Serialize the full envelope.
    /// Serialize envelope đầy đủ.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

/// Short machine slug per ScannerStatus (stable across schema versions).
/// Slug ngắn cho từng ScannerStatus (ổn định qua các version schema).
fn status_slug(report: &AuditReport) -> &'static str {
    use mgc_types::adapter::ScannerStatus;
    match report.scanner_status {
        ScannerStatus::Available => "available",
        ScannerStatus::Partial { .. } => "partial",
        ScannerStatus::ToolMissing { .. } => "tool_missing",
        ScannerStatus::UnsupportedEcosystem { .. } => "unsupported_ecosystem",
        ScannerStatus::Failed { .. } => "failed",
    }
}
