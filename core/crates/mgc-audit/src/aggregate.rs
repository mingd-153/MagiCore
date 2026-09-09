//! `aggregate.rs` — Multi-scanner aggregation contract (Tech Lead
//! 2026-09-09): a multi-language project (e.g. React Native = Node +
//! Gradle + CocoaPods) must aggregate EVERY detected manifest into ONE
//! report — never return the first scanner's result and stop.
//!
//! Rules (verbatim from the contract):
//! - One scanner Failed → the whole report can never be Available.
//! - One ecosystem unsupported → Partial or UnsupportedEcosystem.
//! - Available only when EVERY recognized manifest was scanned.
//! - packages_audited = packages actually scanned.
//! - skipped records manifest/package names + reasons.
//!
//! Tóm tắt hợp đồng: một scanner Failed thì tổng thể không thể
//! Available; một ecosystem chưa hỗ trợ thì ra Partial hoặc
//! UnsupportedEcosystem; chỉ khi MỌI manifest nhận diện đều được scan
//! mới đạt Available; packages_audited là số package thực sự scan;
//! skipped phải ghi rõ tên manifest/package cùng lý do.

use mgc_types::adapter::{AuditReport, ScannerStatus};

/// Aggregate scan results from ALL detected manifests into one report.
/// The result state follows the strictest-wins contract:
/// - Any `Failed` → `Failed` (the audit could not complete).
/// - Any `ToolMissing` → `ToolMissing` (environment cannot complete).
/// - All `UnsupportedEcosystem` (no scanner ran) → `UnsupportedEcosystem`.
/// - Mix of Available + Unsupported/Partial → `Partial` with reasons.
/// - All Available → `Available` (only then can the audit be "clean").
///
/// Tổng hợp kết quả scan của mọi manifest nhận diện. Trạng thái theo
/// nguyên tắc chặt nhất thắng; chỉ khi tất cả Available thì tổng thể
/// mới có thể coi là audit sạch.
pub fn aggregate_reports(reports: Vec<(String, AuditReport)>) -> AuditReport {
    let mut packages_audited = 0usize;
    let mut vulnerabilities = Vec::new();
    let mut scanned: usize = 0;
    let mut skipped: usize = 0;
    let mut reasons: Vec<String> = Vec::new();

    // Track the dominant state across steps.
    // Theo dõi trạng thái chặt nhất giữa các bước.
    let mut any_failed: Option<(String, String)> = None;
    let mut any_tool_missing: Option<(String, String)> = None;
    let mut all_unsupported = true;
    let mut any_available = false;

    for (label, report) in &reports {
        match &report.scanner_status {
            ScannerStatus::Available => {
                all_unsupported = false;
                any_available = true;
                packages_audited += report.packages_audited;
                vulnerabilities.extend(report.vulnerabilities.iter().cloned());
                scanned += 1;
            }
            ScannerStatus::Partial {
                skipped: skipped_n,
                reasons: r,
                ..
            } => {
                all_unsupported = false;
                skipped += skipped_n;
                reasons.extend(r.iter().cloned());
            }
            ScannerStatus::ToolMissing { tool, remediation } => {
                all_unsupported = false;
                if any_tool_missing.is_none() {
                    any_tool_missing = Some((format!("{label}: {tool}"), remediation.clone()));
                }
            }
            ScannerStatus::UnsupportedEcosystem { ecosystem } => {
                reasons.push(format!("{label}: no scanner implemented for '{ecosystem}'"));
            }
            ScannerStatus::Failed { scanner, reason } => {
                all_unsupported = false;
                if any_failed.is_none() {
                    any_failed = Some((scanner.clone(), reason.clone()));
                }
            }
        }
    }

    let scanner_status = if let Some((scanner, reason)) = any_failed {
        ScannerStatus::Failed { scanner, reason }
    } else if let Some((tool, remediation)) = any_tool_missing {
        ScannerStatus::ToolMissing { tool, remediation }
    } else if all_unsupported && !reports.is_empty() {
        // Nothing ran at all — no scanner for ANY recognized manifest.
        // Không scanner nào chạy — không có scanner cho bất kỳ manifest nào.
        let ecosystem = reports
            .first()
            .and_then(|(_, r)| match &r.scanner_status {
                ScannerStatus::UnsupportedEcosystem { ecosystem } => Some(ecosystem.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "multi".to_string());
        ScannerStatus::UnsupportedEcosystem { ecosystem }
    } else if any_available && reasons.is_empty() {
        ScannerStatus::Available
    } else if any_available {
        ScannerStatus::Partial {
            scanned,
            skipped,
            reasons,
        }
    } else if reports.is_empty() {
        // No manifests detected at all — nothing to audit, honest empty.
        // Không nhận diện manifest nào — không có gì để audit.
        ScannerStatus::Available
    } else {
        ScannerStatus::Partial {
            scanned,
            skipped,
            reasons,
        }
    };

    AuditReport {
        packages_audited,
        vulnerability_count: vulnerabilities.len(),
        vulnerabilities,
        scanner_status,
    }
}
