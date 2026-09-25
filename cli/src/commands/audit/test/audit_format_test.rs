//! `audit_format_test.rs` — OutputFormat parsing contract (P1-B).
//! Table/json/sarif/cyclonedx accepted; anything else is a HARD error —
//! CI must never ingest a silent fallback format.
//! Test hợp đồng parse OutputFormat (P1-B): nhận table/json/sarif/
//! cyclonedx; giá trị khác là lỗi CỨNG — CI không bao giờ ingest format
/// rơi về mặc định một cách âm thầm.
use crate::commands::audit::OutputFormat;

#[test]
fn parse_table_default_and_explicit() {
    assert!(matches!(
        OutputFormat::from_flag(None),
        Ok(OutputFormat::Table)
    ));
    assert!(matches!(
        OutputFormat::from_flag(Some("table")),
        Ok(OutputFormat::Table)
    ));
    assert!(matches!(
        OutputFormat::from_flag(Some("json")),
        Ok(OutputFormat::Json)
    ));
    assert!(matches!(
        OutputFormat::from_flag(Some("sarif")),
        Ok(OutputFormat::Sarif)
    ));
    assert!(matches!(
        OutputFormat::from_flag(Some("cyclonedx")),
        Ok(OutputFormat::CycloneDx)
    ));
}

#[test]
fn parse_unknown_format_is_hard_error() {
    // Unknown values must NEVER fall back to table — a silent fallback
    // would hand CI a human table where it expects machine JSON.
    // Giá trị lạ KHÔNG BAO GIỜ rơi về table — rơi về mặc định sẽ đưa CI
    // bảng hiển thị người thay vì JSON máy.
    for bad in ["xml", "SARIF", "", "text", "cyclone"] {
        assert!(
            OutputFormat::from_flag(Some(bad)).is_err(),
            "'{bad}' must be rejected, not silently defaulted"
        );
    }
}

#[test]
fn machine_formats_flagged_not_table() {
    for fmt in [
        OutputFormat::Json,
        OutputFormat::Sarif,
        OutputFormat::CycloneDx,
    ] {
        assert!(fmt.is_machine(), "{fmt:?} must be machine-only payload");
    }
    assert!(!OutputFormat::Table.is_machine());
}

#[test]
fn audit_fix_machine_output_defers_only_when_mutation_will_run() {
    use crate::commands::audit::OutputFormat;
    use crate::commands::audit::web::should_print_initial_report;

    assert!(!should_print_initial_report(
        true,
        OutputFormat::Json,
        true,
        1
    ));
    assert!(should_print_initial_report(
        true,
        OutputFormat::Json,
        true,
        0
    ));
    assert!(should_print_initial_report(
        true,
        OutputFormat::Json,
        false,
        1
    ));
    assert!(should_print_initial_report(
        false,
        OutputFormat::Json,
        true,
        1
    ));
    assert!(should_print_initial_report(
        true,
        OutputFormat::Table,
        true,
        1
    ));
}

/// R4/F2: non-CVE rows must be labeled by what they are — a policy
/// finding printed as "CVE:" would mislead triage.
/// Dòng phi-CVE phải ghi nhãn đúng bản chất — finding policy mà in
/// "CVE:" sẽ gây hiểu lầm.
#[test]
fn finding_table_labels_never_masquerade_as_cve() {
    use crate::commands::audit::finding_table_labels;
    use mgc_types::adapter::FindingClass;
    assert_eq!(
        finding_table_labels(FindingClass::Vulnerability),
        ("", "CVE")
    );
    assert_eq!(
        finding_table_labels(FindingClass::Policy),
        (" [policy]", "Policy rule")
    );
    assert_eq!(
        finding_table_labels(FindingClass::Provenance),
        (" [provenance]", "Provenance check")
    );
    assert_eq!(
        finding_table_labels(FindingClass::Artifact),
        (" [artifact]", "Artifact check")
    );
}
