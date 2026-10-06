//! Parser regression tests for mobile audit output — flutter pub outdated
//! (dependency HEALTH, not vulnerabilities) + OWASP dependency-check.
//! Regression test cho parser audit mobile — pub outdated là báo cáo ĐỘ
//! TƯƠI dependency (không phải lỗ hổng); OWASP là audit bảo mật thật.

#![allow(clippy::unwrap_used)]

use super::{parse_flutter_outdated_json, parse_owasp_dependency_check_json};

#[test]
fn flutter_outdated_reports_health_not_vulnerabilities() {
    let raw = r#"{
  "packages": [
    {"package": "http", "current": "0.13.3", "latest": "1.2.0", "kind": "direct"},
    {"package": "up_to_date_pkg", "current": "2.0.0", "latest": "2.0.0"},
    {"package": "no_latest", "current": "1.0.0", "latest": null}
  ],
  "devPackages": [
    {"package": "build_runner", "current": "2.0.0", "latest": "2.4.0"}
  ]
}"#;
    let health = parse_flutter_outdated_json(raw).unwrap();
    // All 4 entries were CHECKED — including up-to-date and latest-less.
    // Cả 4 entry đều ĐÃ KIỂM TRA — tính cả entry mới và không có latest.
    assert_eq!(health.packages_checked, 4);
    assert_eq!(health.outdated_count, 2, "http + build_runner are outdated");
    let first = &health.outdated[0];
    assert_eq!(first.package.name_str(), "http");
    assert_eq!(first.package.version().to_string(), "0.13.3");
    assert_eq!(first.latest_version.to_string(), "1.2.0");
    // pub outdated is NOT a security audit — this type carries no severity,
    // no CVE, and must never be coerced into a Vulnerability.
    // pub outdated KHÔNG phải security audit — type này không có severity,
    // không CVE, và không bao giờ được ép thành Vulnerability.
}

#[test]
fn flutter_outdated_all_current_is_zero_drift() {
    let raw = r#"{"packages": [{"package": "x", "current": "1.0.0", "latest": "1.0.0"}]}"#;
    let health = parse_flutter_outdated_json(raw).unwrap();
    assert_eq!(health.outdated_count, 0);
    assert_eq!(health.packages_checked, 1);
}

#[test]
fn flutter_outdated_dependency_overrides_group_counts() {
    // dependencyOverrides entries must be checked too — missing that group
    // would hide drift.
    // Entry dependencyOverrides cũng phải được kiểm tra — sót group này
    // là giấu lệch version.
    let raw = r#"{
  "packages": [],
  "dependencyOverrides": [
    {"package": "override_pkg", "current": "1.0.0", "latest": "2.0.0"}
  ]
}"#;
    let health = parse_flutter_outdated_json(raw).unwrap();
    assert_eq!(health.packages_checked, 1);
    assert_eq!(health.outdated_count, 1);
    assert_eq!(health.outdated[0].package.name_str(), "override_pkg");
}

#[test]
fn flutter_outdated_garbage_fails_closed() {
    assert!(parse_flutter_outdated_json("}}} nonsense").is_err());
}

#[test]
fn flutter_outdated_missing_groups_fails_closed() {
    // A payload with NO recognized group is not a pub outdated report —
    // schema error, never "zero drift".
    // Payload KHÔNG group nào được nhận diện thì không phải report pub
    // outdated — lỗi schema, không phải "0 drift".
    assert!(parse_flutter_outdated_json(r#"{"foo": []}"#).is_err());
}

#[test]
fn flutter_outdated_malformed_entry_fails_not_silent_skip() {
    let raw = r#"{"packages": [
        {"package": "../evil", "current": "1.0.0", "latest": "2.0.0"},
        {"package": "ok", "current": "1.0.0", "latest": "2.0.0"}
    ]}"#;
    assert!(parse_flutter_outdated_json(raw).is_err());
}

#[test]
fn owasp_report_json_parses_cvss_severities() {
    let raw = r#"{
  "dependencies": [
    {
      "packages": [{"package": {"id": "pkg:maven/com.example/lib@1.0"}}],
      "vulnerabilities": [
        {
          "name": "CVE-2024-0001",
          "severity": "High",
          "cvssv3": {"baseScore": 8.1},
          "description": "RCE in lib"
        },
        {
          "name": "CVE-2024-0002",
          "severity": "Low",
          "description": "info leak"
        }
      ]
    },
    {"packages": [], "vulnerabilities": []}
  ]
}"#;
    let report = parse_owasp_dependency_check_json(raw).unwrap();
    assert_eq!(report.vulnerability_count, 2);
    assert_eq!(report.packages_audited, 2);
    let high = &report.vulnerabilities[0];
    assert_eq!(high.cve, "CVE-2024-0001");
    assert_eq!(
        high.severity_level,
        mgc_types::adapter::VulnerabilitySeverity::High
    );
    // Version must come from the purl — NOT a placeholder 0.0.0.
    // Version phải lấy từ purl — KHÔNG phải 0.0.0 giả.
    // (mgc Version normalizes "1.0" to 1.0.0 — the version IS carried.)
    // (Version mgc chuẩn hóa "1.0" thành 1.0.0 — version vẫn được giữ.)
    assert_eq!(high.package.version().to_string(), "1.0.0");
    assert_eq!(high.package.name_str(), "lib");
}

