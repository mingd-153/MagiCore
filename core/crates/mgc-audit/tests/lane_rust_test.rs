//! `lane_rust_test.rs` — Full 10-test lane for the Rust (cargo-audit)
//! scanner contract (Tech Lead P1 2026-09-09 "10-test/lane"):
//! clean, vulnerable, tool-missing, malformed, partial, unicode, large
//! (>1 MB), offline-behavior, concurrency, binary E2E (covered by
//! cli/tests/audit_cli_e2e.rs; here the parser lane is hermetic).
//!
//! Lane 10 test đầy đủ cho scanner Rust (cargo-audit): clean,
//! vulnerable, thiếu tool, malformed, partial, unicode, payload lớn
//! (>1 MB), offline, đồng thời, binary E2E (do cli/tests/audit_cli_e2e
//! đảm nhiệm; lane parser ở đây hermetic).
#![allow(clippy::unwrap_used)]

use mgc_audit::scanners::parse_cargo_audit_json;

/// Real cargo-audit 0.22.2 vulnerable fixture shape (captured from a
/// live run, RUSTSEC-2023-0071).
/// Fixture cargo-audit 0.22.2 dính lỗi thật (chụp từ run sống).
const VULNERABLE: &str = r#"{
  "database": {"advisory-count": 683, "commit": "abc", "date": "2026-09-09", "lock": "def", "source": "https://github.com/RustSec/advisory-db", "updated": "2026-09-09"},
  "lockfile": {"dependency-count": 2},
  "settings": {"target_distro": null, "os": null, "cpu": null},
  "vulnerabilities": {
    "count": 1,
    "found": true,
    "list": [
      {
        "advisory": {
          "id": "RUSTSEC-2023-0071",
          "package": "rsa",
          "title": "Marvin Attack: potential key recovery through timing sidechannels",
          "date": "2023-10-27",
          "severity": "medium",
          "cvss": "CVSS:3.1/AV:N/AC:H/PR:N/UI:N/S:U/C:H/I:N/A:N",
          "description": "The_RSA_recovery",
          "url": "https://rustsec.org/advisories/RUSTSEC-2023-0071",
          "references": ["https://github.com/RustCrypto/RSA/releases"],
          "aliases": ["CVE-2023-49092"]
        },
        "versions": {"patched": [">=0.9.12"]},
        "package": {"name": "rsa", "version": "0.9.10"}
      }
    ]
  },
  "warnings": []
}"#;

const CLEAN: &str = r#"{
  "lockfile": {"dependency-count": 1},
  "vulnerabilities": {"count": 0, "found": false, "list": []},
  "warnings": []
}"#;

#[test]
fn lane_rust_1_clean_fixture_parses_zero_findings() {
    let parsed = parse_cargo_audit_json(CLEAN).unwrap();
    assert_eq!(parsed.packages_audited, 1);
    assert!(parsed.vulnerabilities.is_empty());
}

#[test]
fn lane_rust_2_vulnerable_fixture_parses_advisory() {
    let parsed = parse_cargo_audit_json(VULNERABLE).unwrap();
    assert_eq!(parsed.packages_audited, 2);
    assert_eq!(parsed.vulnerabilities.len(), 1);
    let v = &parsed.vulnerabilities[0];
    assert_eq!(v.package.name().as_str(), "rsa");
    assert_eq!(v.cve, "CVE-2023-49092");
    assert_eq!(v.patched_versions.as_deref(), Some(">=0.9.12"));
    assert_eq!(v.scanner.as_deref(), Some("cargo-audit"));
    assert_eq!(v.ecosystem.as_deref(), Some("rust"));
    assert!(v.evidence_at.is_some());
}

#[test]
fn lane_rust_3_missing_tool_state_is_tool_missing_not_clean() {
    // Tool-missing is decided in audit_rust() BEFORE parsing — prove the
    // report constructor is fail-closed: tool_missing() is NEVER clean.
    // Trạng thái thiếu tool quyết định trong audit_rust() TRƯỚC parse —
    // chứng minh constructor fail-closed: tool_missing() KHÔNG BAO GIỜ clean.
    let report = mgc_types::adapter::AuditReport::tool_missing(
        "cargo-audit",
        "cargo install cargo-audit --locked",
    );
    assert!(!report.scanner_available());
    assert!(!report.is_clean());
}

#[test]
fn lane_rust_4_malformed_output_fails_closed() {
    for bad in [
        "",
        "not json",
        "{",
        r#"{"vulnerabilities": "nope"}"#,
        r#"{"vulnerabilities": {"found": true, "list": [{"advisory": {}}]}}"#,
    ] {
        assert!(
            parse_cargo_audit_json(bad).is_err(),
            "malformed payload must fail closed, got a clean parse for: {bad}"
        );
    }
}

#[test]
fn lane_rust_5_count_list_mismatch_fails_closed() {
    // count=2 but list holds 1 → upstream schema drift → error, never
    // an incomplete "clean".
    // count=2 nhưng list có 1 → lệch schema upstream → lỗi, không bao
    // giờ là "sạch" thiếu sót.
    let mismatched = r#"{
      "vulnerabilities": {"count": 2, "found": true, "list": [
        {"advisory": {"id": "RUSTSEC-2023-0071", "title": "t"},
         "versions": {"patched": [">=1.0.0"]},
         "package": {"name": "rsa", "version": "0.9.10"}}
      ]}
    }"#;
    assert!(parse_cargo_audit_json(mismatched).is_err());
}

