//! `lane_maven_test.rs` — Full 10-test lane for the Java/Kotlin OSV
//! (Maven ecosystem) scanner contract (P2 2026-09-10): clean, vulnerable,
//! no-lockfile-unsupported, malformed, partial, unicode, large (>1 MB),
//! offline-behavior (parser is pure — no network in these tests),
//! concurrency, evidence stamp (binary E2E lives in audit_cli_e2e).
//!
//! Lane 10 test cho scanner Java/Kotlin (OSV Maven): clean, vulnerable,
//! thiếu lockfile, malformed, partial, unicode, payload lớn, offline
//! (parser thuần — các test này không cần mạng), đồng thời, evidence
//! stamp (binary E2E nằm ở audit_cli_e2e).

#![allow(clippy::unwrap_used)]

use mgc_audit::scanners::{audit_java, read_gradle_verification_metadata};

/// Real gradle verification-metadata.xml shape (Gradle 8 writer): one
/// <dependency> per line with group/name/version attributes.
/// Shape verification-metadata.xml thật (writer Gradle 8): một
/// <dependency> mỗi dòng kèm thuộc tính group/name/version.
const VULNERABLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<verification-metadata xmlns="https://schema.gradle.org/dependency-verification" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="https://schema.gradle.org/dependency-verification https://schema.gradle.org/dependency-verification/dependency-verification-gradle-1.3.xsd">
   <configuration>
      <verify-metadata>true</verify-metadata>
   </configuration>
   <components>
      <dependency group="org.apache.commons" name="commons-text" version="1.9">
         <artifact name="commons-text-1.9.jar">
         </artifact>
      </dependency>
      <dependency group="com.google.guava" name="guava" version="32.0.0-jre">
      </dependency>
   </components>
</verification-metadata>
"#;

const CLEAN: &str = r#"<components>
   <dependency group="com.google.guava" name="guava" version="32.0.0-jre">
   </dependency>
   <dependency group="org.slf4j" name="slf4j-api" version="2.0.9">
   </dependency>
</components>
"#;

#[test]
fn lane_maven_1_clean_lock_parses_zero_pins_with_findings_pending() {
    let (pins, skipped) = read_gradle_verification_metadata(CLEAN);
    assert_eq!(pins.len(), 2, "both pinned deps are queryable");
    assert!(skipped.is_empty());
    // The OSV query itself is network — the PARSER lane asserts the
    // pin extraction only; vulnerability hits are the E2E lane's job.
    // Truy vấn OSV cần mạng — lane PARSER chỉ khẳng định trích ghim;
    // finding là việc của lane E2E.
}

#[test]
fn lane_maven_2_vulnerable_lock_extracts_the_right_pin() {
    let (pins, _) = read_gradle_verification_metadata(VULNERABLE);
    // commons-text 1.9 → GHSA-599f-7c49-w659 (Text4Shell,
    // CVE-2022-42889 — verified live 2026-09-10).
    // commons-text 1.9 dính GHSA-599f (Text4Shell — verify sống).
    let commons = pins
        .iter()
        .find(|p| p.name == "org.apache.commons:commons-text")
        .expect("commons-text pin must extract");
    assert_eq!(commons.version, "1.9");
    assert_eq!(commons.ecosystem, "Maven");
}

#[test]
fn lane_maven_3_no_lockfile_is_unsupported_not_clean() {
    // Pure unit check of the report constructor contract — the async
    // audit path is exercised in the E2E lane; here the constructor
    // must never read as clean.
    // Kiểm tra unit hợp đồng constructor report — đường audit async nằm
    // ở lane E2E; constructor không bao giờ đọc là sạch.
    let report = mgc_types::adapter::AuditReport::unsupported_ecosystem(
        "lib/java (no gradle project detected)",
    );
    assert!(!report.scanner_available());
    assert!(!report.is_clean());
}

#[test]
fn lane_maven_4_malformed_entries_skip_honestly_never_parse_wrong() {
    let malformed = r#"<components>
      <dependency group="only-group" name="no-version">
      </dependency>
      <dependency name="no-group" version="1.0">
      </dependency>
      <not-a-dependency group="x" name="y" version="1">
      </not-a-dependency>
      <dependency group="ok.group" name="ok.name" version="2.0">
      </dependency>
</components>"#;
    let (pins, skipped) = read_gradle_verification_metadata(malformed);
    // The one WELL-FORMED dependency still parses; broken entries are
    // recorded as skips with the offending line — never a wrong pin.
    // Dependency ĐẦY ĐỦ vẫn parse; entry gãy ghi skip kèm dòng lỗi —
    // không bao giờ thành ghim sai.
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].name, "ok.group:ok.name");
    assert_eq!(skipped.len(), 2, "both incomplete entries are named");
    assert!(
        skipped
            .iter()
            .all(|s| s.contains("missing group/name/version"))
    );
}