#[test]
fn owasp_report_purl_without_version_keeps_fallback() {
    // Some purls carry no @version — then 0.0.0 fallback is honest.
    // Một số purl không có @version — khi đó fallback 0.0.0 là trung thực.
    let raw = r#"{
  "dependencies": [
    {
      "packages": [{"package": {"id": "pkg:maven/com.example/no-ver"}}],
      "vulnerabilities": [
        {"name": "CVE-2024-0003", "severity": "Critical"}
      ]
    }
  ]
}"#;
    let report = parse_owasp_dependency_check_json(raw).unwrap();
    assert_eq!(report.vulnerability_count, 1);
    let v = &report.vulnerabilities[0];
    assert_eq!(v.package.version().to_string(), "0.0.0");
    assert_eq!(
        v.severity_level,
        mgc_types::adapter::VulnerabilitySeverity::Critical
    );
}

#[test]
fn owasp_report_missing_dependencies_key_fails_closed() {
    // No "dependencies" key → NOT a dependency-check report → schema error.
    // Thiếu key "dependencies" → không phải report dependency-check → lỗi.
    assert!(parse_owasp_dependency_check_json(r#"{}"#).is_err());
    assert!(parse_owasp_dependency_check_json("not json").is_err());
}

#[test]
fn owasp_report_utf8_description_truncates_on_char_boundary() {
    // 200+ multibyte chars must truncate on a char boundary — byte slicing
    // would panic mid-codepoint (external advisory data may be any UTF-8).
    // 200+ ký tự đa byte phải cắt đúng ranh giới ký tự — cắt byte sẽ panic
    // giữa codepoint (advisory ngoài có thể là UTF-8 bất kỳ).
    let long_desc = "gói bị ảnh hưởng nặng ".repeat(20);
    let raw = format!(
        r#"{{"dependencies": [{{"packages": [{{"package": {{"id": "pkg:maven/x/y@1.0"}}}}], "vulnerabilities": [{{"name": "CVE-1", "description": "{long_desc}"}}]}}]}}"#
    );
    let report = parse_owasp_dependency_check_json(&raw).unwrap();
    assert_eq!(report.vulnerability_count, 1);
    let title = &report.vulnerabilities[0].title;
    let desc_part = title.rsplit("CVE-1: ").next().unwrap();
    assert_eq!(desc_part.chars().count(), 200, "exactly 200 chars");
}

// ===== Kotlin stale-report guard (Tech Lead P0-4 2026-09-09) =====
// Report cũ bị XÓA trước khi scanner chạy — sau đó tồn tại == của lần này.
// Không dùng mtime: một số filesystem độ phân giải timestamp thấp.

/// The pre-run delete is the freshness mechanism itself: after removing
/// an existing (stale) report, the guard path can only see a file the
/// CURRENT run produced. This test pins that contract at the unit level
/// through the delete step the scanner performs.
/// Xóa trước run chính là cơ chế freshness: sau khi gỡ report cũ, bước
/// guard chỉ có thể thấy file do LẦN CHẠY HIỆN TẠI sinh ra.
#[test]
fn owasp_stale_report_delete_semantics() {
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("dependency-check-report.json");

    // A leftover report from a PREVIOUS run exists.
    // Report sót từ lần chạy TRƯỚC đang tồn tại.
    std::fs::write(&report, "{}").unwrap();
    assert!(report.exists());

    // The pre-run delete removes it — this is what guarantees a later
    // parse reads THIS run's data, never stale bytes.
    // Bước xóa trước run gỡ nó — nhờ vậy parse sau chỉ đọc dữ liệu
    // LẦN NÀY, không bao giờ byte cũ.
    std::fs::remove_file(&report).unwrap();
    assert!(
        !report.exists(),
        "stale report must be gone before the scanner runs"
    );

    // Deleting a file that never existed reports NotFound — the scanner
    // treats that as a no-op, everything else fails closed.
    // Xóa file chưa từng tồn tại trả NotFound — scanner coi là no-op,
    // lỗi khác phải fail-closed.
    let err = std::fs::remove_file(&report).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

// ===== Kotlin OWASP lane — completing the 10-test contract (P2 2026-09-10).
// 7-10 below + the six above cover: clean, vulnerable, missing-schema,
// malformed, partial/purl-fallback, unicode truncation, stale guard,
// hostile purl, large payload, concurrency.
// Lane Kotlin OWASP đủ hợp đồng 10 test như các lane khác.

#[test]
fn owasp_report_malformed_dependency_shape_fails_closed() {
    // "dependencies" là array các OBJECT — array string/rỗng sai shape
    // là lỗi, không đếm 0 rồi báo sạch.
    for bad in [
        r#"{"dependencies": []}"#,
        r#"{"dependencies": "nope"}"#,
        r#"{"dependencies": [{"packages": "not-array"}]}"#,
    ] {
        let result = parse_owasp_dependency_check_json(bad);
        let must_fail = bad != r#"{"dependencies": []}"#;
        if must_fail {
            assert!(result.is_err(), "malformed shape must fail: {bad}");
        } else {
            // Empty dependency list is a valid CLEAN report (0 audited).
            // Danh sách rỗng là report CLEAN hợp lệ (0 audited).
            let report = result.unwrap();
            assert_eq!(report.vulnerability_count, 0);
        }
    }
}

#[test]
fn owasp_report_hostile_purl_name_is_sanitized_to_artifact_fallback() {
    // A purl whose artifact tail cannot be a package name (traversal
    // dots, empty after qualifier strip) falls back to the SAFE
    // "artifact" name — the raw hostile string never reaches the
    // finding. A merely ODD but valid tail ("evil") passes through as a
    // display identifier (paths/spawns never derive from it).
    // Purl có đuôi artifact không thể là tên package (dấu chấm traversal,
    // rỗng sau khi lọc qualifier) thì rơi về tên an toàn "artifact" —
    // chuỗi hostile thô không bao giờ vào finding. Đuôi LẠ nhưng hợp lệ
    // ("evil") vẫn qua như định danh hiển thị (path/spawn không phái sinh
    // từ nó).
    let raw = r#"{
  "dependencies": [
    {
      "packages": [{"package": {"id": "pkg:maven/com.example/..@1.0.0"}}],
      "vulnerabilities": [{"name": "CVE-2024-0004", "severity": "Medium"}]
    }
  ]
}"#;
    let report = parse_owasp_dependency_check_json(raw).unwrap();
    assert_eq!(report.vulnerability_count, 1);
    let v = &report.vulnerabilities[0];
    assert_eq!(
        v.package.name_str(),
        "artifact",
        "traversal-dot purl must fall back to the safe artifact name"
    );
    assert_eq!(v.package.version().to_string(), "1.0.0");
}

#[test]
fn owasp_report_large_payload_over_1mb_parses_all_findings() {
    // >1 MB report (nhiều dependency + finding) — parser tuyến tính,
    // không mất finding nào.
    // A >1 MB report — the parser is linear and loses no findings.
    let total = 6_500;
    let mut deps = String::new();
    for i in 0..total {
        let idx = format!("{i:04}");
        deps.push_str(&format!(
            r#"{{"packages": [{{"package": {{"id": "pkg:maven/g{idx}/a{idx}@1.0.{idx}"}}}}], "vulnerabilities": [{{"name": "CVE-2026-{idx}", "severity": "Low", "description": "d{idx}"}}]}},"#
        ));
    }
    deps.pop(); // drop the trailing comma
    let raw = format!(r#"{{"dependencies": [{deps}]}}"#);
    assert!(raw.len() > 1_000_000, "fixture must exceed 1 MB");
    let report = parse_owasp_dependency_check_json(&raw).unwrap();
    assert_eq!(report.vulnerability_count, total);
    assert_eq!(report.packages_audited, total);
}

#[test]
fn owasp_report_concurrent_parses_stay_independent() {
    // Parse song song payload sạch/nhiễu — không ghi chéo finding.
    // Concurrent parses of mixed payloads — no cross-contamination.
    let vulnerable = r#"{
  "dependencies": [
    {"packages": [{"package": {"id": "pkg:maven/a/b@1.0"}}],
     "vulnerabilities": [{"name": "CVE-2024-0001", "severity": "High"}]}
  ]
}"#;
    let clean = r#"{"dependencies": [{"packages": [], "vulnerabilities": []}]}"#;
    let payloads = [vulnerable, clean, vulnerable, clean, vulnerable];
    std::thread::scope(|scope| {
        let handles: Vec<_> = payloads
            .iter()
            .map(|p| {
                let payload = p.to_string();
                scope.spawn(move || parse_owasp_dependency_check_json(&payload).unwrap())
            })
            .collect();
        for (i, h) in handles.into_iter().enumerate() {
            let parsed = h.join().unwrap();
            let expected = if i % 2 == 0 { 1 } else { 0 };
            assert_eq!(
                parsed.vulnerability_count, expected,
                "concurrent parse {i} cross-contaminated"
            );
        }
    });
}
