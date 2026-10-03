//! `audit_lane_test.rs` — Full 10-test lane for the npm Bulk Advisory
//! flow (Tech Lead P2 2026-09-10): clean, vulnerable, malformed
//! response, hostile-package response, semver range edge, scoped +
//! unicode packages, large response, multiple versions per package,
//! concurrency, evidence stamp. Binary E2E lives in
//! cli/tests/audit_cli_e2e.rs.
//!
//! Lane 10 test đầy đủ cho luồng npm Bulk Advisory: clean, vulnerable,
//! response malformed, response đỉnh package lạ, biên semver range,
//! package scope + unicode, response lớn, nhiều version mỗi package,
//! đồng thời, evidence stamp. Binary E2E nằm ở cli/tests/audit_cli_e2e.

#![allow(clippy::unwrap_used)]
#![allow(unsafe_code)]

use crate::audit::parse_advisory_bulk_response;

fn pins(entries: &[(&str, &str)]) -> Vec<(String, String)> {
    entries
        .iter()
        .map(|(n, v)| (n.to_string(), v.to_string()))
        .collect()
}

fn advisory(id: u64, vulnerable: &str, severity: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "url": "https://github.com/advisories/GHSA-test",
        "title": "Test advisory",
        "severity": severity,
        "vulnerable_versions": vulnerable,
    })
}

#[test]
fn lane_npm_1_clean_response_zero_findings() {
    // No advisories returned → clean (the packages were asked, the
    // registry answered none apply).
    // Không advisory trả về → sạch (package đã hỏi, registry trả lời
    // không cái nào áp dụng).
    let payload = serde_json::json!({});
    let vulns = parse_advisory_bulk_response(&payload, &pins(&[("lodash", "4.17.21")])).unwrap();
    assert!(vulns.is_empty());
}

#[test]
fn lane_npm_2_vulnerable_version_matches_range() {
    let payload = serde_json::json!({"lodash": [advisory(1102260, "<4.17.21", "high")]});
    let vulns = parse_advisory_bulk_response(&payload, &pins(&[("lodash", "4.17.12")])).unwrap();
    assert_eq!(vulns.len(), 1);
    assert_eq!(vulns[0].package.version().to_string(), "4.17.12");
    assert_eq!(vulns[0].cve, "1102260");
    assert_eq!(vulns[0].severity, "high");
    assert_eq!(vulns[0].scanner.as_deref(), Some("npm-bulk-advisory"));
    assert_eq!(vulns[0].ecosystem.as_deref(), Some("web/javascript"));
    assert!(vulns[0].evidence_at.is_some());
}

#[test]
fn lane_npm_3_patched_version_outside_range_no_finding() {
    let payload = serde_json::json!({"lodash": [advisory(1102260, "<4.17.21", "high")]});
    let vulns = parse_advisory_bulk_response(&payload, &pins(&[("lodash", "4.17.21")])).unwrap();
    assert!(vulns.is_empty(), "patched version must not match");
}

#[test]
fn lane_npm_4_malformed_response_fails_closed() {
    // Wrong shape, malformed advisory records, unparseable ranges — every
    // one is an error, never an implicit clean.
    // Shape sai, bản ghi advisory rác, range không parse được — tất cả
    // là lỗi, không bao giờ tự biến thành sạch.
    let p = &pins(&[("lodash", "4.17.12")]);
    for bad in [
        serde_json::json!([]),
        serde_json::json!("nope"),
        serde_json::Value::Null,
        serde_json::json!({"lodash": "not-array"}),
        serde_json::json!({"lodash": [serde_json::json!({"id": "string-not-number"})]}),
        serde_json::json!({"lodash": [serde_json::json!({"id": 1, "url": "u", "title": "t", "severity": "s", "vulnerable_versions": "garbage-range"})]}),
    ] {
        assert!(
            parse_advisory_bulk_response(&bad, p).is_err(),
            "malformed response must fail closed: {bad}"
        );
    }
}

#[test]
fn lane_npm_5_hostile_package_in_response_is_rejected() {
    // The response mentions a package we never requested — possible
    // hostile injection; hard error.
    // Response nhắc package ta chưa từng hỏi — có thể tiêm thù địch;
    // lỗi cứng.
    let payload = serde_json::json!({"evil-pkg": [advisory(1, "<99.0.0", "critical")]});
    let result = parse_advisory_bulk_response(&payload, &pins(&[("lodash", "4.17.12")]));
    assert!(result.is_err());
}

