//! `lane_dotnet_test.rs` — Full 10-test lane for the .NET OSV (NuGet
//! ecosystem) scanner contract (P2 2026-09-10): clean, vulnerable,
//! no-lockfile-unsupported, malformed fail-closed, partial, unicode,
//! large (>1 MB), TFM-dedup, concurrency, audit-entry async contract.
//!
//! Lane 10 test cho scanner .NET (OSV NuGet): clean, vulnerable, thiếu
//! lockfile, malformed fail-closed, partial, unicode, payload lớn,
//! khử trùng TFM, đồng thời, hợp đồng audit async.

#![allow(clippy::unwrap_used)]
// Edition 2024 makes env::set_var unsafe — OSV override below is
// test-only with save/restore.
// (Set_var unsafe — override có lưu/phục hồi.)
#![allow(unsafe_code)]

use mgc_audit::scanners::{audit_dotnet, read_packages_lock};

/// Real packages.lock.json shape (dotnet restore --locked-mode writer):
/// per-TFM dependency maps with version pins.
/// Shape packages.lock.json thật (writer của dotnet restore): map
/// dependency theo TFM kèm ghim version.
const VULNERABLE: &str = r#"{
  "version": 1,
  "dependencies": {
    "net8.0": {
        "Newtonsoft.Json": {
          "type": "Direct",
          "requested": "[12.0.2, )",
          "resolved": "12.0.2",
          "contentHash": "x="
        },
        "Serilog": {
          "type": "Transitive",
          "resolved": "3.1.1",
          "contentHash": "x="
        }
      }
  }
}"#;

const CLEAN: &str = r#"{
  "version": 1,
  "dependencies": {
    "net8.0": {
        "Serilog": {
          "type": "Direct",
          "requested": "[3.1.1, )",
          "resolved": "3.1.1",
          "contentHash": "x="
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
            "Broken": {
              "type": "Direct"
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
            "Crâtes.日本語": {"type": "Direct", "requested": "[1.0.0, )", "resolved": "1.0.0", "contentHash": "x="}
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
        entries.push_str("\": {\"type\": \"Direct\", \"resolved\": \"1.0.");
        entries.push_str(&format!("{i:04}"));
        entries.push_str("\"}");
        if i + 1 < total {
            entries.push(',');
        }
    }
    let mut large = String::from(r#"{"dependencies": {"net8.0": {"#);
    large.push_str(&entries);
    // Close: net8.0 entry, dependencies root, root object.
    // Đóng: entry net8.0, dependencies gốc, object gốc.
    large.push_str("}}}");
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
            "Serilog": {"type": "Direct", "resolved": "3.1.1"},
            "Serilog.Sinks.Console": {"type": "Transitive", "resolved": "5.0.0"}
        },
        "net8.0-windows": {
            "Serilog": {"type": "Direct", "resolved": "3.1.1"}
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
                ecosystem.contains("no .sln/.csproj detected"),
                "got: {ecosystem}"
            );
        }
        other => panic!("expected UnsupportedEcosystem, got {other:?}"),
    }
    assert_eq!(report.vulnerability_count, 0);
}

/// P0/F4: a .sln lists project paths — nested projects are discovered,
/// non-csproj entries (solution folders, website refs) are ignored.
/// .sln liệt kê project — project lồng nhau được phát hiện, entry
/// không-csproj bị bỏ qua.
#[test]
fn lane_dotnet_8_solution_lists_nested_projects() {
    let sln = "Microsoft Visual Studio Solution File, Format Version 12.00\n\
        Project(\"{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}\") = \"Api\", \"src\\Api\\Api.csproj\", \"{GUID}\"\n\
        Project(\"{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}\") = \"Tests\", \"tests/Tests/Tests.csproj\", \"{GUID2}\"\n\
        Project(\"{2150E333-8FDC-42A3-9474-1A3956D46DE8}\") = \"Solution Items\", \"Solution Items\", \"{GUID3}\"\n\
        Global\nEndGlobal\n";
    let projects = mgc_audit::scanners::read_solution_projects(sln);
    assert_eq!(projects.len(), 2, "only .csproj entries, got {projects:?}");
    assert!(projects.iter().any(|p| p.ends_with("Api.csproj")));
    assert!(projects.iter().any(|p| p.ends_with("Tests.csproj")));
}