#[test]
fn lane_rust_6_unicode_package_and_text_survive() {
    // Unicode in advisory title/description and package names is
    // external data — must round-trip, never panic, never drop.
    // Unicode trong title/description advisory và tên package là dữ
    // liệu ngoài — phải qua nguyên vẹn, không panic, không bỏ.
    let unicode = r#"{
      "lockfile": {"dependency-count": 1},
      "vulnerabilities": {"count": 1, "found": true, "list": [
        {"advisory": {"id": "RUSTSEC-2026-0001", "title": "Lỗ hổng dấu螃蟹 émoji 🦀 unicode",
                      "cvss": null, "aliases": []},
         "versions": {"patched": []},
         "package": {"name": "rsa", "version": "0.9.10"}}
      ]}
    }"#;
    let parsed = parse_cargo_audit_json(unicode).unwrap();
    assert!(parsed.vulnerabilities[0].title.contains("unicode"));
    assert!(parsed.vulnerabilities[0].title.contains("🦀"));
}

#[test]
fn lane_rust_7_large_output_over_1mb_parses() {
    // 1 MB+ of findings: the parser must complete and count every row —
    // byte-capped capture is the exec layer's job, the parser is
    // linear-time and lossless on whatever it receives.
    // Hơn 1 MB finding: parser phải xử lý hết và đếm đúng từng dòng —
    // capture giới hạn byte là việc tầng exec; parser chạy tuyến tính và
    // không mất gì trên những gì nhận được.
    let mut findings = String::new();
    let total = 8_000;
    let mut entries: Vec<String> = Vec::with_capacity(total);
    for i in 0..total {
        entries.push(format!(
            r#"{{"advisory": {{"id": "RUSTSEC-2026-{i:04}", "title": "t{i}"}},
               "versions": {{"patched": [">=1.0.0"]}},
               "package": {{"name": "crate-{i:04}", "version": "0.1.0"}}}}"#
        ));
    }
    findings.push_str(&entries.join(","));
    let large = format!(
        r#"{{"lockfile": {{"dependency-count": {total}}},
        "vulnerabilities": {{"count": {total}, "found": true, "list": [{findings}]}}}}"#
    );
    assert!(
        large.len() > 1_000_000,
        "fixture must exceed 1 MB, got {}",
        large.len()
    );
    let parsed = parse_cargo_audit_json(&large).unwrap();
    assert_eq!(parsed.vulnerabilities.len(), total);
}

#[test]
fn lane_rust_8_cvss_vector_derives_severity() {
    // No severity label → CVSS vector drives the level (offline-capable:
    // no network, no advisory DB).
    // Không nhãn severity → vector CVSS quyết định level (không cần
    // mạng, không cần advisory DB).
    let cvss_only = r#"{
      "vulnerabilities": {"count": 1, "found": true, "list": [
        {"advisory": {"id": "RUSTSEC-2026-0002", "title": "cvss only",
                      "cvss": "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H"},
         "package": {"name": "rsa", "version": "0.9.10"}}
      ]}
    }"#;
    let parsed = parse_cargo_audit_json(cvss_only).unwrap();
    assert!(matches!(
        parsed.vulnerabilities[0].severity_level,
        mgc_types::adapter::VulnerabilitySeverity::Critical
    ));
}

#[test]
fn lane_rust_9_concurrent_parses_do_not_cross_contaminate() {
    // Parser is pure — concurrent parses of DIFFERENT payloads must stay
    // independent (no shared state, no cross-written findings).
    // Parser thuần — parse ĐỒNG THỜI các payload KHÁC nhau phải độc lập
    // (không state chung, không ghi chéo finding).
    let payloads = [VULNERABLE, CLEAN, VULNERABLE, CLEAN, VULNERABLE];
    std::thread::scope(|scope| {
        let handles: Vec<_> = payloads
            .iter()
            .map(|p| {
                let payload = p.to_string();
                scope.spawn(move || parse_cargo_audit_json(&payload).unwrap())
            })
            .collect();
        for (i, h) in handles.into_iter().enumerate() {
            let parsed = h.join().unwrap();
            let expected = if i % 2 == 0 { 1 } else { 0 };
            assert_eq!(
                parsed.vulnerabilities.len(),
                expected,
                "concurrent parse {i} cross-contaminated"
            );
        }
    });
}

#[test]
fn lane_rust_10_evidence_stamp_on_every_finding() {
    // Binary E2E lives in cli/tests/audit_cli_e2e.rs; the lane's last
    // contract here: EVERY finding carries scanner+ecosystem+timestamp
    // so machine formats can attribute evidence.
    // Binary E2E nằm ở cli/tests/audit_cli_e2e.rs; hợp đồng cuối của
    // lane: MỌI finding có scanner+ecosystem+timestamp để format máy
    // quy được nguồn evidence.
    let parsed = parse_cargo_audit_json(VULNERABLE).unwrap();
    for v in &parsed.vulnerabilities {
        assert_eq!(v.scanner.as_deref(), Some("cargo-audit"));
        assert_eq!(v.ecosystem.as_deref(), Some("rust"));
        assert!(v.evidence_at.as_deref().is_some_and(|t| t.ends_with('Z')));
    }
}