#[test]
fn lane_npm_6_semver_range_edges() {
    let p = &pins(&[("semver-edge", "2.0.0")]);
    // Exact-match style ranges from the real API.
    // Range kiểu khớp chính xác từ API thật.
    let exact = serde_json::json!({"semver-edge": [advisory(1, "=2.0.0", "medium")]});
    assert_eq!(
        parse_advisory_bulk_response(&exact, p).unwrap().len(),
        1,
        "=2.0.0 must match 2.0.0"
    );

    let above = serde_json::json!({"semver-edge": [advisory(2, "<2.0.0", "low")]});
    assert!(
        parse_advisory_bulk_response(&above, p).unwrap().is_empty(),
        "<2.0.0 must not match 2.0.0"
    );
}

#[test]
fn lane_npm_7_scoped_and_unicode_packages() {
    // @scope names and unicode titles ride through without mojibake.
    // Tên @scope và title unicode qua nguyên vẹn không vỡ chữ.
    let payload = serde_json::json!({
        "@babel/core": [serde_json::json!({
            "id": 7, "url": "https://x", "title": "RCE trong 📦 @babel/core — lỗi unicode",
            "severity": "critical", "vulnerable_versions": "<7.24.0"
        })]
    });
    let vulns =
        parse_advisory_bulk_response(&payload, &pins(&[("@babel/core", "7.23.9")])).unwrap();
    assert_eq!(vulns.len(), 1);
    assert_eq!(vulns[0].package.name().as_str(), "@babel/core");
    assert!(vulns[0].title.contains("📦"));
}

#[test]
fn lane_npm_8_large_response_over_1mb() {
    let total = 12_000;
    let mut map = serde_json::Map::new();
    for i in 0..total {
        map.insert(
            format!("pkg-{i:05}"),
            serde_json::Value::Array(vec![advisory(i as u64 + 1, "<1.0.0", "moderate")]),
        );
    }
    let payload = serde_json::Value::Object(map);
    let mut pin_entries = Vec::with_capacity(total);
    for i in 0..total {
        pin_entries.push((format!("pkg-{i:05}"), "0.9.0".to_string()));
    }
    let vulns = parse_advisory_bulk_response(&payload, &pin_entries).unwrap();
    assert_eq!(vulns.len(), total, "every advisory row must survive");
}

#[test]
fn lane_npm_9_multiple_installed_versions_per_package() {
    // Two pinned versions of the SAME package: one inside the range, one
    // outside → exactly one finding for the vulnerable pin.
    // Hai version ghim của CÙNG package: một trong range, một ngoài →
    // đúng một finding cho ghim dính lỗi.
    let payload = serde_json::json!({"lodash": [advisory(1102260, "<4.17.21", "high")]});
    let vulns = parse_advisory_bulk_response(
        &payload,
        &pins(&[("lodash", "4.17.12"), ("lodash", "4.17.21")]),
    )
    .unwrap();
    assert_eq!(vulns.len(), 1);
    assert_eq!(vulns[0].package.version().to_string(), "4.17.12");
}

#[test]
fn lane_npm_bulk_request_keeps_all_versions_for_each_package() {
    // The registry request must include every installed version of a name.
    // Request gửi registry phải giữ đủ mọi version đang cài của cùng tên.
    let body = crate::audit::advisory_bulk_request_body(&pins(&[
        ("lodash", "4.17.21"),
        ("react", "18.2.0"),
        ("lodash", "4.17.12"),
        ("lodash", "4.17.12"),
    ]));
    assert_eq!(
        serde_json::Value::Object(body),
        serde_json::json!({
            "lodash": ["4.17.12", "4.17.21"],
            "react": ["18.2.0"]
        })
    );
}

