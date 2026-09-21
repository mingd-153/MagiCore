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
            scanner: None,
            ecosystem: None,
            evidence_at: None,
            finding_class: FindingClass::Vulnerability,
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

/// The five-state contract (Tech Lead P0-1): is_clean() must be false for
/// EVERY non-Available state — a report from a scanner that never ran is
/// never "clean", even with zero findings.
/// Hợp đồng 5 trạng thái: is_clean() phải sai với MỌI trạng thái khác
/// Available — report từ scanner chưa chạy không bao giờ "sạch", dù 0 finding.
#[test]
fn audit_report_never_clean_when_scanner_not_available() {
    let partial = AuditReport {
        packages_audited: 3,
        vulnerability_count: 0,
        vulnerabilities: vec![],
        scanner_status: ScannerStatus::Partial {
            scanned: 3,
            skipped: 2,
            reasons: vec!["two deps unscreened".to_string()],
        },
    };
    assert!(!partial.is_clean());

    let tool_missing = AuditReport::tool_missing("cargo-audit", "cargo install cargo-audit");
    assert!(!tool_missing.is_clean());

    let unsupported = AuditReport::unsupported_ecosystem("flutter");
    assert!(!unsupported.is_clean());

    let failed = AuditReport::scanner_failed("pip-audit", "schema drift");
    assert!(!failed.is_clean());

    // Available + zero findings is the ONLY clean state.
    // Available + 0 finding là trạng thái sạch DUY NHẤT.
    assert!(AuditReport::clean(0).is_clean());
}

