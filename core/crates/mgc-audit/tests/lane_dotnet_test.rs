//! `lane_dotnet_test.rs` — Full 10-test lane for the .NET OSV (NuGet
//! ecosystem) scanner contract (P2 2026-09-10): clean, vulnerable,
//! no-lockfile-unsupported, malformed fail-closed, partial, unicode,
//! large (>1 MB), TFM-dedup, concurrency, audit-entry async contract.
//!
//! Lane 10 test cho scanner .NET (OSV NuGet): clean, vulnerable, thiếu
//! lockfile, malformed fail-closed, partial, unicode, payload lớn,
//! khử trùng TFM, đồng thời, hợp đồng audit async.

#![allow(clippy::unwrap_used)]

use mgc_audit::scanners::{audit_dotnet, read_packages_lock};

/// Real packages.lock.json shape (dotnet restore --locked-mode writer):
/// per-TFM dependency maps with version pins.
/// Shape packages.lock.json thật (writer của dotnet restore): map
/// dependency theo TFM kèm ghim version.
const VULNERABLE: &str = r#"{
  "version": 1,
  "dependencies": {
    "net8.0": {
      "dependencies": {
        "Newtonsoft.Json": {
          "type": "Direct",
          "version": "12.0.2"
        },
        "Serilog": {
          "type": "Transitive",
          "version": "3.1.1"
        }
      }
    }
  }
}"#;

const CLEAN: &str = r#"{
  "version": 1,
  "dependencies": {
    "net8.0": {
      "dependencies": {
        "Serilog": {
          "type": "Direct",
          "version": "3.1.1"
        }
      }
    }
  }
}"#;

#[test]
fn lane_dotnet_1_clean_lock_extracts_pins() {
    let (pins, skipped) = read_packages_lock(CLEAN).unwrap();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].name, "Serilog");
    assert_eq!(pins[0].version, "3.1.1");
    assert!(skipped.is_empty());
}

#[test]
fn lane_dotnet_2_vulnerable_lock_extracts_the_right_pin() {
    // Newtonsoft.Json 12.0.2 → GHSA-5crp-9r3c-p9vr (verified live
    // 2026-09-10); 13.0.1 is clean.
    // Newtonsoft.Json 12.0.2 dính GHSA-5crp (verify sống); 13.0.1 sạch.
    let (pins, _) = read_packages_lock(VULNERABLE).unwrap();
    let json = pins.iter().find(|p| p.name == "Newtonsoft.Json").unwrap();
    assert_eq!(json.version, "12.0.2");
    assert_eq!(json.ecosystem, "NuGet");
}

#[test]
fn lane_dotnet_3_malformed_payload_fails_closed() {
    for bad in ["", "not json", "{", r#"{"dependencies": "nope"}"#] {
        assert!(
            read_packages_lock(bad).is_err(),
            "malformed lock must fail closed: {bad}"
        );
    }
}

#[test]
fn lane_dotnet_4_missing_version_is_partial_not_silent_clean() {
    let partial = r#"{
      "dependencies": {
        "net8.0": {
          "dependencies": {
            "Broken": {
              "type": "Direct"
            }
          }
        }
      }
    }"#;
    let (pins, skipped) = read_packages_lock(partial).unwrap();
    assert!(pins.is_empty());
    assert_eq!(skipped.len(), 1);
    assert!(skipped[0].contains("Broken"));
    assert!(skipped[0].contains("no version pin"));
}

#[test]
fn lane_dotnet_5_empty_lock_is_zero_pins() {
    let (pins, skipped) = read_packages_lock(r#"{"dependencies": {}}"#).unwrap();
    assert!(pins.is_empty());
    assert!(skipped.is_empty());
}

#[test]
fn lane_dotnet_6_unicode_package_names_survive() {
    let unicode = r#"{
      "dependencies": {
        "net8.0": {
          "dependencies": {
            "Crâtes.日本語": {"type": "Direct", "version": "1.0.0"}
          }
        }
      }
    }"#;
    let (pins, _) = read_packages_lock(unicode).unwrap();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].name, "Crâtes.日本語");
}

