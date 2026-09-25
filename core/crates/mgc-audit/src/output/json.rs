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
    /// Skip reasons for partial/unverified states (additive field —
    /// schema stays v1: old readers ignore unknown fields, and empty
    /// states serialize as `[]`). CI gates on UNVERIFIED need the
    /// *why*, not just the slug.
    /// (Lý do skip cho trạng thái chưa hoàn tất — field cộng thêm,
    /// schema giữ v1.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_reasons: Vec<String>,
}

impl<'a> JsonAuditEnvelope<'a> {
    pub fn from_report(report: &'a AuditReport) -> Self {
        Self {
            schema_version: JSON_SCHEMA_VERSION,
            scanner_status: status_slug(report),
            packages_audited: report.packages_audited,
            vulnerability_count: report.vulnerability_count,
            vulnerabilities: &report.vulnerabilities,
            skipped_reasons: skipped_reasons(report),
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

/// Skip/skip-reason lines for unverified states (empty when Available).
/// (Dòng lý do skip cho trạng thái chưa hoàn tất.)
fn skipped_reasons(report: &AuditReport) -> Vec<String> {
    use mgc_types::adapter::ScannerStatus;
    match &report.scanner_status {
        ScannerStatus::Partial { reasons, .. } => reasons.clone(),
        ScannerStatus::ToolMissing { tool, remediation } => {
            vec![format!("{tool}: {remediation}")]
        }
        ScannerStatus::UnsupportedEcosystem { ecosystem } => vec![ecosystem.clone()],
        ScannerStatus::Failed { scanner, reason } => vec![format!("{scanner}: {reason}")],
        ScannerStatus::Available => Vec::new(),
    }
}
