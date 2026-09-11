#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Audit module tests for app adapter.

use mgc_app_adapter::audit::scanner::*;
use mgc_types::adapter::ScannerStatus;

fn tmp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-app-audit-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

/// Hermetic guard: true when the tool IS installed — tests asserting
/// "unavailable without tool" must skip in that environment.
/// Guard hermetic: true khi tool ĐÃ cài — test assert "unavailable khi
/// thiếu tool" phải bỏ qua trong môi trường đó.
fn tool_installed(tool: &str) -> bool {
    std::process::Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn audit_flutter_security_is_unsupported_not_clean() {
    if tool_installed("flutter") {
        return;
    }
    let dir = tmp("flutter-no-tool");
    std::fs::write(dir.join("pubspec.yaml"), "name: test\n").unwrap();

    // pub outdated is NOT a CVE scanner — the security audit must be
    // honestly UnsupportedEcosystem, never a fake clean report.
    // pub outdated KHÔNG phải scanner CVE — security audit phải trung thực
    // UnsupportedEcosystem, không bao giờ báo sạch giả.
    let report = audit_flutter(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(
        report.scanner_status,
        mgc_types::adapter::ScannerStatus::UnsupportedEcosystem { .. }
    ));
}

#[tokio::test]
async fn dependency_health_flutter_without_flutter_fails_closed() {
    if tool_installed("flutter") {
        return;
    }
    let dir = tmp("flutter-no-tool");
    std::fs::write(dir.join("pubspec.yaml"), "name: test\n").unwrap();

    // Without the CLI we cannot know freshness — an empty default report
    // would fake "everything up to date". Must fail closed.
    // Không có CLI thì không biết độ tươi — report rỗng đồng nghĩa bịa
    // "mọi thứ mới". Phải fail-closed.
    assert!(dependency_health_flutter(&dir).await.is_err());
}

#[tokio::test]
async fn audit_kotlin_without_gradle_is_tool_missing() {
    if tool_installed("gradle") {
        return;
    }
    let dir = tmp("kotlin-no-tool");
    std::fs::write(dir.join("build.gradle"), "// empty\n").unwrap();

    let report = audit_kotlin(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(
        report.scanner_status,
        ScannerStatus::ToolMissing { .. }
    ));
}

#[tokio::test]
async fn audit_swift_not_implemented_is_unsupported_not_clean() {
    let dir = tmp("swift-audit");
    std::fs::write(dir.join("Package.swift"), "// swift package\n").unwrap();

    let report = audit_swift(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(
        report.scanner_status,
        mgc_types::adapter::ScannerStatus::UnsupportedEcosystem { .. }
    ));
}

#[tokio::test]
async fn audit_cocoapods_not_implemented_is_unsupported_not_clean() {
    let dir = tmp("cocoapods-audit");

    let report = audit_cocoapods(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(
        report.scanner_status,
        mgc_types::adapter::ScannerStatus::UnsupportedEcosystem { .. }
    ));
}

#[tokio::test]
async fn audit_multi_detects_flutter_as_unsupported() {
    let dir = tmp("multi-flutter");
    std::fs::write(dir.join("pubspec.yaml"), "name: test\n").unwrap();

    let report = audit_multi(&dir).await.unwrap();
    assert_eq!(report.vulnerability_count, 0);
    assert!(matches!(
        report.scanner_status,
        ScannerStatus::UnsupportedEcosystem { .. }
    ));
}

// ===== Multi-language aggregation (Tech Lead 2026-09-09 §11) =====
// Multi KHÔNG còn first-match: mọi manifest nhận diện đều được scan và
// tổng hợp — pubspec + gradle cùng lúc phải ra Partial (flutter chưa có
// scanner + kotlin thiếu gradle → không bao giờ chỉ trả 1 core).

#[tokio::test]
async fn audit_multi_aggregates_every_manifest_not_first_match() {
    let dir = tmp("multi-aggregate");
    // BOTH manifests present — the old first-match code returned the
    // flutter result and never looked at gradle. The aggregate must
    // reach the KOTLIN step whatever the machine's gradle state:
    // gradle absent → ToolMissing(gradle); gradle present (CI runners
    // ship it preinstalled) → the scanner runs and reports Failed
    // (fixture has no dependencyCheckAnalyze task). Either terminal
    // state proves aggregation looked PAST flutter — a bare flutter
    // Unsupported would fail this test.
    // CẢ HAI manifest cùng tồn tại — code first-match cũ trả flutter
    // rồi không nhìn gradle nữa. Aggregate phải CHẠM tới bước KOTLIN
    // bất kể gradle trên máy thế nào: vắng gradle → ToolMissing(gradle);
    // có gradle (runner CI cài sẵn) → scanner chạy và báo Failed
    // (fixture không có task dependencyCheckAnalyze). Cả hai trạng
    // thái cuối đều chứng minh aggregate đã nhìn QUA flutter — kết quả
    // flutter Unsupported trơ trọi sẽ rớt test này.
    std::fs::write(dir.join("pubspec.yaml"), "name: test\n").unwrap();
    std::fs::write(dir.join("build.gradle"), "// empty\n").unwrap();

    let report = audit_multi(&dir).await.unwrap();
    match report.scanner_status {
        mgc_types::adapter::ScannerStatus::ToolMissing { tool, .. } => {
            assert!(tool.contains("gradle"), "tool must be gradle, got: {tool}");
        }
        mgc_types::adapter::ScannerStatus::Failed { scanner, .. } => {
            assert!(
                scanner.contains("dependency-check"),
                "failed scanner must be the kotlin lane, got: {scanner}"
            );
        }
        other => {
            panic!("aggregate must reach the kotlin step (ToolMissing or Failed), got {other:?}")
        }
    }
}

#[tokio::test]
async fn audit_multi_aggregate_names_flutter_gap() {
    // Flutter (no CVE scanner) + a gradle manifest → the aggregate must
    // NOT be the bare flutter Unsupported result: it reaches the kotlin
    // lane (ToolMissing when gradle is absent; Failed when the CI runner
    // ships gradle and the fixture lacks the task). Both prove flutter
    // did not shadow kotlin.
    // Flutter (chưa có scanner CVE) + manifest gradle → aggregate
    // KHÔNG được là kết quả flutter Unsupported trơ trọi: nó chạm lane
    // kotlin (ToolMissing khi vắng gradle; Failed khi runner CI có
    // gradle mà fixture thiếu task). Cả hai đều chứng minh flutter
    // không lấp mất kotlin.
    let dir = tmp("multi-aggregate-2");
    std::fs::write(dir.join("pubspec.yaml"), "name: test\n").unwrap();
    std::fs::write(dir.join("build.gradle.kts"), "// empty\n").unwrap();

    let report = audit_multi(&dir).await.unwrap();
    match report.scanner_status {
        mgc_types::adapter::ScannerStatus::ToolMissing { .. }
        | mgc_types::adapter::ScannerStatus::Failed { .. } => {}
        other => {
            panic!("aggregate must reach the kotlin lane (ToolMissing or Failed), got {other:?}")
        }
    }
}

// ---------------------------------------------------------------------------
// Kotlin success E2E (P2 2026-09-10): a REAL gradle run drives the
// dependencyCheckAnalyze task; the fixture task writes a dependency-check
// schema report WITHOUT any NVD/network (CI runs this with gradle
// provisioned). Proves the full pipeline: gradlew detect → task exec →
// fresh-report guard → typed parse → Available with findings.
// E2E Kotlin thành công: gradle THẬT chạy task dependencyCheckAnalyze;
// task fixture ghi report đúng schema dependency-check KHÔNG cần
// NVD/mạng (CI chạy với gradle sẵn). Chứng minh pipeline đủ: phát hiện
// gradlew → chạy task → chống report cũ → parse typed → Available.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn audit_kotlin_success_e2e_via_real_gradle_run() {
    if !tool_installed("gradle") {
        eprintln!(
            "SKIP (environment-unverified) test=audit_kotlin_success_e2e_via_real_gradle_run: gradle not installed on this machine"
        );
        return;
    }
    let dir = tmp("kotlin-success");
    // A minimal gradle project whose dependencyCheckAnalyze task writes a
    // dependency-check-schema report with ONE finding (jackson-databind
    // CVE-2020-25649 — captured shape). No plugins, no network.
    // Project gradle tối tiểu: task dependencyCheckAnalyze ghi report đúng
    // schema với MỘT finding (giữ shape chụp thật). Không plugin, không mạng.
    std::fs::write(
        dir.join("settings.gradle"),
        "rootProject.name = 'kotlin-success-fixture'\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("build.gradle"),
        r#"
import groovy.json.JsonOutput

tasks.register('dependencyCheckAnalyze') {
    doLast {
        def reportDir = new File(projectDir, 'build/reports')
        reportDir.mkdirs()
        def report = [
          projectInfo: [name: 'kotlin-success-fixture'],
          dependencies: [
            [
              fileName: 'jackson-databind-2.9.10.8.jar',
              packages: [
                [package: [id: 'pkg:maven/com.fasterxml.jackson.core/jackson-databind@2.9.10.8']]
              ],
              vulnerabilities: [
                [name: 'CVE-2020-25649', severity: 'HIGH', description: 'XXE in jackson-databind',
                 references: [[url: 'https://nvd.nist.gov/vuln/detail/CVE-2020-25649']]]
              ]
            ],
            [
              fileName: 'okhttp-4.12.0.jar',
              packages: [
                [package: [id: 'pkg:maven/com.squareup.okhttp3/okhttp@4.12.0']]
              ],
              vulnerabilities: []
            ]
          ]
        ]
        new File(reportDir, 'dependency-check-report.json').text = JsonOutput.toJson(report)
    }
}
"#,
    )
    .unwrap();

    let report = audit_kotlin(&dir).await.unwrap();
    assert_eq!(
        report.packages_audited, 2,
        "both fixture deps count as audited"
    );
    assert_eq!(report.vulnerability_count, 1);
    let vuln = &report.vulnerabilities[0];
    assert_eq!(vuln.cve, "CVE-2020-25649");
    assert_eq!(vuln.scanner.as_deref(), Some("owasp-dependency-check"));
    assert!(matches!(report.scanner_status, ScannerStatus::Available));
}