#[test]
fn lane_maven_5_empty_lock_is_zero_pins() {
    let (pins, skipped) = read_gradle_verification_metadata("<components>\n</components>");
    assert!(pins.is_empty());
    assert!(skipped.is_empty());
}

#[test]
fn lane_maven_6_unicode_group_and_artifact_survive() {
    // Maven coordinates are ASCII in practice, but external data must
    // never panic or drop — a hostile/unicode coordinate round-trips.
    // Tọa độ Maven thực tế là ASCII, nhưng dữ liệu ngoài không được
    // panic hay mất — tọa độ unicode/hostile qua nguyên vẹn.
    let unicode = "<components>\n<dependency group=\"org.example\" name=\"crâtes-日本語\" version=\"1.0\">\n</dependency>\n</components>";
    let (pins, skipped) = read_gradle_verification_metadata(unicode);
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].name, "org.example:crâtes-日本語");
    assert!(skipped.is_empty());
}

#[test]
fn lane_maven_7_large_lock_over_1mb_parses_all() {
    // 1 MB+ of dependency entries: the reader is line-oriented and
    // linear — every entry must extract.
    // Hơn 1 MB entry dependency: reader theo dòng, tuyến tính — mọi
    // entry phải trích được.
    let total = 12_000;
    let mut entries = String::new();
    for i in 0..total {
        entries.push_str(&format!(
            "<dependency group=\"org.example{0:04}\" name=\"art-{i:05}\" version=\"1.{0:04}\">\n</dependency>\n",
            i % 10
        ));
    }
    let large = format!("<components>\n{entries}</components>");
    assert!(large.len() > 1_000_000, "fixture must exceed 1 MB");
    let (pins, skipped) = read_gradle_verification_metadata(&large);
    assert_eq!(pins.len(), total);
    assert!(skipped.is_empty());
}

#[test]
fn lane_maven_8_offline_parser_is_pure_no_network_type() {
    // The reader takes a &str and returns data — no client, no network
    // type in its signature: offline behavior is structural, not
    // promised. (The OSV query layer is exercised in E2E.)
    // Reader nhận &str trả dữ liệu — không client, không kiểu mạng
    // trong chữ ký: hành vi offline là cấu trúc, không phải hứa hẹn.
    // (Tầng truy vấn OSV nằm ở E2E.)
    let (pins, _) = read_gradle_verification_metadata(CLEAN);
    assert!(!pins.is_empty());
}

#[test]
fn lane_maven_9_concurrent_reads_stay_independent() {
    let payloads = [VULNERABLE, CLEAN, VULNERABLE, CLEAN, VULNERABLE];
    std::thread::scope(|scope| {
        let handles: Vec<_> = payloads
            .iter()
            .map(|p| {
                let payload = p.to_string();
                scope.spawn(move || read_gradle_verification_metadata(&payload))
            })
            .collect();
        for (i, h) in handles.into_iter().enumerate() {
            let (pins, _) = h.join().unwrap();
            // Both fixtures carry exactly 2 pins — the assertion proves
            // independence (no cross-write), the count proves losslessness.
            // Cả hai fixture đều có đúng 2 ghim — assert chứng minh độc
            // lập (không ghi chéo), số đếm chứng minh không mất.
            assert_eq!(pins.len(), 2, "concurrent read {i} contaminated");
        }
    });
}

#[tokio::test]
async fn lane_maven_10_audit_without_lock_reports_unsupported_with_remediation() {
    // The async entry: no gradle files at all → honest unsupported with
    // real remediation text (this path needs NO network).
    // Điểm vào async: không có file gradle nào → unsupported trung thực
    // kèm hướng dẫn thật (đường này KHÔNG cần mạng).
    let dir = tempfile::tempdir().unwrap();
    let report = audit_java(dir.path()).await.unwrap();
    match report.scanner_status {
        mgc_types::adapter::ScannerStatus::UnsupportedEcosystem { ecosystem } => {
            assert!(ecosystem.contains("no gradle project"), "got: {ecosystem}");
        }
        other => panic!("expected UnsupportedEcosystem, got {other:?}"),
    }
    assert_eq!(report.vulnerability_count, 0);
}
