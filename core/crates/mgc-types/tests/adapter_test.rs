#![cfg(test)]
#![allow(clippy::unwrap_used)]

use mgc_types::adapter::*;
use mgc_types::package::PackageId;

#[test]
fn audit_report_is_clean_when_no_vulns() {
    let report = AuditReport::clean(0);
    assert!(report.is_clean());
}

#[test]
fn audit_report_clean_tracks_packages_audited_without_vulns() {
    let report = AuditReport::clean(3);
    assert_eq!(report.packages_audited, 3);
    assert_eq!(report.vulnerability_count, 0);
    assert!(report.is_clean());
}

#[test]
fn audit_report_not_clean_with_vulnerabilities_vec() {
    let report = AuditReport {
        packages_audited: 1,
        vulnerability_count: 1,
        vulnerabilities: vec![Vulnerability {
            package: PackageId::parse("test@1.0.0").unwrap(),
            title: "vuln".to_string(),
            severity: "high".to_string(),
            cve: "CVE-123".to_string(),
            severity_level: VulnerabilitySeverity::High,
            patched_versions: None,
            url: None,
        }],
        scanner_status: ScannerStatus::Available,
    };
    assert!(!report.is_clean());
}

#[test]
fn audit_report_not_clean_with_nonzero_count() {
    let report = AuditReport {
        packages_audited: 3,
        vulnerability_count: 3,
        vulnerabilities: vec![],
        scanner_status: ScannerStatus::Available,
    };
    assert!(!report.is_clean());
}

#[test]
fn resolved_graph_empty_creates_empty() {
    let g = ResolvedGraph::empty();
    assert_eq!(g.len(), 0);
    assert!(g.is_empty());
}

#[test]
fn resolved_graph_default_is_empty() {
    let g = ResolvedGraph::default();
    assert!(g.is_empty());
}

#[test]
fn resolved_graph_with_packages_not_empty() {
    let g = ResolvedGraph {
        packages: vec![ResolvedPackage {
            id: PackageId::parse("foo@1.0.0").unwrap(),
            integrity: "sha1-xxx".to_string(),
            tarball_url: "https://example.com/pkg.tgz".to_string(),
            deps: vec![],
            peer_deps: vec![],
            direct: true,
            dev: false,
        }],
    };
    assert_eq!(g.len(), 1);
    assert!(!g.is_empty());
}

#[test]
fn add_options_default() {
    let opts = AddOptions::default();
    assert!(!opts.dev);
    assert!(!opts.optional);
    assert!(!opts.peer);
    assert!(!opts.exact);
}

#[test]
fn install_summary_default() {
    let s = InstallSummary::default();
    assert!(s.added.is_empty());
    assert_eq!(s.bytes_from_cache, 0);
    assert_eq!(s.duration_ms, 0);
}
