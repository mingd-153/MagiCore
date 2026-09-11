//! `govulncheck_test.rs` — govulncheck streaming-JSON parser contract
//! tests (P1 matrix row "Lib/Web/AI Go govulncheck").
//!
//! Fixtures mirror the OFFICIAL Message stream protocol
//! (golang.org/x/vuln/internal/govulncheck, protocol v1.0.0): one
//! Message per line — config | progress | SBOM | osv | finding.
//! Test hợp đồng parser stream govulncheck: fixture khớp protocol
//! Message chính thức — mỗi dòng một Message.
#![allow(clippy::unwrap_used)]

use mgc_audit::scanners::parse_govulncheck_json;

/// A realistic vulnerable stream: SBOM + OSV + module-level finding.
/// Stream dính lỗi thực tế: SBOM + OSV + finding level module.
const VULNERABLE_STREAM: &str = r#"{"config":{"protocol_version":"v1.0.0","scanner_name":"govulncheck","scanner_version":"v1.8.0","db":"vuln.go.dev","go_version":"go1.26.4","scan_level":"symbol","scan_mode":"source"}}
{"progress":{"message":"Scanning your code and P packages across M dependent modules for known vulnerabilities..."}}
{"osv":{"id":"GO-2020-0013","summary":"Denial of service via crafted Content-Length header in golang.org/x/text","aliases":["CVE-2020-14040"],"severity":[],"references":[{"url":"https://go.dev/issue/39157"}]}}
{"osv":{"id":"GO-2021-0113","summary":"Wire is vulnerable to DoS in golang.org/x/crypto","details":"A maliciously crafted input can cause excessive memory allocation.","aliases":["CVE-2021-43565"],"severity":[],"database_specific":{"severity":"HIGH"},"references":[]}}
{"SBOM":{"go_version":"go1.26.4","modules":[{"path":"example.com/vuln","version":"v1.0.0"},{"path":"golang.org/x/text","version":"v0.3.2"}],"roots":["example.com/vuln"]}}
{"finding":{"osv":"GO-2020-0013","fixed_version":"v0.3.3","trace":[{"module":"golang.org/x/text","version":"v0.3.2"}]}}
{"finding":{"osv":"GO-2021-0113","fixed_version":"v0.0.0-20211202","trace":[{"module":"golang.org/x/crypto","version":"v0.0.0-20211202"}]}}
{"finding":{"osv":"GO-2021-0113","fixed_version":"v0.0.0-20211202","trace":[{"module":"golang.org/x/crypto","version":"v0.0.0-20211202","package":"golang.org/x/crypto/ssh"}]}}
"#;

const CLEAN_STREAM: &str = r#"{"config":{"protocol_version":"v1.0.0","scan_level":"symbol","scan_mode":"source"}}
{"progress":{"message":"Scanning..."}}
{"SBOM":{"go_version":"go1.26.4","modules":[{"path":"example.com/clean","version":"v1.0.0"}],"roots":["example.com/clean"]}}
"#;

#[test]
fn parses_vulnerable_stream_with_dedup_and_evidence() {
    let parsed = parse_govulncheck_json(VULNERABLE_STREAM).unwrap();
    // SBOM module count feeds packages_audited.
    // Số module trong SBOM là packages_audited.
    assert_eq!(parsed.packages_audited, 2);

    // GO-2021-0113 appears TWICE (module+package level) — deduped to one.
    // GO-2021-0113 xuất hiện HAI LẦN (level module+package) — gộp còn một.
    assert_eq!(parsed.vulnerabilities.len(), 2);

    let text_vuln = parsed
        .vulnerabilities
        .iter()
        .find(|v| v.cve == "CVE-2020-14040")
        .expect("CVE alias must surface");
    // Display name = LAST module path segment; full path in the title.
    // Tên hiển thị = ĐOẠN CUỐI path module; path đầy đủ trong title.
    assert_eq!(text_vuln.package.name().as_str(), "text");
    assert!(
        text_vuln.title.contains("golang.org/x/text"),
        "full module path must stay in the title: {}",
        text_vuln.title
    );
    assert_eq!(text_vuln.package.version().to_string(), "0.3.2");
    assert_eq!(text_vuln.patched_versions.as_deref(), Some("v0.3.3"));
    assert_eq!(text_vuln.scanner.as_deref(), Some("govulncheck"));
    assert_eq!(text_vuln.ecosystem.as_deref(), Some("go"));
    assert!(text_vuln.evidence_at.is_some(), "evidence must be stamped");
    assert!(
        text_vuln.url.as_deref().unwrap().contains("go.dev/issue"),
        "OSV reference URL must ride the finding"
    );
}

