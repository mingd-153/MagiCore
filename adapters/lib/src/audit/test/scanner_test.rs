//! Parser regression tests for audit JSON output — cargo-audit + pip-audit.
//! Regression test cho parser JSON audit — fixture cargo-audit CAPTURED từ
//! `cargo audit --json --no-fetch` thật (cargo-audit 0.22.2, 2026-09-09);
//! fixture pip-audit theo schema chính thức PyPA (top-level array).

#![allow(clippy::unwrap_used)]

use super::{parse_cargo_audit_json, parse_pip_audit_json};
use crate::audit::scanner::{find_pylock_file, find_requirements_file};

/// Fixture captured verbatim from a real `cargo audit --json --no-fetch`
/// run (cargo-audit 0.22.2) against this workspace — contains the genuine
/// RUSTSEC-2023-0071 advisory (rsa, CVSS 5.9 Medium via NVD).
/// Fixture chụp nguyên vẹn từ `cargo audit --json --no-fetch` thật
/// (cargo-audit 0.22.2) trên chính workspace này — chứa advisory
/// RUSTSEC-2023-0071 thật (rsa, CVSS 5.9 Medium theo NVD).
const CARGO_AUDIT_REAL: &str = include_str!("fixtures/cargo-audit-real-0.22.2.json");
/// Same-schema variant with zero findings (found=false, count=0, list=[]).
/// Biến thể cùng schema với 0 finding.
const CARGO_AUDIT_CLEAN: &str = include_str!("fixtures/cargo-audit-clean-0.22.2.json");

#[test]
fn cargo_audit_real_fixture_parses_advisory_fields() {
    let parsed = parse_cargo_audit_json(CARGO_AUDIT_REAL).unwrap();
    assert_eq!(
        parsed.vulnerabilities.len(),
        1,
        "real fixture has 1 finding"
    );
    let v = &parsed.vulnerabilities[0];
    // Advisory id/title live INSIDE advisory — verified against real output.
    assert_eq!(
        v.cve, "CVE-2023-49092",
        "CVE alias must be picked from aliases"
    );
    assert!(v.title.starts_with("RUSTSEC-2023-0071"), "{}", v.title);
    assert!(v.title.contains("Marvin Attack"), "{}", v.title);
    assert_eq!(
        v.severity_level,
        mgc_types::adapter::VulnerabilitySeverity::Medium,
        "CVSS:3.1/AV:N/AC:H/PR:N/UI:N/S:U/C:H/I:N/A:N scores 5.9 → Medium"
    );
    // rsa 0.9.10 — package name/version from the real dependency graph.
    assert_eq!(v.package.name_str(), "rsa");
    assert_eq!(v.package.version().to_string(), "0.9.10");
}

#[test]
fn cargo_audit_real_fixture_dependency_count_from_lockfile() {
    // packages_audited must be lockfile.dependency-count (607 = the whole
    // audited graph incl. transitive), NOT a Cargo.toml line count.
    let parsed = parse_cargo_audit_json(CARGO_AUDIT_REAL).unwrap();
    assert_eq!(parsed.packages_audited, 607);
}

#[test]
fn cargo_audit_clean_fixture_is_zero_findings() {
    let parsed = parse_cargo_audit_json(CARGO_AUDIT_CLEAN).unwrap();
    assert!(
        parsed.vulnerabilities.is_empty(),
        "clean audit = 0 findings"
    );
    assert_eq!(parsed.packages_audited, 607, "count comes from lockfile");
}

#[test]
fn cargo_audit_missing_required_key_fails_closed() {
    // No "vulnerabilities" object → NOT a cargo-audit report → schema error.
    // Thiếu object "vulnerabilities" → không phải report cargo-audit → lỗi.
    assert!(parse_cargo_audit_json(r#"{"lockfile": {"dependency-count": 5}}"#).is_err());
    // Missing nested advisory id → malformed finding → error, not a skip.
    let malformed = r#"{"vulnerabilities": {"found": true, "count": 1, "list": [
        {"package": {"name": "x", "version": "1.0.0"}, "versions": {"patched": []}}
    ]}}"#;
    assert!(parse_cargo_audit_json(malformed).is_err());
    // count/list mismatch → inconsistent payload → error.
    let inconsistent = r#"{"vulnerabilities": {"found": true, "count": 3, "list": []}}"#;
    assert!(parse_cargo_audit_json(inconsistent).is_err());
}