#[test]
fn lane_npm_10_concurrent_parses_no_cross_contamination() {
    let vulnerable = serde_json::json!({"lodash": [advisory(1102260, "<4.17.21", "high")]});
    let clean = serde_json::json!({});
    let payloads = [
        vulnerable.clone(),
        clean.clone(),
        vulnerable.clone(),
        clean.clone(),
    ];
    let pin_sets: Vec<Vec<(String, String)>> = payloads
        .iter()
        .map(|_| pins(&[("lodash", "4.17.12")]))
        .collect();
    std::thread::scope(|scope| {
        let handles: Vec<_> = payloads
            .iter()
            .zip(&pin_sets)
            .map(|(p, pins)| {
                let payload = p.clone();
                let pins = pins.clone();
                scope.spawn(move || parse_advisory_bulk_response(&payload, &pins).unwrap())
            })
            .collect();
        for (i, h) in handles.into_iter().enumerate() {
            let expected = if i % 2 == 0 { 1 } else { 0 };
            assert_eq!(
                h.join().unwrap().len(),
                expected,
                "concurrent parse {i} cross-contaminated"
            );
        }
    });
}

// ------------------------------------------------ P0-3 (2026-09-10): mgc.lock là nguồn audit duy nhất

/// Block on the async audit fn without a tokio test-macro dependency —
/// the adapter crate deliberately keeps test deps light.
/// Chạy blocking audit fn async không cần tokio test-macro.
fn run_audit_blocking(dir: &std::path::Path) -> Result<mgc_types::adapter::AuditReport, String> {
    // These contract checks fire BEFORE any network await (fail-closed
    // remediation / clean-zero), so a minimal current-thread runtime
    // suffices for driving the future to completion.
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    rt.block_on(crate::audit::run_audit(dir, "https://registry.npmjs.org"))
        .map_err(|e| e.to_string())
}

/// Write a v4 fixture with its real payload digest.
/// Ghi fixture v4 với digest payload được tính thật.
fn write_v4_fixture(dir: &std::path::Path, text: &str) {
    let mut doc = mgc_lockfile::canonical::parse_v4_document(text).unwrap();
    doc.metadata.lockfile_hash = mgc_lockfile::canonical::payload_digest(&doc.payload());
    let serialized = mgc_lockfile::canonical::write_v4_document(&doc).unwrap();
    std::fs::write(dir.join("mgc.lock"), serialized).unwrap();
}

#[test]
fn p0_3_rival_lockfile_without_mgc_lock_fails_closed_with_import_remediation() {
    // CONTRACT: bun.lock/deno.lock tồn tại mà mgc.lock vắng → run_audit
    // FAIL kèm remediation `mgc import` — không fallback audit lockfile
    // đối thủ (đường vận hành chỉ tiêu thụ mgc.lock).
    // (Rival lockfile without mgc.lock → fail with the import remediation;
    // the operational audit lane consumes mgc.lock ONLY.)
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("bun.lock"),
        r#"{"packages": {"lodash@4.17.20": ["lodash@4.17.20", "", {}, "sha512-x"]}}"#,
    )
    .unwrap();

    let err = run_audit_blocking(dir.path()).unwrap_err();
    assert!(
        err.contains("mgc.lock is missing") && err.contains("mgc import"),
        "audit must fail-closed with the import remediation, got: {err}"
    );
}

#[test]
fn p0_3_deno_lock_without_mgc_lock_fails_closed_with_import_remediation() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("deno.lock"),
        r#"{"version": "5", "npm": {"lodash@4.17.20": {"integrity": "sha512-x"}}}"#,
    )
    .unwrap();

    let err = run_audit_blocking(dir.path()).unwrap_err();
    assert!(
        err.contains("mgc.lock is missing") && err.contains("mgc import deno"),
        "deno.lock remediation must name `mgc import deno`, got: {err}"
    );
}

#[test]
fn p0_3_clean_project_without_any_lockfile_reports_clean_zero() {
    // Không lockfile nào → clean 0 (không phải lỗi — không có gì để audit).
    let dir = tempfile::tempdir().unwrap();
    let report = run_audit_blocking(dir.path()).unwrap();
    assert_eq!(report.packages_audited, 0);
    assert_eq!(report.vulnerability_count, 0);
}