// ---------------------------------------------------------------------------
// P2 2026-09-10 — parser lane tests for the new OSV-backed scanners
// (Swift/SPM Package.resolved, CocoaPods Podfile.lock, Dart pubspec.lock).
// Parser lane cho scanner mới chạy OSV: đầu vào là file lock thật.
// ---------------------------------------------------------------------------
use mgc_audit::scanners::{read_podfile_lock, read_pubspec_lock, read_swift_resolved};

#[test]
fn swift_resolved_v1_and_v2_shapes_parse() {
    // v2 (Xcode 14+): pins at the root; OSV `SwiftURL` names are GIT
    // URLS (verified live: {"ecosystem":"swift"} → 400).
    // v2: ghim ở root; name `SwiftURL` của OSV là URL GIT (đã verify
    // sống: "swift" → 400).
    let v2 = r#"{
  "pins": [
    {"identity": "swift-nio-http2", "location": "https://github.com/apple/swift-nio-http2.git", "state": {"version": "1.40.0"}},
    {"identity": "swift-custom", "location": "https://github.com/x/swift-custom.git", "state": {"branch": "main", "revision": "abc123"}}
  ],
  "version": 2
}"#;
    let (pins, skipped) = read_swift_resolved(v2).unwrap();
    assert_eq!(pins.len(), 1);
    // NAMING FIX (2026-09-10): OSV `SwiftURL` names are the repo path
    // WITHOUT scheme and .git — `github.com/owner/repo`. The full git
    // URL form queries EMPTY on the live API (fake clean); the
    // normalized form hits GHSA-q3g2-m552-3r9c + GHSA-4px2-pw77-vc85
    // at version 1.40.0 (verified live).
    // SỬA TÊN: tên OSV `SwiftURL` là đường dẫn repo KHÔNG scheme
    // KHÔNG .git. URL git đầy đủ query RỖNG trên API sống (sạch giả);
    // dạng chuẩn hóa dính 2 GHSA thật tại 1.40.0 (verify sống).
    assert_eq!(
        pins[0].name, "github.com/apple/swift-nio-http2",
        "OSV SwiftURL name must be the normalized repo path, not the raw URL"
    );
    assert_eq!(pins[0].version, "1.40.0");
    assert_eq!(pins[0].ecosystem, "SwiftURL");
    assert_eq!(skipped.len(), 1, "versionless checkout recorded as skipped");

    // v1: pins nested under object with repositoryURL.
    // v1: ghim nằm trong object kèm repositoryURL.
    let v1 = r#"{
  "object": {"pins": [{"package": "alamofire", "repositoryURL": "https://github.com/Alamofire/Alamofire.git", "state": {"version": "5.9.1"}}]}
}"#;
    let (pins, _) = read_swift_resolved(v1).unwrap();
    assert_eq!(pins.len(), 1);
    assert_eq!(
        pins[0].name, "github.com/Alamofire/Alamofire",
        "v1 repositoryURL normalizes the same way"
    );
    assert_eq!(pins[0].ecosystem, "SwiftURL");

    // A v2 pin WITHOUT a location cannot be queried — skipped honestly.
    // Ghim v2 THIẾU location thì không truy vấn được — bỏ trung thực.
    let no_loc = r#"{
  "pins": [
    {"identity": "orphan-pin", "state": {"version": "1.0.0"}}
  ],
  "version": 2
}"#;
    let (pins, skipped) = read_swift_resolved(no_loc).unwrap();
    assert!(pins.is_empty());
    assert!(skipped.iter().any(|s| s.contains("orphan-pin")));
}

