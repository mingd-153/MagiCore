//! `finding_class_test.rs` — R4: every emitted Vulnerability row carries
//! its finding class (vulnerability | policy | provenance | artifact).
//! Non-CVE lanes must label their rows; CVE lanes stay `vulnerability`.
//! Mọi dòng Vulnerability phải mang class; lane phi-CVE gán đúng class.

#![allow(clippy::unwrap_used)]
// Edition 2024 makes env::set_var unsafe — override below is test-only
// with save/restore, and this binary's other test never touches OSV.
// (Set_var unsafe — override có lưu/phục hồi, test còn lại không chạm OSV.)
#![allow(unsafe_code)]

use mgc_types::adapter::FindingClass;

#[test]
fn github_actions_findings_are_policy_class() {
    let dir = tempfile::tempdir().unwrap();
    let wf = dir.path().join(".github").join("workflows");
    std::fs::create_dir_all(&wf).unwrap();
    std::fs::write(
        wf.join("ci.yml"),
        "name: ci\non: [push]\njobs:\n  b:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    )
    .unwrap();

    let report = mgc_audit::scanners::audit_github_actions(dir.path()).unwrap();
    assert!(
        !report.vulnerabilities.is_empty(),
        "unpinned uses + missing permissions must yield findings"
    );
    for v in &report.vulnerabilities {
        assert_eq!(
            v.finding_class,
            FindingClass::Policy,
            "workflow finding must be policy class, got {:?}: {}",
            v.finding_class,
            v.title
        );
    }
}

#[test]
fn osv_lane_findings_stay_vulnerability_class() {
    let pins = vec![mgc_audit::scanners::OsvPin {
        name: "lodash".to_string(),
        version: "4.17.12".to_string(),
        ecosystem: "npm",
    }];
    // MGC_OSV_API_BASE override keeps this deterministic without network:
    // point at a dead endpoint — the lane must fail CLOSED (Failed), never
    // emit rows with a wrong class.
    // Override endpoint chết để không cần mạng: lane phải Failed đóng,
    // không bao giờ phát hành dòng sai class.
    let saved = std::env::var("MGC_OSV_API_BASE").ok();
    unsafe { std::env::set_var("MGC_OSV_API_BASE", "http://127.0.0.1:1/v1") };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let report = rt
        .block_on(mgc_audit::scanners::audit_osv_pins(&pins))
        .unwrap();
    match saved {
        Some(v) => unsafe { std::env::set_var("MGC_OSV_API_BASE", v) },
        None => unsafe { std::env::remove_var("MGC_OSV_API_BASE") },
    }
    assert!(
        matches!(
            report.scanner_status,
            mgc_types::adapter::ScannerStatus::Failed { .. }
        ),
        "dead OSV endpoint must fail closed, got {:?}",
        report.scanner_status
    );
    assert!(report.vulnerabilities.is_empty());
}

/// Nợ 1: concurrent OSV queries stay fail-closed — MANY pins against a
/// dead endpoint must still collapse to ONE honest Failed (not hang,
/// not partial-clean).
/// Query đồng thời vẫn fail-closed — nhiều ghim tới endpoint chết phải
/// gộp thành Failed trung thực.
#[test]
fn osv_many_pins_against_dead_endpoint_fail_closed() {
    let pins: Vec<mgc_audit::scanners::OsvPin> = (0..12)
        .map(|i| mgc_audit::scanners::OsvPin {
            name: format!("example-dep-{i}"),
            version: "1.0.0".to_string(),
            ecosystem: "PyPI",
        })
        .collect();
    let saved = std::env::var("MGC_OSV_API_BASE").ok();
    unsafe { std::env::set_var("MGC_OSV_API_BASE", "http://127.0.0.1:1/v1") };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let report = rt
        .block_on(mgc_audit::scanners::audit_osv_pins(&pins))
        .unwrap();
    match saved {
        Some(v) => unsafe { std::env::set_var("MGC_OSV_API_BASE", v) },
        None => unsafe { std::env::remove_var("MGC_OSV_API_BASE") },
    }
    assert!(
        matches!(
            report.scanner_status,
            mgc_types::adapter::ScannerStatus::Failed { .. }
        ),
        "12 dead pins must fail closed, got {:?}",
        report.scanner_status
    );
    assert!(report.vulnerabilities.is_empty());
}