/// P0/F4: per-project packages.lock.json files ride the OSV query —
/// a solution WITHOUT a root lock must still reach a real scan (dead
/// endpoint keeps it hermetic).
/// Lock từng project đi query OSV — solution thiếu root lock vẫn phải
/// tới scan thật.
#[test]
fn lane_dotnet_9_solution_project_locks_reach_osv() {
    let dir = std::env::temp_dir().join(format!("mgc-dotnet-sln-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src").join("Api")).unwrap();
    std::fs::write(
        dir.join("app.sln"),
        "Microsoft Visual Studio Solution File, Format Version 12.00\nProject(\"{FAE04EC0}\") = \"Api\", \"src\\Api\\Api.csproj\", \"{G}\"\nEndProject\nGlobal\nEndGlobal\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src").join("Api").join("Api.csproj"),
        "<Project/>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src").join("Api").join("packages.lock.json"),
        r#"{"version":1,"dependencies":{"net8.0":{"Example.Lib":{"type":"Direct","requested":"[1.0, )","resolved":"1.0","contentHash":"x="}}}}"#,
    )
    .unwrap();
    let saved = std::env::var("MGC_OSV_API_BASE").ok();
    unsafe { std::env::set_var("MGC_OSV_API_BASE", "http://127.0.0.1:1/v1") };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let report = rt
        .block_on(mgc_audit::scanners::audit_dotnet(&dir))
        .unwrap();
    match saved {
        Some(v) => unsafe { std::env::set_var("MGC_OSV_API_BASE", v) },
        None => unsafe { std::env::remove_var("MGC_OSV_API_BASE") },
    }
    match &report.scanner_status {
        mgc_types::adapter::ScannerStatus::Failed { scanner, .. } => {
            assert_eq!(scanner, "osv-dev-api")
        }
        other => panic!("solution locks must reach OSV (Failed), got {other:?}"),
    }
}

/// P1-RED: solution entries escaping the project root (`../..`,
/// absolute paths) are REFUSED with a recorded reason — never read
/// outside the project.
/// Entry thoát root bị từ chối có ghi nhận — không đọc ngoài project.
#[test]
fn lane_dotnet_11_solution_escape_refused() {
    let dir = std::env::temp_dir().join(format!("mgc-dotnet-escape-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("app.sln"),
        "Microsoft Visual Studio Solution File, Format Version 12.00\nProject(\"{FAE04EC0}\") = \"Evil\", \"..\\outside\\Evil.csproj\", \"{G}\"\nProject(\"{FAE04EC0}\") = \"Abs\", \"C:\\Windows\\Abs.csproj\", \"{G2}\"\nEndProject\nGlobal\nEndGlobal\n",
    )
    .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let report = rt
        .block_on(mgc_audit::scanners::audit_dotnet(&dir))
        .unwrap();
    match &report.scanner_status {
        mgc_types::adapter::ScannerStatus::Partial { reasons, .. } => assert!(
            reasons.iter().any(|r| r.contains("refused")),
            "escapes must be refused with reasons, got: {reasons:?}"
        ),
        other => panic!("escape-only solution must be Partial, got {other:?}"),
    }
    assert_eq!(report.vulnerability_count, 0);
}

/// P1-RED: EVERY root solution is read — a second .sln is a lane, not
/// litter. Two solutions with one lock each aggregate both pins.
/// Mọi solution ở root đều được đọc — hai sln gộp cả hai lock.
#[test]
fn lane_dotnet_12_second_solution_is_a_lane() {
    let dir = std::env::temp_dir().join(format!("mgc-dotnet-multi-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for proj in ["a", "b"] {
        std::fs::create_dir_all(dir.join(proj)).unwrap();
        std::fs::write(
            dir.join(proj).join(format!("{proj}.csproj")),
            "<Project/>\n",
        )
        .unwrap();
        std::fs::write(
            dir.join(proj).join("packages.lock.json"),
            format!(
                "{{\"version\":1,\"dependencies\":{{\"net8.0\":{{\"{proj}.Lib\":{{\"type\":\"Direct\",\"resolved\":\"1.0\",\"contentHash\":\"x=\"}}}}}}}}"
            ),
        )
        .unwrap();
    }
    std::fs::write(
        dir.join("one.sln"),
        "Microsoft Visual Studio Solution File, Format Version 12.00\nProject(\"{F}\") = \"A\", \"a\\A.csproj\", \"{G}\"\nEndProject\nGlobal\nEndGlobal\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("two.sln"),
        "Microsoft Visual Studio Solution File, Format Version 12.00\nProject(\"{F}\") = \"B\", \"b\\B.csproj\", \"{G}\"\nEndProject\nGlobal\nEndGlobal\n",
    )
    .unwrap();
    // Hermetic: collect pins directly (no network) — both solutions
    // must contribute.
    // Hermetic: gom ghim trực tiếp — cả hai solution đều phải góp.
    let (pins, _, recognized) = mgc_audit::scanners::collect_dotnet_pins(&dir).unwrap();
    assert!(recognized, "two solutions = recognized");
    assert_eq!(pins.len(), 2, "both solutions' locks must yield pins");
    assert!(pins.iter().any(|p| p.name == "a.Lib"));
    assert!(pins.iter().any(|p| p.name == "b.Lib"));
}