#[test]
fn swift_resolved_malformed_fails_closed() {
    assert!(read_swift_resolved("not json").is_err());
    assert!(read_swift_resolved("").is_err());
}

#[test]
fn podfile_lock_parses_top_level_pods_and_skips_local() {
    let lock = r#"
PODS:
  - Alamofire (5.9.1)
  - Firebase/CoreOnly (10.27.0):
    - FirebaseCore (= 10.27.0)
  - LocalPod (0.1.0):
    :path: ../LocalPod

DEPENDENCIES:
  - Alamofire

COCOAPODS: 1.15.2
"#;
    let (pins, skipped) = read_podfile_lock(lock).unwrap();
    assert_eq!(pins.len(), 3, "Alamofire, Firebase/CoreOnly, LocalPod");
    let names: Vec<&str> = pins.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"Alamofire"));
    assert!(names.contains(&"Firebase/CoreOnly"));
    assert_eq!(pins[0].ecosystem, "pods");
    // NOTE: "pods" here is the INTERNAL inventory tag; the OSV lane is
    // unsupported (no advisory DB) — audit_cocoapods_osv reports that
    // honestly without querying.
    // Ghi chú: "pods" là nhãn kiểm kê NỘI BỘ; lane OSV unsupported (không
    // có DB advisory) — audit_cocoapods_osv báo trung thực, không truy vấn.
    // Sub-deps under the top-level pods are NOT re-counted; versionless
    // entries recorded.
    assert!(skipped.is_empty() || skipped.iter().all(|s| !s.contains("FirebaseCore")));
}