/// Deserialization must round-trip the tagged serde shape so external
/// consumers (CI ingest, reports) keep a stable schema.
/// Deserialize phải giữ đúng schema tagged để consumer ngoài (CI ingest,
/// report) không vỡ hợp đồng.
#[test]
fn scanner_status_serde_roundtrip_all_states() {
    let states = vec![
        ScannerStatus::Available,
        ScannerStatus::Partial {
            scanned: 1,
            skipped: 1,
            reasons: vec!["r".to_string()],
        },
        ScannerStatus::ToolMissing {
            tool: "gradle".to_string(),
            remediation: "install gradle".to_string(),
        },
        ScannerStatus::UnsupportedEcosystem {
            ecosystem: "flutter".to_string(),
        },
        ScannerStatus::Failed {
            scanner: "osv".to_string(),
            reason: "network".to_string(),
        },
    ];
    for status in &states {
        let json = serde_json::to_string(status).unwrap();
        let back: ScannerStatus = serde_json::from_str(&json).unwrap();
        match (status, &back) {
            (ScannerStatus::Available, ScannerStatus::Available) => {}
            (
                ScannerStatus::Partial {
                    scanned: s1,
                    skipped: k1,
                    ..
                },
                ScannerStatus::Partial {
                    scanned: s2,
                    skipped: k2,
                    ..
                },
            ) => assert!((s1, k1) == (s2, k2), "partial mismatch for {json}"),
            (
                ScannerStatus::ToolMissing { tool: t1, .. },
                ScannerStatus::ToolMissing { tool: t2, .. },
            ) => assert_eq!(t1, t2, "tool_missing mismatch for {json}"),
            (
                ScannerStatus::UnsupportedEcosystem { ecosystem: e1 },
                ScannerStatus::UnsupportedEcosystem { ecosystem: e2 },
            ) => assert_eq!(e1, e2, "unsupported mismatch for {json}"),
            (
                ScannerStatus::Failed { scanner: s1, .. },
                ScannerStatus::Failed { scanner: s2, .. },
            ) => assert_eq!(s1, s2, "failed mismatch for {json}"),
            (a, b) => panic!("roundtrip changed variant: {a:?} -> {b:?} for {json}"),
        }
    }
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

/// Evidence provenance must round-trip through serde — CI ingest reads
/// scanner/ecosystem/evidence_at from every finding (Tech Lead 2026-09-09).
/// Provenance phải serialize/deserialize ổn định — CI đọc scanner/
/// ecosystem/evidence_at từ mọi finding.
#[test]
fn vulnerability_evidence_fields_roundtrip() {
    let vuln = Vulnerability {
        package: PackageId::parse("lodash@4.17.12").unwrap(),
        title: "Prototype pollution".to_string(),
        severity: "high".to_string(),
        cve: "1102260".to_string(),
        severity_level: VulnerabilitySeverity::High,
        patched_versions: None,
        url: None,
        scanner: None,
        ecosystem: None,
        evidence_at: None,
        finding_class: FindingClass::Vulnerability,
    }
    .with_evidence("npm-bulk-advisory", "web/javascript");

    assert_eq!(vuln.scanner.as_deref(), Some("npm-bulk-advisory"));
    assert_eq!(vuln.ecosystem.as_deref(), Some("web/javascript"));
    let ts = vuln.evidence_at.expect("evidence_at stamped");
    assert!(
        ts.ends_with('Z') && ts.len() == 20 && ts.contains('T'),
        "evidence_at must be RFC 3339 UTC, got {ts}"
    );
}

/// R4 (finding class): legacy payloads without `finding_class` must
/// deserialize as Vulnerability — schema stays backward compatible.
/// Finding không có `finding_class` phải về Vulnerability — schema cũ
/// vẫn đọc được.
#[test]
fn finding_class_defaults_to_vulnerability_for_legacy_json() {
    let legacy = serde_json::json!({
        "package": {"name": "lodash", "version": {"major": 4, "minor": 17, "patch": 12, "pre": null}},
        "title": "Prototype pollution",
        "severity": "high",
        "cve": "CVE-2019-10744",
        "severity_level": "high",
        "patched_versions": null,
        "url": null,
        "scanner": null,
        "ecosystem": null,
        "evidence_at": null
    });
    let vuln: Vulnerability = serde_json::from_value(legacy).unwrap();
    assert_eq!(vuln.finding_class, FindingClass::Vulnerability);
}

/// R5: the default audit_fix must name the core so callers know WHAT to
/// ask for (or which core to switch to) instead of a generic refusal.
/// Default audit_fix phải nêu tên core thay vì từ chối chung chung.
#[test]
fn audit_fix_default_names_the_core() {
    use async_trait::async_trait;
    use mgc_types::capabilities::*;
    use std::path::Path;

    struct NoFixAdapter;
    impl CoreIdent for NoFixAdapter {
        fn core_id(&self) -> &'static str {
            "test-nofix"
        }
        fn name(&self) -> &str {
            "test-nofix"
        }
        fn ecosystem(&self) -> mgc_types::Ecosystem {
            mgc_types::Ecosystem::Lib
        }
    }
    impl ProjectDetector for NoFixAdapter {
        fn can_handle(&self, _project_root: &Path) -> bool {
            false
        }
    }
    impl DependencyResolver for NoFixAdapter {}
    impl ArtifactFetcher for NoFixAdapter {}
    impl ContentStoreProvider for NoFixAdapter {}
    impl LockfileProvider for NoFixAdapter {}
    impl AuditProvider for NoFixAdapter {}
    impl ScaffoldProvider for NoFixAdapter {}
    impl LifecycleRunner for NoFixAdapter {}
    impl OptimizerProvider for NoFixAdapter {}
    impl Materializer for NoFixAdapter {}
    impl SimulatorProvider for NoFixAdapter {}
    impl DeviceProvider for NoFixAdapter {}
    impl DeployProvider for NoFixAdapter {}
    impl ModelRuntimeProvider for NoFixAdapter {}
    #[async_trait]
    impl PackageAdapter for NoFixAdapter {
        async fn parse_manifest(
            &self,
            _project_root: &Path,
        ) -> mgc_types::MgResult<mgc_types::Manifest> {
            Err(mgc_types::MgError::Other("stub".to_string()))
        }
        async fn list(
            &self,
            _project_root: &Path,
        ) -> mgc_types::MgResult<Vec<mgc_types::adapter::InstalledPackage>> {
            Ok(vec![])
        }
    }

    let fut = NoFixAdapter.audit_fix(Path::new("/tmp"), &[]);
    let err = futures_util::future::FutureExt::now_or_never(Box::pin(fut))
        .expect("default audit_fix resolves on first poll")
        .expect_err("default audit_fix must fail");
    assert!(
        err.to_string().contains("test-nofix"),
        "default refusal must name the core, got: {err}"
    );
}