#[test]
fn severity_maps_vulndb_label_and_defaults_info() {
    let parsed = parse_govulncheck_json(VULNERABLE_STREAM).unwrap();
    let crypto = parsed
        .vulnerabilities
        .iter()
        .find(|v| v.cve == "CVE-2021-43565")
        .unwrap();
    // database_specific.severity=HIGH → High level, "high" label.
    // database_specific.severity=HIGH → level High, nhãn "high".
    assert_eq!(crypto.severity, "high");
    assert!(matches!(
        crypto.severity_level,
        mgc_types::adapter::VulnerabilitySeverity::High
    ));

    // GO-2020-0013 has no severity block → honest "info", no guessing.
    // GO-2020-0013 không có severity → "info" trung thực, không đoán.
    let text_vuln = parsed
        .vulnerabilities
        .iter()
        .find(|v| v.cve == "CVE-2020-14040")
        .unwrap();
    assert_eq!(text_vuln.severity, "info");
}

#[test]
fn clean_stream_is_zero_findings_with_module_count() {
    let parsed = parse_govulncheck_json(CLEAN_STREAM).unwrap();
    assert_eq!(parsed.packages_audited, 1);
    assert!(parsed.vulnerabilities.is_empty());
}

#[test]
fn pretty_printed_stream_parses_like_line_stream() {
    // govulncheck v1.8 emits PRETTY multi-line JSON objects — the parser
    // must handle both encodings identically (StreamDeserializer).
    // govulncheck v1.8 phát object JSON PRETTY nhiều dòng — parser phải
    // xử lý cả hai kiểu như nhau (StreamDeserializer).
    let pretty = r#"{
  "config": {
    "protocol_version": "v1.0.0"
  }
}
{
  "SBOM": {
    "modules": [
      {
        "path": "example.com/p",
        "version": "v1.0.0"
      }
    ]
  }
}
{
  "osv": {
    "id": "GO-2025-0001",
    "summary": "pretty stream vuln",
    "database_specific": {
      "severity": "MODERATE"
    }
  }
}
{
  "finding": {
    "osv": "GO-2025-0001",
    "fixed_version": "v1.0.1",
    "trace": [
      {
        "module": "example.com/p",
        "version": "v1.0.0"
      }
    ]
  }
}"#;
    let parsed = parse_govulncheck_json(pretty).unwrap();
    assert_eq!(parsed.packages_audited, 1);
    assert_eq!(parsed.vulnerabilities.len(), 1);
    assert!(matches!(
        parsed.vulnerabilities[0].severity_level,
        mgc_types::adapter::VulnerabilitySeverity::Medium
    ));
}

#[test]
fn pseudo_version_module_keeps_finding_with_real_version_in_title() {
    // golang.org/x/crypto pseudo-version v0.0.0-20211202 is NOT plain
    // semver — the finding MUST survive (0.0.0 carrier + real version in
    // title). Dropping it would hide a real advisory.
    // Pseudo-version v0.0.0-20211202 không phải semver thuần — finding
    // PHẢI sống sót (carrier 0.0.0 + version thật trong title). Bỏ nó
    // là giấu advisory thật.
    let parsed = parse_govulncheck_json(VULNERABLE_STREAM).unwrap();
    let crypto = parsed
        .vulnerabilities
        .iter()
        .find(|v| v.cve == "CVE-2021-43565")
        .unwrap();
    assert!(
        crypto.title.contains("v0.0.0-20211202"),
        "pseudo-version must be recorded in the title: {}",
        crypto.title
    );
}

#[test]
fn dangling_osv_reference_fails_closed() {
    // A finding whose OSV entry NEVER appeared in the stream is a
    // contract violation — never a silent skip, never a fake clean.
    // Finding trỏ OSV chưa từng xuất hiện trong stream là vi phạm hợp
    // đồng — không bỏ âm thầm, không bịa sạch.
    let stream = concat!(
        r#"{"config":{"protocol_version":"v1.0.0"}}"#,
        "\n",
        r#"{"finding":{"osv":"GO-0000-0000","trace":[{"module":"example.com/x","version":"v1.0.0"}]}}"#,
        "\n",
    );
    assert!(parse_govulncheck_json(stream).is_err());
}

#[test]
fn malformed_line_fails_closed_not_clean() {
    // Truncated/garbage JSON line must ERROR — never report zero
    // findings from a stream we could not read.
    // Dòng JSON cắt quăng/rác phải LỖI — không báo 0 finding từ stream
    // đọc không nổi.
    for bad in ["", "not json", "{", r#"{"finding":"#] {
        assert!(
            parse_govulncheck_json(bad).is_err() || bad.is_empty(),
            "malformed stream must fail closed"
        );
    }
    // The empty string is special: zero messages is a valid (if odd)
    // empty stream — the SCANNER layer guards against it via exit codes.
    // Chuỗi rỗng là stream trống hợp lệ — tầng SCANNER chặn bằng exit
    // code.
}