#[test]
fn lane_dotnet_7_large_lock_over_1mb_parses_all() {
    let total = 25_000;
    let mut entries = String::new();
    for i in 0..total {
        // Build each entry with plain pushes — no format-brace escaping
        // (the JSON object braces conflict with format! syntax).
        // Ghép entry bằng push thuần — không escape brace format (brace
        // object JSON xung đột với cú pháp format!).
        entries.push_str("\"Pkg");
        entries.push_str(&format!("{i:05}"));
        entries.push_str("\": {\"type\": \"Direct\", \"version\": \"1.0.");
        entries.push_str(&format!("{i:04}"));
        entries.push_str("\"}");
        if i + 1 < total {
            entries.push(',');
        }
    }
    let mut large = String::from(r#"{"dependencies": {"net8.0": {"dependencies": {"#);
    large.push_str(&entries);
    // Close: inner deps map, net8.0 entry, dependencies root, root object.
    // Đóng: map deps trong, entry net8.0, dependencies gốc, object gốc.
    large.push_str("}}}}");
    assert!(large.len() > 1_000_000, "fixture must exceed 1 MB");
    let (pins, skipped) = read_packages_lock(&large).unwrap();
    assert_eq!(pins.len(), total);
    assert!(skipped.is_empty());
}

#[test]
fn lane_dotnet_8_tfm_duplicates_are_deduplicated() {
    // The same pin under two target frameworks is ONE audit, not two —
    // packages_audited must reflect real coverage.
    // Cùng ghim ở hai target framework là MỘT audit — số đếm phải đúng
    // độ phủ thật.
    let dup = r#"{
      "dependencies": {
        "net8.0": {
          "dependencies": {
            "Serilog": {"type": "Direct", "version": "3.1.1"},
            "Serilog.Sinks.Console": {"type": "Transitive", "version": "5.0.0"}
          }
        },
        "net8.0-windows": {
          "dependencies": {
            "Serilog": {"type": "Direct", "version": "3.1.1"}
          }
        }
      }
    }"#;
    let (pins, _) = read_packages_lock(dup).unwrap();
    assert_eq!(pins.len(), 2, "Serilog counted once across TFMs");
    assert_eq!(pins.iter().filter(|p| p.name == "Serilog").count(), 1);
}

#[test]
fn lane_dotnet_9_concurrent_reads_stay_independent() {
    let payloads = [VULNERABLE, CLEAN, VULNERABLE, CLEAN, VULNERABLE];
    std::thread::scope(|scope| {
        let handles: Vec<_> = payloads
            .iter()
            .map(|p| {
                let payload = p.to_string();
                scope.spawn(move || read_packages_lock(&payload).unwrap())
            })
            .collect();
        for (i, h) in handles.into_iter().enumerate() {
            let (pins, _) = h.join().unwrap();
            let expected = if i % 2 == 0 { 2 } else { 1 };
            assert_eq!(pins.len(), expected, "concurrent read {i} contaminated");
        }
    });
}

#[tokio::test]
async fn lane_dotnet_10_audit_without_lock_reports_unsupported_with_remediation() {
    // No csproj/lockfile at all → honest unsupported with real
    // remediation text (this path needs NO network).
    // Không có csproj/lockfile → unsupported trung thực kèm hướng dẫn
    // thật (đường này KHÔNG cần mạng).
    let dir = tempfile::tempdir().unwrap();
    let report = audit_dotnet(dir.path()).await.unwrap();
    match report.scanner_status {
        mgc_types::adapter::ScannerStatus::UnsupportedEcosystem { ecosystem } => {
            assert!(
                ecosystem.contains("no .csproj detected"),
                "got: {ecosystem}"
            );
        }
        other => panic!("expected UnsupportedEcosystem, got {other:?}"),
    }
    assert_eq!(report.vulnerability_count, 0);
}
