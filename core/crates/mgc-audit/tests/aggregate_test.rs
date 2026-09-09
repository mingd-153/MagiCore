//! Aggregation contract tests — the multi-scanner rules from the Tech
//! Lead review (2026-09-09): strictest state wins, every manifest
//! scanned, skipped recorded with reasons.
//! Test hợp đồng tổng hợp: trạng thái chặt nhất thắng, mọi manifest
//! được scan, skipped ghi lại kèm lý do.
#![allow(clippy::unwrap_used)]

use mgc_audit::aggregate_reports;
use mgc_types::adapter::{AuditReport, ScannerStatus};

#[test]
fn all_available_aggregates_to_available() {
    let a = AuditReport::clean(3);
    let b = AuditReport::clean(4);
    let out = aggregate_reports(vec![("node:osv".into(), a), ("rust:cargo-audit".into(), b)]);
    assert!(matches!(out.scanner_status, ScannerStatus::Available));
    assert_eq!(out.packages_audited, 7);
    assert_eq!(out.vulnerability_count, 0);
}

#[test]
fn one_failed_dominates_everything() {
    // A Failed scanner poisons the aggregate — even with other green
    // scans, the whole audit is NOT complete.
    // Một scanner Failed làm nhiễm tổng thể — dù các scan khác xanh,
    // toàn bộ audit vẫn KHÔNG hoàn tất.
    let a = AuditReport::clean(3);
    let b = AuditReport::scanner_failed("pip-audit", "schema drift");
    let out = aggregate_reports(vec![("node:osv".into(), a), ("py:pip-audit".into(), b)]);
    assert!(matches!(out.scanner_status, ScannerStatus::Failed { .. }));
    assert!(!out.is_clean());
}

#[test]
fn tool_missing_beats_available_but_not_failed() {
    let ok = AuditReport::clean(2);
    let missing = AuditReport::tool_missing("gradle", "install gradle");
    let out = aggregate_reports(vec![
        ("node:osv".into(), ok),
        ("kotlin:owasp".into(), missing),
    ]);
    assert!(matches!(
        out.scanner_status,
        ScannerStatus::ToolMissing { .. }
    ));

    let failed = AuditReport::scanner_failed("x", "y");
    let missing2 = AuditReport::tool_missing("gradle", "install gradle");
    let out2 = aggregate_reports(vec![("a:x".into(), failed), ("b:y".into(), missing2)]);
    assert!(matches!(out2.scanner_status, ScannerStatus::Failed { .. }));
}

#[test]
fn available_plus_unsupported_is_partial_with_reasons() {
    // Mixed coverage: the node tree was scanned, flutter was not — the
    // aggregate must be Partial naming the gap, never silently Available.
    // Phủ trộn lẫn: node đã scan, flutter chưa — tổng hợp phải Partial
    // nêu rõ khoảng hở, không âm thầm Available.
    let ok = AuditReport::clean(5);
    let unsupported = AuditReport::unsupported_ecosystem("flutter");
    let out = aggregate_reports(vec![
        ("node:osv".into(), ok),
        ("flutter:none".into(), unsupported),
    ]);
    match out.scanner_status {
        ScannerStatus::Partial {
            scanned,
            skipped,
            reasons,
        } => {
            assert_eq!(scanned, 1);
            assert_eq!(skipped, 0);
            assert!(
                reasons.iter().any(|r| r.contains("flutter")),
                "must name the unsupported gap: {reasons:?}"
            );
        }
        other => panic!("expected Partial, got {other:?}"),
    }
}

#[test]
fn all_unsupported_is_unsupported_ecosystem() {
    let a = AuditReport::unsupported_ecosystem("flutter");
    let out = aggregate_reports(vec![("flutter:none".into(), a)]);
    assert!(matches!(
        out.scanner_status,
        ScannerStatus::UnsupportedEcosystem { .. }
    ));
    assert!(!out.is_clean());
}

#[test]
fn findings_merge_across_ecosystems() {
    let mut vuln_report = AuditReport::clean(1);
    vuln_report.vulnerability_count = 1;
    vuln_report
        .vulnerabilities
        .push(mgc_types::adapter::Vulnerability {
            package: mgc_types::PackageId::parse("requests@2.19.0").unwrap(),
            title: "PYSEC test".to_string(),
            severity: "high".to_string(),
            cve: "PYSEC-2018-28".to_string(),
            severity_level: mgc_types::adapter::VulnerabilitySeverity::High,
            patched_versions: Some("2.20.0".to_string()),
            url: None,
            scanner: None,
            ecosystem: None,
            evidence_at: None,
        });
    let clean = AuditReport::clean(2);
    let out = aggregate_reports(vec![
        ("py:pip-audit".into(), vuln_report),
        ("node:osv".into(), clean),
    ]);
    assert_eq!(out.vulnerability_count, 1);
    assert_eq!(out.packages_audited, 3);
}

#[test]
fn empty_plan_is_honest_available_zero_packages() {
    // No manifests detected → nothing to audit → Available with zero
    // packages (not Unsupported, not clean-with-packages).
    // Không nhận diện manifest → không có gì audit → Available với 0
    // package (không phải Unsupported, không phải sạch-có-package).
    let out = aggregate_reports(vec![]);
    assert!(matches!(out.scanner_status, ScannerStatus::Available));
    assert_eq!(out.packages_audited, 0);
    assert_eq!(out.vulnerability_count, 0);
}