#[test]
fn v4_pins_come_from_reachable_web_instances_with_jsr_split() {
    // v4 replay: chỉ instance Web tới được từ root vào pins; orphan,
    // non-Web và tên jsr: đi đúng lane của chúng.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mgc.toml"),
        "name = \"demo\"\necosystem = \"web\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join(".mgc.core"), "web\n").unwrap();
    let pin = |ecosystem: &str, name: &str, version: &str| {
        format!(
            "[[package]]\n[package.key]\necosystem = \"{ecosystem}\"\nname = \"{name}\"\nversion = \"{version}\"\nsource_id = \"npm\"\n"
        )
    };
    let edge = |name: &str, version: &str| {
        format!(
            "[[package.edges]]\ntarget_key = {{ ecosystem = \"web\", name = \"{name}\", version = \"{version}\", source_id = \"npm\" }}\nrange = \"*\"\nkind = \"normal\"\norigin = \"Manifest\"\n"
        )
    };
    let mut doc = String::from(
        "version = \"4\"\nroot_dependencies = [\"app@1.0.0\"]\n\n[metadata]\ngenerated_at = \"2026-10-01T00:00:00Z\"\ngenerator = \"mgc/test\"\nlockfile_hash = \"\"\n\n",
    );
    doc.push_str(&pin("web", "app", "1.0.0"));
    doc.push_str(&edge("leftpad", "1.3.0"));
    doc.push_str(&edge("jsr:@scope/pkg", "1.0.0"));
    doc.push_str(&pin("web", "leftpad", "1.3.0"));
    doc.push_str(&pin("web", "jsr:@scope/pkg", "1.0.0"));
    doc.push_str(&pin("web", "orphan", "9.9.9"));
    doc.push_str(&pin("python", "six", "1.17.0"));
    write_v4_fixture(dir.path(), &doc);
    let (pins, jsr_pins) = crate::audit::v4_pins_for_audit(dir.path())
        .unwrap()
        .expect("v4 lock must take the v4 pin path");
    let names: Vec<&str> = pins.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.contains(&"app"), "root must audit: {names:?}");
    assert!(
        names.contains(&"leftpad"),
        "reachable must audit: {names:?}"
    );
    assert!(
        !names.contains(&"orphan"),
        "orphan must not audit: {names:?}"
    );
    assert!(!names.contains(&"six"), "non-web must not audit: {names:?}");
    assert!(
        !names.iter().any(|name| name.starts_with("jsr:")),
        "{names:?}"
    );
    assert_eq!(jsr_pins, vec!["jsr:@scope/pkg".to_string()]);
}

#[test]
fn v4_audit_derives_roots_from_manifest_when_lock_roots_are_empty() {
    // Web v4 intentionally omits root pins; the manifest supplies anchors.
    // Web v4 cố ý không ghi root pin; manifest cung cấp các điểm neo.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name":"demo","version":"1.0.0","dependencies":{"app":"1.0.0"}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("mgc.toml"),
        "name = \"demo\"\necosystem = \"web\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join(".mgc.core"), "web\n").unwrap();
    let mut doc = String::from(
        "version = \"4\"\nroot_dependencies = []\n\n[metadata]\ngenerated_at = \"2026-10-01T00:00:00Z\"\ngenerator = \"mgc/test\"\nlockfile_hash = \"\"\n\n",
    );
    doc.push_str("[[package]]\n[package.key]\necosystem = \"web\"\nname = \"app\"\nversion = \"1.0.0\"\nsource_id = \"npm\"\n\n");
    doc.push_str("[[package.edges]]\ntarget_key = { ecosystem = \"web\", name = \"leftpad\", version = \"1.3.0\", source_id = \"npm\" }\nrange = \"*\"\nkind = \"normal\"\norigin = \"Manifest\"\n\n");
    doc.push_str("[[package]]\n[package.key]\necosystem = \"web\"\nname = \"leftpad\"\nversion = \"1.3.0\"\nsource_id = \"npm\"\n\n");
    doc.push_str("[[package]]\n[package.key]\necosystem = \"web\"\nname = \"orphan\"\nversion = \"9.9.9\"\nsource_id = \"npm\"\n");
    write_v4_fixture(dir.path(), &doc);

    let (pins, jsr_pins) = crate::audit::v4_pins_for_audit(dir.path())
        .unwrap()
        .expect("v4 lock must take the v4 pin path");
    let names: Vec<&str> = pins.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, vec!["app", "leftpad"]);
    assert!(jsr_pins.is_empty());
}