#[test]
fn module_path_unrepresentable_falls_back_honestly() {
    // A module path that PackageName cannot hold must NEVER drop the
    // finding — the scanner keeps it with the tail segment as the
    // display name and the full path recorded in the title.
    // Module path mà PackageName không giữ được KHÔNG BAO GIỜ được bỏ
    // finding — scanner giữ với đoạn cuối làm tên hiển thị, path đầy
    // đủ ghi trong title.
    let stream = concat!(
        r#"{"osv":{"id":"GO-2024-0001","summary":"bad path module"}}"#,
        "\n",
        r#"{"finding":{"osv":"GO-2024-0001","trace":[{"module":"","version":"v1.0.0"}]}}"#,
        "\n",
    );
    let parsed = parse_govulncheck_json(stream).unwrap();
    assert_eq!(parsed.vulnerabilities.len(), 1);
    assert_eq!(
        parsed.vulnerabilities[0].package.name().as_str(),
        "unknown-module"
    );
}

#[test]
fn cvss_vector_severity_fallback() {
    // No database_specific label → CVSS vector drives the level.
    // Không có nhãn database_specific → vector CVSS quyết định level.
    let stream = concat!(
        r#"{"osv":{"id":"GO-2023-0001","summary":"cvss driven","severity":[{"score":"CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H"}]}}"#,
        "\n",
        r#"{"SBOM":{"modules":[{"path":"example.com/v","version":"v1.2.3"}]}}"#,
        "\n",
        r#"{"finding":{"osv":"GO-2023-0001","fixed_version":"v1.2.4","trace":[{"module":"example.com/v","version":"v1.2.3"}]}}"#,
        "\n",
    );
    let parsed = parse_govulncheck_json(stream).unwrap();
    let vuln = &parsed.vulnerabilities[0];
    assert!(matches!(
        vuln.severity_level,
        mgc_types::adapter::VulnerabilitySeverity::Critical
    ));
    assert_eq!(vuln.severity, "critical");
}

// ---------------------------------------------------------------------------
// Lane completion (P2 2026-09-10): reach the full 10-test/lane set.
// Phần bổ sung lane: đủ bộ 10 test/lane theo spec.
// ---------------------------------------------------------------------------

#[test]
fn lane_go_7_large_stream_over_1mb_parses() {
    let total = 6_000;
    let mut stream = String::new();
    for i in 0..total {
        stream.push_str(&format!(
            r#"{{"osv":{{"id":"GO-2026-{i:05}","summary":"v{i}","database_specific":{{"severity":"LOW"}}}}}}"#
        ));
        stream.push('\n');
        stream.push_str(&format!(
            r#"{{"finding":{{"osv":"GO-2026-{i:05}","fixed_version":"v2.0.0","trace":[{{"module":"example.com/m{i:05}","version":"v1.0.0"}}]}}}}"#
        ));
        stream.push('\n');
    }
    assert!(stream.len() > 1_000_000, "fixture must exceed 1 MB");
    let parsed = parse_govulncheck_json(&stream).unwrap();
    assert_eq!(parsed.vulnerabilities.len(), total);
}

#[test]
fn lane_go_8_empty_stream_is_zero_packages_not_fake_count() {
    // No SBOM message → packages_audited stays 0 (honest), findings 0.
    // Không message SBOM → packages_audited giữ 0 (trung thực).
    let parsed = parse_govulncheck_json("").unwrap();
    assert_eq!(parsed.packages_audited, 0);
    assert!(parsed.vulnerabilities.is_empty());
}

#[test]
fn lane_go_9_unicode_summary_and_module_survive() {
    let stream = concat!(
        r#"{"osv":{"id":"GO-2026-0009","summary":"lỗ hổng 🦀 unicode émoji"}}"#,
        "\n",
        r#"{"finding":{"osv":"GO-2026-0009","trace":[{"module":"example.com/m","version":"v1.0.0"}]}}"#,
        "\n",
    );
    let parsed = parse_govulncheck_json(stream).unwrap();
    assert!(parsed.vulnerabilities[0].title.contains("🦀"));
}

#[test]
fn lane_go_10_concurrent_streams_stay_independent() {
    let streams = [
        VULNERABLE_STREAM,
        CLEAN_STREAM,
        VULNERABLE_STREAM,
        CLEAN_STREAM,
    ];
    std::thread::scope(|scope| {
        let handles: Vec<_> = streams
            .iter()
            .map(|s| {
                let payload = s.to_string();
                scope.spawn(move || parse_govulncheck_json(&payload).unwrap())
            })
            .collect();
        for (i, h) in handles.into_iter().enumerate() {
            let parsed = h.join().unwrap();
            let expected = if i % 2 == 0 { 2 } else { 0 };
            assert_eq!(
                parsed.vulnerabilities.len(),
                expected,
                "stream {i} cross-contaminated"
            );
        }
    });
}