#[test]
fn cargo_audit_garbage_fails_closed() {
    assert!(parse_cargo_audit_json("not json at all").is_err());
}

#[test]
fn cargo_audit_count_list_mismatch_fails() {
    let raw = r#"{"vulnerabilities": {"found": false, "count": 2, "list": []}}"#;
    assert!(parse_cargo_audit_json(raw).is_err());
}

#[test]
fn cargo_audit_rejects_unparseable_package_finding_not_silent_skip() {
    // A finding whose package/version cannot be represented must ERROR —
    // silently skipping a security finding fakes a clean audit.
    // Finding mà package/version không biểu diễn được phải LỖI — bỏ âm
    // thầm finding bảo mật đồng nghĩa báo sạch giả.
    let raw = r#"{"vulnerabilities": {"found": true, "count": 1, "list": [
        {"advisory": {"id": "RUSTSEC-2024-0001", "title": "t"},
         "package": {"name": "../evil", "version": "1.0.0"}}
    ]}}"#;
    assert!(parse_cargo_audit_json(raw).is_err());
}

/// pip-audit official `--format json` schema: a TOP-LEVEL ARRAY of
/// dependencies, each with nested `vulns[]`. Source:
/// https://pypi.org/project/pip-audit/ (JSON output docs).
/// Schema chính thức pip-audit `--format json`: MẢNG TOP-LEVEL dependency,
/// mỗi dep có `vulns[]` lồng nhau.
#[test]
fn pip_audit_official_schema_parses_nested_vulns() {
    let raw = r#"[
  {
    "name": "flask",
    "version": "0.5",
    "vulns": [
      {
        "id": "PYSEC-2024-0001",
        "fix_versions": ["1.0"],
        "aliases": ["CVE-2024-1234", "GHSA-xxxx"],
        "description": "bad thing"
      }
    ]
  }
]"#;
    let parsed = parse_pip_audit_json(raw).unwrap();
    assert_eq!(parsed.packages_audited, 1, "one dependency audited");
    assert_eq!(parsed.vulnerabilities.len(), 1, "one vuln inside it");
    let v = &parsed.vulnerabilities[0];
    assert_eq!(v.package.name_str(), "flask");
    // mgc Version normalizes "0.5" to 0.5.0 — the version IS carried.
    // Version mgc chuẩn hóa "0.5" thành 0.5.0 — version vẫn được giữ.
    assert_eq!(v.package.version().to_string(), "0.5.0");
    assert_eq!(v.cve, "CVE-2024-1234", "CVE picked from aliases");
    assert_eq!(v.patched_versions.as_deref(), Some("1.0"));
    assert!(v.title.contains("bad thing"));
    assert_eq!(
        v.severity_level,
        mgc_types::adapter::VulnerabilitySeverity::Info
    );
}

#[test]
fn pip_audit_zero_vulns_still_counts_dependencies() {
    // Dependencies audited but no vulns — the count is the dependency count,
    // not the vulnerability count.
    // Dependency đã audit nhưng không có vuln — packages_audited là số
    // dependency, không phải số vulnerability.
    let raw = r#"[
  {"name": "requests", "version": "2.31.0", "vulns": []},
  {"name": "flask", "version": "3.0.0", "vulns": []}
]"#;
    let parsed = parse_pip_audit_json(raw).unwrap();
    assert_eq!(parsed.packages_audited, 2);
    assert!(parsed.vulnerabilities.is_empty());
}

#[test]
fn pip_audit_object_schema_is_rejected_not_fake_parsed() {
    // The OLD parser accepted an object with a "vulnerabilities" array —
    // that shape is NOT official pip-audit output; accepting it fakes
    // compatibility. Fail closed.
    // Parser CŨ chấp nhận object có "vulnerabilities" — schema đó
    // KHÔNG phải output chính thức pip-audit; chấp nhận là giả tương
    // thích. Fail-closed.
    let bogus = r#"{"vulnerabilities": [{"name": "x", "id": "PYSEC-1"}]}"#;
    assert!(parse_pip_audit_json(bogus).is_err());
}

#[test]
fn pip_audit_malformed_entry_fails_not_silent_skip() {
    let raw = r#"[
  {"name": "good", "version": "1.0.0", "vulns": []},
  {"name": "../bad", "version": "1.0.0", "vulns": []}
]"#;
    assert!(parse_pip_audit_json(raw).is_err());
}

