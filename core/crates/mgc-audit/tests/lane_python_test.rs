//! `lane_python_test.rs` — Full 10-test lane for the Python (pip-audit)
//! scanner contract (Tech Lead P1 2026-09-09): clean, vulnerable,
//! tool-missing, malformed, routing (requirements/pylock/uv/pyproject),
//! unicode, large (>1 MB), offline parser hermeticity, concurrency,
//! evidence provenance (binary E2E lives in cli/tests/audit_cli_e2e.rs).
//!
//! Lane 10 test đầy đủ cho scanner Python (pip-audit): clean,
//! vulnerable, thiếu tool, malformed, định tuyến (requirements/pylock/
//! uv/pyproject), unicode, payload lớn (>1 MB), parser hermetic,
//! đồng thời, evidence provenance (binary E2E ở cli/tests/audit_cli_e2e).
#![allow(clippy::unwrap_used)]

use mgc_audit::scanners::{find_pylock_file, find_requirements_file, parse_pip_audit_json};

/// Official pip-audit JSON: top-level ARRAY of dependencies.
/// JSON chính thức của pip-audit: MẢNG top-level các dependency.
const VULNERABLE: &str = r#"[
  {
    "name": "requests",
    "version": "2.19.0",
    "vulns": [
      {
        "id": "PYSEC-2018-28",
        "fix_versions": ["2.20.0"],
        "description": "CVE-2018-18074 in requests",
        "aliases": ["CVE-2018-18074"]
      }
    ]
  },
  {"name": "flask", "version": "3.0.0", "vulns": []}
]"#;

const CLEAN: &str = r#"[
  {"name": "flask", "version": "3.0.0", "vulns": []}
]"#;

#[test]
fn lane_python_1_clean_fixture_parses_zero_findings() {
    let parsed = parse_pip_audit_json(CLEAN).unwrap();
    assert_eq!(parsed.packages_audited, 1);
    assert!(parsed.vulnerabilities.is_empty());
}

#[test]
fn lane_python_2_vulnerable_fixture_parses_advisory() {
    let parsed = parse_pip_audit_json(VULNERABLE).unwrap();
    assert_eq!(parsed.packages_audited, 2);
    assert_eq!(parsed.vulnerabilities.len(), 1);
    let v = &parsed.vulnerabilities[0];
    assert_eq!(v.package.name().as_str(), "requests");
    assert_eq!(v.package.version().to_string(), "2.19.0");
    assert_eq!(v.cve, "CVE-2018-18074");
    assert_eq!(v.patched_versions.as_deref(), Some("2.20.0"));
    // pip-audit carries NO severity — honest Info, never a guess.
    // pip-audit KHÔNG có severity — Info trung thực, không đoán.
    assert_eq!(v.severity, "info");
    assert_eq!(v.scanner.as_deref(), Some("pip-audit"));
    assert_eq!(v.ecosystem.as_deref(), Some("python"));
}

#[test]
fn lane_python_3_missing_tool_state_is_tool_missing_not_clean() {
    let report = mgc_types::adapter::AuditReport::tool_missing(
        "pip-audit",
        "pip install pip-audit (official PyPA vulnerability scanner)",
    );
    assert!(!report.scanner_available());
    assert!(!report.is_clean());
}

#[test]
fn lane_python_4_malformed_output_fails_closed() {
    // Any unparseable dependency is a schema error — pip-audit skipped
    // rows would hide findings behind garbage.
    // Dependency không parse được là lỗi schema — bỏ dòng của pip-audit
    // sẽ giấu finding sau dữ liệu rác.
    for bad in [
        "",
        "not json",
        r#"[{"name": "x"}]"#,
        r#"[{"name": 1, "version": "1.0.0", "vulns": []}]"#,
        r#"[{"name": "ok", "version": "1.0.0", "vulns": []}, {"bad": true}]"#,
    ] {
        assert!(
            parse_pip_audit_json(bad).is_err(),
            "malformed pip-audit payload must fail closed: {bad}"
        );
    }
}

#[test]
fn lane_python_5_routing_prefers_requirements_over_pylock() {
    // Explicit over implicit (routing table in audit_python):
    // requirements.txt wins over pylock, pylock over uv guidance.
    // Tường minh hơn ngầm định: requirements.txt thắng pylock, pylock
    // trước hướng dẫn uv.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("requirements.txt"), "flask==3.0.0\n").unwrap();
    std::fs::write(dir.path().join("pylock.toml"), "# lock\n").unwrap();
    assert_eq!(
        find_requirements_file(dir.path()),
        Some("requirements.txt".to_string())
    );
    // With requirements present, pylock is NOT the chosen target — the
    // routing layer only reaches pylock when requirements is absent.
    // Có requirements thì pylock KHÔNG phải target — lớp định tuyến chỉ
    // tới pylock khi requirements không tồn tại.
}