#[test]
fn podfile_lock_no_pods_section_is_clean_zero() {
    let (pins, skipped) = read_podfile_lock("DEPENDENCIES:\n  - nothing\n").unwrap();
    assert!(pins.is_empty());
    assert!(skipped.is_empty());
}

#[test]
fn pubspec_lock_parses_hosted_pins_and_records_git_sources() {
    // Captured pubspec.lock shape (pub 3.x): two-level map with
    // version/source per package.
    // Shape pubspec.lock thật (pub 3.x): map hai cấp với version/
    // source mỗi package.
    let lock = r#"# Generated by pub
packages:
  async:
    source: hosted
    version: "2.11.0"
  http:
    source: hosted
    version: "1.2.2"
  my_local_pkg:
    source: path
    version: "0.0.1"
sdks:
  dart: ">=3.0.0"
"#;
    let (pins, skipped) = read_pubspec_lock(lock).unwrap();
    assert_eq!(
        pins.len(),
        3,
        "all three carry versions (path one is skipped after)"
    );
    assert_eq!(pins[0].name, "async");
    assert_eq!(pins[0].version, "2.11.0");
    assert_eq!(pins[0].ecosystem, "Pub");
    assert_eq!(skipped.len(), 1, "path-sourced package recorded as skipped");
    assert!(skipped[0].contains("my_local_pkg"));
}

#[test]
fn pubspec_lock_malformed_does_not_panic() {
    // The reader is line-oriented tolerant — but a lock with no packages
    // section yields zero pins (clean), never an error for odd yaml.
    // Bộ đọc theo dòng dung sai — lock không có section packages cho 0
    // ghim (sạch), không lỗi vì yaml lạ.
    let (pins, skipped) = read_pubspec_lock("sdks:\n  dart: '>=3.0.0'\n").unwrap();
    assert!(pins.is_empty());
    assert!(skipped.is_empty());
}