#[test]
fn v4_audit_rejects_payload_tampering_and_unresolved_full_edge_keys() {
    // A valid digest is required before audit; edge identity includes the
    // complete source/variant key, not only name and version.
    // Digest hợp lệ là điều kiện bắt buộc; cạnh dùng đủ khóa source/variant.
    let tampered = tempfile::tempdir().unwrap();
    let valid = "version = \"4\"\n\n[metadata]\ngenerated_at = \"2026-10-01T00:00:00Z\"\ngenerator = \"mgc/test\"\nlockfile_hash = \"\"\n";
    write_v4_fixture(tampered.path(), valid);
    let lock_path = tampered.path().join("mgc.lock");
    let contents = std::fs::read_to_string(&lock_path).unwrap();
    std::fs::write(
        &lock_path,
        contents.replace("generator = \"mgc/test\"", "generator = \"mgc/forged\""),
    )
    .unwrap();
    let err = crate::audit::v4_pins_for_audit(tampered.path()).unwrap_err();
    assert!(err.to_string().contains("payload digest"), "{err}");

    let broken_edge = tempfile::tempdir().unwrap();
    std::fs::write(
        broken_edge.path().join("mgc.toml"),
        "name = \"demo\"\necosystem = \"web\"\n",
    )
    .unwrap();
    std::fs::write(broken_edge.path().join(".mgc.core"), "web\n").unwrap();
    let mut doc = String::from(
        "version = \"4\"\nroot_dependencies = [\"app@1.0.0\"]\n\n[metadata]\ngenerated_at = \"2026-10-01T00:00:00Z\"\ngenerator = \"mgc/test\"\nlockfile_hash = \"\"\n\n",
    );
    doc.push_str("[[package]]\n[package.key]\necosystem = \"web\"\nname = \"app\"\nversion = \"1.0.0\"\nsource_id = \"npm\"\n");
    doc.push_str("\n[[package.edges]]\ntarget_key = { ecosystem = \"web\", name = \"leftpad\", version = \"1.3.0\", source_id = \"mirror\" }\nrange = \"*\"\nkind = \"normal\"\norigin = \"Manifest\"\n\n");
    doc.push_str("[[package]]\n[package.key]\necosystem = \"web\"\nname = \"leftpad\"\nversion = \"1.3.0\"\nsource_id = \"npm\"\n");
    write_v4_fixture(broken_edge.path(), &doc);
    let err = crate::audit::v4_pins_for_audit(broken_edge.path()).unwrap_err();
    assert!(err.to_string().contains("edge target"), "{err}");
}

#[test]
fn v4_pins_returns_none_for_legacy_locks() {
    // Lock legacy → None để đường legacy cũ sở hữu (không tranh).
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("mgc.lock"), "version = \"3\"\n").unwrap();
    assert!(
        crate::audit::v4_pins_for_audit(dir.path())
            .unwrap()
            .is_none()
    );
    let empty = tempfile::tempdir().unwrap();
    assert!(
        crate::audit::v4_pins_for_audit(empty.path())
            .unwrap()
            .is_none()
    );
}

#[test]
fn insecure_loopback_requires_explicit_opt_in() {
    // Default deny: loopback http is rejected unless the operator opts in.
    // Serialized: env is process-global.
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    let _guard = LOCK
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap();
    let old = std::env::var_os("MAGICORE_WEB_ALLOW_INSECURE_LOCALHOST");
    let restore = |previous: Option<std::ffi::OsString>| {
        if let Some(value) = previous {
            unsafe { std::env::set_var("MAGICORE_WEB_ALLOW_INSECURE_LOCALHOST", value) };
        } else {
            unsafe { std::env::remove_var("MAGICORE_WEB_ALLOW_INSECURE_LOCALHOST") };
        }
    };
    unsafe { std::env::remove_var("MAGICORE_WEB_ALLOW_INSECURE_LOCALHOST") };
    assert!(!crate::audit::allow_insecure_loopback_url(
        "http://127.0.0.1:4315/x"
    ));
    assert!(!crate::audit::allow_insecure_loopback_url(
        "https://registry.npmjs.org/x"
    ));
    unsafe { std::env::set_var("MAGICORE_WEB_ALLOW_INSECURE_LOCALHOST", "1") };
    assert!(crate::audit::allow_insecure_loopback_url(
        "http://127.0.0.1:4315/x"
    ));
    assert!(!crate::audit::allow_insecure_loopback_url(
        "http://example.com/x"
    ));
    restore(old);
}
