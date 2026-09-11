//! `output_cyclonedx_test.rs` — CycloneDX 1.5 output contract tests.
//! Verifies the Vulnerable BOM shape: components dedupe per affected
//! package, vulnerabilities carry source/severity/affects, unverified
//! reports NEVER emit vulnerabilities, serials are content-deterministic.
//! Test hợp đồng output CycloneDX 1.5: component khử trùng theo package
//! dính, vulnerability giữ source/severity/affects, report chưa xác
//! thực KHÔNG BAO GIỜ emit vulnerability, serial all định theo nội dung.

#![allow(clippy::unwrap_used)]

use mgc_audit::output::cyclonedx::{CYCLONEDX_BOM_FORMAT, CYCLONEDX_SPEC_VERSION, to_cyclonedx};
use mgc_types::adapter::{AuditReport, Vulnerability, VulnerabilitySeverity};
use mgc_types::{PackageId, PackageName, Version};

/// Build one vulnerability row with full evidence stamp.
/// Dựng một dòng vulnerability có gắn evidence đầy đủ.
fn vuln(name: &str, version: &str, cve: &str, level: VulnerabilitySeverity) -> Vulnerability {
    Vulnerability {
        package: PackageId::new(
            PackageName::new(name.to_string()).unwrap(),
            Version::parse(version).unwrap(),
        ),
        title: format!("{cve}: advisory title"),
        severity: "high".to_string(),
        cve: cve.to_string(),
        severity_level: level,
        patched_versions: Some(">=1.2.3".to_string()),
        url: Some("https://example.com/advisory".to_string()),
        scanner: None,
        ecosystem: None,
        evidence_at: None,
    }
    .with_evidence("cargo-audit", "rust")
}