#[test]
fn pip_audit_garbage_fails_closed() {
    assert!(parse_pip_audit_json("not json").is_err());
}

#[test]
fn pip_audit_utf8_description_truncates_on_char_boundary() {
    // 200+ CHARS of multibyte text (Vietnamese diacritics) must truncate on
    // a char boundary — byte slicing would panic mid-codepoint.
    // 200+ KÝ TỰ tiếng Việt phải cắt đúng ranh giới ký tự — cắt theo byte
    // sẽ panic giữa codepoint.
    let long_desc = "lỗ hổng nghiêm trọng ".repeat(20); // 420 chars, all multibyte
    let raw = format!(
        r#"[{{"name": "pkg", "version": "1.0.0", "vulns": [{{"id": "PYSEC-1", "fix_versions": [], "description": "{long_desc}"}}]}}]"#
    );
    let parsed = parse_pip_audit_json(&raw).unwrap();
    assert_eq!(parsed.vulnerabilities.len(), 1);
    let title = &parsed.vulnerabilities[0].title;
    let desc_part = title.rsplit("PYSEC-1: ").next().unwrap();
    assert_eq!(desc_part.chars().count(), 200, "exactly 200 chars");
}

#[test]
fn pip_audit_progress_noise_before_json_still_parses() {
    // Tool wrappers may print progress noise before the JSON array —
    // parser must skip to the first '['.
    // Wrapper có thể in nhiễu trước mảng JSON — parser phải nhảy tới '['.
    let raw =
        "Resolving dependencies...\n[{\"name\": \"pkg\", \"version\": \"1.0.0\", \"vulns\": []}]";
    let parsed = parse_pip_audit_json(raw).unwrap();
    assert_eq!(parsed.packages_audited, 1);
}

#[test]
fn cargo_audit_progress_noise_before_json_still_parses() {
    let raw = format!("Resolving dependencies...\n{CARGO_AUDIT_REAL}");
    let parsed = parse_cargo_audit_json(&raw).unwrap();
    assert_eq!(parsed.vulnerabilities.len(), 1);
}

// ===== Python audit target routing (Tech Lead P0-2 2026-09-09) =====
// pip-audit --locked cho PEP 751; -r cho requirements; fail-closed còn lại.

#[test]
fn pylock_detection_covers_canonical_and_pep751_variants() {
    // Canonical pylock.toml wins.
    // pylock.toml chuẩn thắng.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("pylock.dev.toml"), "# lock\n").unwrap();
    std::fs::write(dir.path().join("pylock.toml"), "# canonical lock\n").unwrap();
    assert_eq!(find_pylock_file(dir.path()).as_deref(), Some("pylock.toml"));

    // Without canonical, first sorted pylock.*.toml variant is found —
    // PEP 751 allows pylock.dev.toml / pylock.production.toml etc.
    // Không có file chuẩn, biến thể pylock.*.toml đầu tiên theo thứ tự —
    // PEP 751 cho phép pylock.dev.toml / pylock.production.toml...
    let dir2 = tempfile::tempdir().unwrap();
    std::fs::write(dir2.path().join("pylock.production.toml"), "# prod lock\n").unwrap();
    std::fs::write(dir2.path().join("pylock.dev.toml"), "# dev lock\n").unwrap();
    let found = find_pylock_file(dir2.path());
    assert!(
        matches!(
            found.as_deref(),
            Some("pylock.dev.toml") | Some("pylock.production.toml")
        ),
        "must discover a PEP 751 variant, got {found:?}"
    );

    // No pylock at all → None (routing falls through to uv/pyproject).
    // Không có pylock → None (routing rơi sang uv/pyproject).
    let dir3 = tempfile::tempdir().unwrap();
    assert!(find_pylock_file(dir3.path()).is_none());
}

#[test]
fn requirements_detection_prefers_canonical_then_sorted_variants() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("requirements-dev.txt"), "-e .\n").unwrap();
    assert_eq!(
        find_requirements_file(dir.path()).as_deref(),
        Some("requirements-dev.txt")
    );
    std::fs::write(dir.path().join("requirements.txt"), "# canonical\n").unwrap();
    assert_eq!(
        find_requirements_file(dir.path()).as_deref(),
        Some("requirements.txt")
    );
}