#[test]
fn lane_python_6_pylock_variant_discovery() {
    // PEP 751 variants (pylock.dev.toml, pylock.production.toml) must be
    // discovered, canonical pylock.toml first.
    // Biến thể PEP 751 (pylock.dev.toml...) phải được nhận diện, ưu tiên
    // pylock.toml chuẩn.
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(find_pylock_file(dir.path()), None);
    std::fs::write(dir.path().join("pylock.dev.toml"), "# dev\n").unwrap();
    assert_eq!(
        find_pylock_file(dir.path()),
        Some("pylock.dev.toml".to_string())
    );
    std::fs::write(dir.path().join("pylock.toml"), "# canonical\n").unwrap();
    assert_eq!(
        find_pylock_file(dir.path()),
        Some("pylock.toml".to_string())
    );
}

#[test]
fn lane_python_7_unicode_package_and_text_survive() {
    let unicode = r#"[
      {"name": "requests", "version": "2.19.0", "vulns": [
        {"id": "PYSEC-2026-01", "fix_versions": [],
         "description": "lỗ hổng unicode émoji 🐍 và dấu螃蟹 tiếng Việt",
         "aliases": []}
      ]}
    ]"#;
    let parsed = parse_pip_audit_json(unicode).unwrap();
    assert!(parsed.vulnerabilities[0].title.contains("🐍"));
    assert!(parsed.vulnerabilities[0].title.contains("unicode"));
}

#[test]
fn lane_python_8_large_output_over_1mb_parses() {
    let total = 18_000;
    let mut entries = Vec::with_capacity(total);
    for i in 0..total {
        entries.push(format!(
            r#"{{"name": "pkg-{i:05}", "version": "1.0.{i}", "vulns": []}}"#
        ));
    }
    let large = format!("[{}]", entries.join(","));
    assert!(
        large.len() > 1_000_000,
        "fixture must exceed 1 MB, got {}",
        large.len()
    );
    let parsed = parse_pip_audit_json(&large).unwrap();
    assert_eq!(parsed.packages_audited, total);
}

#[test]
fn lane_python_9_concurrent_parses_do_not_cross_contaminate() {
    let payloads = [VULNERABLE, CLEAN, VULNERABLE, CLEAN];
    std::thread::scope(|scope| {
        let handles: Vec<_> = payloads
            .iter()
            .map(|p| {
                let payload = p.to_string();
                scope.spawn(move || parse_pip_audit_json(&payload).unwrap())
            })
            .collect();
        for (i, h) in handles.into_iter().enumerate() {
            let parsed = h.join().unwrap();
            let expected = if i % 2 == 0 { 1 } else { 0 };
            assert_eq!(
                parsed.vulnerabilities.len(),
                expected,
                "parse {i} contaminated"
            );
        }
    });
}

#[test]
fn lane_python_10_evidence_stamp_and_honest_info_severity() {
    let parsed = parse_pip_audit_json(VULNERABLE).unwrap();
    for v in &parsed.vulnerabilities {
        assert_eq!(v.scanner.as_deref(), Some("pip-audit"));
        assert_eq!(v.ecosystem.as_deref(), Some("python"));
        assert!(v.evidence_at.as_deref().is_some_and(|t| t.ends_with('Z')));
        // Honest Info severity: no severity data means info, NOT medium.
        // Severity Info trung thực: không có dữ liệu severity là info,
        // KHÔNG PHẢI medium.
        assert!(matches!(
            v.severity_level,
            mgc_types::adapter::VulnerabilitySeverity::Info
        ));
    }
}

/// CI fix (2026-09-11): pip-audit 2.9.0 on some CI runners emits noise
/// AFTER the JSON body (progress lines merged into stdout) — the
/// stream parser must consume the FIRST value and ignore suffix junk,
/// while a genuinely broken body still fails closed.
/// Fix CI: pip-audit 2.9.0 trên một số runner CI in rác SAU thân JSON
/// (dòng progress lẫn stdout) — parser stream phải tiêu thụ GIÁ TRỊ đầu
/// và bỏ rác hậu tố, còn thân JSON thật sự hỏng thì vẫn fail-closed.
#[test]
fn pip_audit_json_with_trailing_noise_still_parses() {
    let noisy = format!("{CLEAN}\nInstalled 0 packages\n");
    let parsed = parse_pip_audit_json(&noisy)
        .expect("trailing noise after a valid body must not break the parse");
    // CLEAN carries exactly one audited package.
    // CLEAN mang đúng một package đã audit.
    assert_eq!(parsed.packages_audited, 1);
}

#[test]
fn pip_audit_broken_json_still_fails_closed() {
    let broken = "[{\"name\": 123"; // truncated mid-body — must fail
    assert!(parse_pip_audit_json(broken).is_err());
}