#[test]
fn cyclonedx_emits_component_and_vulnerability_per_finding() {
    let report = AuditReport::scanned(
        12,
        vec![
            vuln(
                "rsa",
                "0.9.10",
                "RUSTSEC-2023-0071",
                VulnerabilitySeverity::Critical,
            ),
            vuln(
                "openssl",
                "0.10.1",
                "CVE-2025-0001",
                VulnerabilitySeverity::High,
            ),
        ],
    );

    let bom = to_cyclonedx(&report);
    assert_eq!(bom.bom_format, CYCLONEDX_BOM_FORMAT);
    assert_eq!(bom.spec_version, CYCLONEDX_SPEC_VERSION);
    assert_eq!(
        bom.components.len(),
        2,
        "one component per affected package"
    );
    assert_eq!(bom.vulnerabilities.len(), 2);

    // Component purl shape: pkg:cargo/rsa@0.9.10.
    // Hình purl component: pkg:cargo/rsa@0.9.10.
    let rsa = bom.components.iter().find(|c| c.name == "rsa").unwrap();
    assert_eq!(rsa.purl, "pkg:cargo/rsa@0.9.10");
    assert_eq!(rsa.component_type, "library");
    assert!(rsa.bom_ref.starts_with("purl:cargo/rsa@"));

    // Vulnerability: id, source=scanner, rating, affects the component.
    // Vulnerability: id, source=scanner, rating, trỏ component dính.
    let vuln_row = bom
        .vulnerabilities
        .iter()
        .find(|v| v.id == "RUSTSEC-2023-0071")
        .unwrap();
    assert_eq!(vuln_row.source.name, "cargo-audit");
    assert_eq!(vuln_row.ratings[0].severity, "critical");
    assert!(
        !vuln_row.affects.is_empty(),
        "affects must reference the component"
    );
    assert!(vuln_row.affects[0].r#ref.contains("rsa@0.9.10"));

    // Evidence provenance travels as properties.
    // Evidence đi kèm trong properties.
    let evidence_prop = vuln_row
        .properties
        .iter()
        .find(|p| p.name == "magicore:evidence_at")
        .unwrap();
    assert!(
        !evidence_prop.value.is_empty(),
        "evidence_at must be stamped"
    );
}

#[test]
fn cyclonedx_dedupes_component_across_multiple_advisories() {
    // Two advisories hitting the SAME package → ONE component, TWO vulns.
    // Hai advisory dính CÙNG package → MỘT component, HAI vulnerability.
    let report = AuditReport::scanned(
        5,
        vec![
            vuln(
                "rsa",
                "0.9.10",
                "RUSTSEC-2023-0071",
                VulnerabilitySeverity::Critical,
            ),
            vuln(
                "rsa",
                "0.9.10",
                "RUSTSEC-2024-0002",
                VulnerabilitySeverity::Medium,
            ),
        ],
    );

    let bom = to_cyclonedx(&report);
    assert_eq!(bom.components.len(), 1, "same package = same component");
    assert_eq!(bom.vulnerabilities.len(), 2);
}

#[test]
fn cyclonedx_unverified_report_never_emits_vulnerabilities() {
    // A ToolMissing report with (hypothetically) findings attached must
    // still yield ZERO vulnerabilities — unverified data never becomes
    // CycloneDX evidence. The tool carries the loud UNVERIFIED property.
    // Report ToolMissing dù có finding gắn kèm cũng phải ra KHÔNG
    // vulnerability — dữ liệu chưa xác thực không thành evidence
    // CycloneDX. Tool gắn property UNVERIFIED rõ ràng.
    let mut report = AuditReport::tool_missing("cargo-audit", "cargo install cargo-audit");
    report.vulnerabilities = vec![vuln(
        "rsa",
        "0.9.10",
        "RUSTSEC-2023-0071",
        VulnerabilitySeverity::Critical,
    )];

    let bom = to_cyclonedx(&report);
    assert!(
        bom.vulnerabilities.is_empty(),
        "unverified must not emit vulns"
    );
    assert!(
        bom.components.is_empty(),
        "unverified must not emit components"
    );
    let status = bom.metadata.tools.components[0]
        .properties
        .iter()
        .find(|p| p.name == "magicore:status")
        .expect("UNVERIFIED marker must be present");
    assert!(status.value.contains("UNVERIFIED"));
}

#[test]
fn cyclonedx_clean_report_has_no_vulnerabilities_section() {
    // Clean Available scan → zero vulns; `vulnerabilities` is skipped
    // entirely (empty vec), not an empty array left in the payload.
    // Scan Available sạch → 0 vuln; trường `vulnerabilities` bị bỏ hẳn
    // (vec rỗng), không để mảng rỗng trong payload.
    let report = AuditReport::clean(3);
    let bom = to_cyclonedx(&report);
    assert!(bom.vulnerabilities.is_empty());
    assert!(bom.components.is_empty());

    let json = serde_json::to_value(&bom).unwrap();
    assert!(
        json.get("vulnerabilities").is_none(),
        "empty arrays must be skipped per skip_serializing_if"
    );
    assert_eq!(json["bomFormat"], "CycloneDX");
    assert_eq!(json["specVersion"], "1.5");
    assert!(
        json["serialNumber"]
            .as_str()
            .unwrap()
            .starts_with("urn:uuid:")
    );
}

#[test]
fn cyclonedx_serial_is_content_deterministic() {
    // Same report content → same serial (CI diffable); different
    // finding counts → different serial.
    // Cùng nội dung → cùng serial (CI diff được); khác số finding →
    // khác serial.
    let report = AuditReport::scanned(
        2,
        vec![vuln(
            "rsa",
            "0.9.10",
            "RUSTSEC-2023-0071",
            VulnerabilitySeverity::Critical,
        )],
    );
    let serial_a = to_cyclonedx(&report).serial_number;
    let serial_b = to_cyclonedx(&report).serial_number;
    assert_eq!(serial_a, serial_b, "same content = same serial");

    let other = AuditReport::scanned(
        2,
        vec![
            vuln(
                "rsa",
                "0.9.10",
                "RUSTSEC-2023-0071",
                VulnerabilitySeverity::Critical,
            ),
            vuln(
                "openssl",
                "0.10.1",
                "CVE-2025-0001",
                VulnerabilitySeverity::High,
            ),
        ],
    );
    let serial_c = to_cyclonedx(&other).serial_number;
    assert_ne!(serial_a, serial_c, "different content = different serial");
}

#[test]
fn cyclonedx_severity_labels_match_spec() {
    // All five contract levels map onto CycloneDX 1.5 severity names.
    // Năm level của hợp đồng map đúng tên severity CycloneDX 1.5.
    let report = AuditReport::scanned(
        5,
        vec![
            vuln("a", "1.0.0", "CVE-1", VulnerabilitySeverity::Critical),
            vuln("b", "1.0.0", "CVE-2", VulnerabilitySeverity::High),
            vuln("c", "1.0.0", "CVE-3", VulnerabilitySeverity::Medium),
            vuln("d", "1.0.0", "CVE-4", VulnerabilitySeverity::Low),
            vuln("e", "1.0.0", "CVE-5", VulnerabilitySeverity::Info),
        ],
    );
    let bom = to_cyclonedx(&report);
    let severities: Vec<&str> = bom
        .vulnerabilities
        .iter()
        .map(|v| v.ratings[0].severity.as_str())
        .collect();
    for expected in ["critical", "high", "medium", "low", "info"] {
        assert!(severities.contains(&expected), "missing {expected}");
    }
}

#[test]
fn cyclonedx_purl_ecosystem_mapping() {
    // Ecosystem tags map to correct purl prefixes; unknown falls to
    // `generic` — never a crash, never a wrong ecosystem guess.
    // Tag ecosystem map đúng tiền tố purl; không biết thì `generic` —
    // không crash, không đoán sai ecosystem.
    let mut py = vuln(
        "requests",
        "2.19.0",
        "CVE-2018-18074",
        VulnerabilitySeverity::Low,
    );
    py.ecosystem = Some("python".to_string());
    let mut web = vuln(
        "lodash",
        "4.17.20",
        "CVE-2020-8203",
        VulnerabilitySeverity::High,
    );
    web.ecosystem = Some("web/javascript".to_string());
    let mut unknown = vuln("mystery", "1.0.0", "CVE-9", VulnerabilitySeverity::Info);
    unknown.ecosystem = Some("lang-of-the-future".to_string());

    let report = AuditReport::scanned(3, vec![py, web, unknown]);
    let bom = to_cyclonedx(&report);

    let purls: Vec<&str> = bom.components.iter().map(|c| c.purl.as_str()).collect();
    assert!(purls.contains(&"pkg:pypi/requests@2.19.0"));
    assert!(purls.contains(&"pkg:npm/lodash@4.17.20"));
    assert!(purls.contains(&"pkg:generic/mystery@1.0.0"));
}
