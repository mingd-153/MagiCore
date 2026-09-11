//! Audit scanner for mobile platforms — honest security audit only.
//! Scanner audit cho nền tảng mobile — chỉ audit bảo mật trung thực.
//!
//! `flutter pub outdated` reports VERSION DRIFT, not CVEs: it feeds
//! `dependency_health_flutter`, never the security audit. The security
//! audit stays unavailable until a real CVE scanner exists — a fake clean
//! report is worse than an honest unavailable one.
//! `flutter pub outdated` báo LỆCH VERSION, không phải CVE: kết quả đi vào
//! `dependency_health_flutter`, không bao giờ vào security audit. Audit
//! bảo mật giữ unavailable đến khi có scanner CVE thật — báo sạch giả còn
//! tệ hơn unavailable trung thực.

use mgc_types::adapter::{
    AuditReport, DependencyHealthReport, OutdatedDependency, ScannerStatus, Vulnerability,
    VulnerabilitySeverity,
};
use mgc_types::{MgError, MgResult, PackageId, PackageName, Version};
use serde::Deserialize;
use std::path::Path;

/// Exit codes that mean "tool ran fine" (1 = findings/outdated found).
/// Exit code nghĩa là "tool chạy xong" (1 = có finding/outdated).
const PUB_OUTDATED_OK_EXIT_CODES: [i32; 2] = [0, 1];

// ---------------------------------------------------------------------------
// Typed schemas — mirror REAL tool output. OWASP dependency-check JSON
// report schema verified against the official reporter format; flutter
// `pub outdated --json` verified against the Dart pub tool docs. Parsers
// fail closed on schema mismatch.
// Schema typed — khớp output THẬT của tool; lệch schema thì fail-closed.
// ---------------------------------------------------------------------------

/// `flutter pub outdated --json` — top-level object with package groups
/// (camelCase keys per the real Dart pub output).
#[derive(Debug, Deserialize)]
struct PubOutdatedReport {
    /// At least one recognized group must exist, else the payload is not a
    /// pub outdated report — fail closed instead of reporting zero drift.
    /// Phải có ít nhất 1 group được nhận diện, nếu không payload không phải
    /// report pub outdated — fail-closed thay vì báo 0 drift.
    #[serde(default)]
    packages: Vec<PubOutdatedEntry>,
    #[serde(default, rename = "devPackages")]
    dev_packages: Vec<PubOutdatedEntry>,
    #[serde(default, rename = "dependencyOverrides")]
    dependency_overrides: Vec<PubOutdatedEntry>,
}

#[derive(Debug, Deserialize)]
struct PubOutdatedEntry {
    package: String,
    #[serde(default)]
    current: Option<String>,
    #[serde(default)]
    latest: Option<String>,
}

/// Parse `flutter pub outdated --json` output into a DEPENDENCY HEALTH
/// report — version drift, NOT vulnerabilities.
/// Parse output `flutter pub outdated --json` thành báo cáo ĐỘ TƯƠI
/// dependency — lệch version, KHÔNG phải lỗ hổng.
pub(crate) fn parse_flutter_outdated_json(raw: &str) -> MgResult<DependencyHealthReport> {
    let json_start = raw.find('{').unwrap_or(0);
    let report: PubOutdatedReport = serde_json::from_str(raw[json_start..].trim())
        .map_err(|e| MgError::Other(format!("invalid flutter pub outdated JSON: {e}")))?;

    let mut checked = 0usize;
    let mut outdated = Vec::new();
    let mut rejected = Vec::new();

    for entry in report
        .packages
        .iter()
        .chain(&report.dev_packages)
        .chain(&report.dependency_overrides)
    {
        checked += 1;
        let Ok(pkg_name) = PackageName::new(&entry.package) else {
            rejected.push(format!("package '{}'", entry.package));
            continue;
        };
        let latest = entry.latest.as_deref().unwrap_or("");
        let current = entry.current.as_deref().unwrap_or("");
        // Entries without a resolvable latest (discontinued/unpublished) are
        // counted as checked but carry no actionable target.
        // Entry không có latest giải được (ngừng publish) tính là đã kiểm
        // tra nhưng không có mục tiêu xử lý.
        if latest.is_empty() {
            continue;
        }
        let (Ok(latest_version), Ok(current_version)) =
            (Version::parse(latest), Version::parse(current))
        else {
            rejected.push(format!(
                "package '{}' versions '{current}'/'{latest}'",
                entry.package
            ));
            continue;
        };
        if current_version == latest_version {
            continue;
        }
        outdated.push(OutdatedDependency {
            package: PackageId::new(pkg_name, current_version),
            latest_version,
        });
    }

    // No recognized group at all → schema error, not "zero drift".
    // Không group nào được nhận diện → lỗi schema, không phải "0 drift".
    if checked == 0 {
        return Err(MgError::Other(
            "flutter pub outdated JSON has no packages/devPackages entries".to_string(),
        ));
    }
    if !rejected.is_empty() {
        return Err(MgError::Other(format!(
            "flutter pub outdated JSON contained {} unparseable entries: {}",
            rejected.len(),
            rejected.join("; ")
        )));
    }

    Ok(DependencyHealthReport {
        packages_checked: checked,
        outdated_count: outdated.len(),
        outdated,
    })
}

/// Dependency health check for Flutter — runs `flutter pub outdated --json`
/// and reports version drift. This is NOT a security audit.
/// Kiểm tra độ tươi dependency Flutter — chạy `flutter pub outdated --json`
/// và báo lệch version. Đây KHÔNG phải security audit.
pub async fn dependency_health_flutter(project_root: &Path) -> MgResult<DependencyHealthReport> {
    // Fail closed: without the CLI we CANNOT know freshness — returning a
    // default report would fake "everything is up to date".
    // Fail-closed: không có CLI thì KHÔNG BIẾT độ tươi — trả report mặc
    // định đồng nghĩa bịa "mọi thứ đều mới".
    if which::which("flutter").is_err() {
        return Err(MgError::Other(
            "flutter CLI not installed — cannot check dependency freshness".to_string(),
        ));
    }

    let args = vec![
        "pub".to_string(),
        "outdated".to_string(),
        "--json".to_string(),
    ];
    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        // Exit 1 means outdated packages found — JSON payload still valid.
        // Exit 1 nghĩa là có outdated — payload JSON vẫn hợp lệ.
        allowed_exit_codes: PUB_OUTDATED_OK_EXIT_CODES.to_vec(),
        ..Default::default()
    };

    let result = mgc_exec::run::run("flutter", &args, &exec_opts)
        .map_err(|e| MgError::Other(format!("flutter pub outdated failed: {}", e)))?;

    if !PUB_OUTDATED_OK_EXIT_CODES.contains(&result.exit_code) {
        return Err(MgError::Other(format!(
            "flutter pub outdated exited with code {}",
            result.exit_code
        )));
    }

    parse_flutter_outdated_json(&result.stdout_tail)
}

/// Security audit for Flutter — honestly unavailable: `pub outdated` is a
/// freshness check, not a CVE scanner, and no dedicated Flutter CVE scanner
/// is implemented yet. Feeding outdated packages in here would mislabel
/// version drift as security findings.
/// Security audit Flutter — trung thực unavailable: `pub outdated` là kiểm
/// tra độ tươi, không phải scanner CVE; chưa có scanner CVE Flutter nào
/// được hiện thực. Ép package cũ vào đây sẽ gán nhầm lệch version thành
/// finding bảo mật.
pub async fn audit_flutter(project_root: &Path) -> MgResult<AuditReport> {
    // P2 2026-09-10: pubspec.lock pins → OSV.dev `pub` ecosystem — a
    // REAL CVE scan; without the lockfile the honest unsupported state
    // stays (pub outdated is freshness, not security).
    // pubspec.lock ghim → OSV.dev ecosystem `pub` — scan CVE THẬT; thiếu
    // lockfile giữ trạng thái unsupported trung thực (pub outdated chỉ
    // là độ tươi, không phải bảo mật).
    mgc_audit::scanners::audit_flutter_osv(project_root).await
}

pub async fn audit_kotlin(project_root: &Path) -> MgResult<AuditReport> {
    let tool = if project_root.join("gradlew").exists() {
        "./gradlew"
    } else if which::which("gradle").is_ok() {
        "gradle"
    } else {
        return Ok(AuditReport::tool_missing(
            "gradle/gradlew",
            "install Gradle (https://gradle.org/install/) or add the Gradle wrapper to the project",
        ));
    };

    let args = vec!["dependencyCheckAnalyze".to_string()];
    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };

    let report_path = project_root
        .join("build")
        .join("reports")
        .join("dependency-check-report.json");

    // Stale-report guard (Tech Lead P0-4): remove any report from a
    // PREVIOUS run BEFORE invoking the scanner. After a successful
    // delete, the ONLY way the file exists again is that THIS scanner
    // run wrote it — no mtime comparison (some filesystems have coarse
    // timestamp resolution) and no stale reuse path.
    // Chống stale report: xóa report của lần chạy TRƯỚC trước khi gọi
    // scanner. Sau khi xóa thành công, file chỉ có thể tồn tại trở lại
    // nếu CHÍNH LẦN CHẠY NÀY ghi ra — không so mtime (một số filesystem
    // độ phân giải timestamp thấp) và không còn đường tái dùng report cũ.
    let removed_previous = std::fs::remove_file(&report_path);
    // remove_file errors ONLY when the file did not exist (ENOENT) —
    // any other error (permissions etc.) must fail the audit closed.
    // remove_file chỉ lỗi khi file KHÔNG tồn tại (ENOENT) — lỗi khác
    // (quyền...) thì audit phải fail-closed.
    if let Err(e) = &removed_previous
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Ok(AuditReport::scanner_failed(
            "owasp-dependency-check",
            format!("cannot remove stale report {}: {e}", report_path.display()),
        ));
    }

    let result = mgc_exec::run::run(tool, &args, &exec_opts);
    // OWASP dependency-check writes its report regardless of the gradle
    // exit code; parse the JSON report ONLY when it was freshly written
    // by THIS run (pre-run delete guarantees existence == fresh), else
    // fail closed with the real reason.
    // dependency-check ghi report bất kể exit code; chỉ parse JSON khi
    // file được CHÍNH LẦN NÀY ghi (đã xóa trước run nên tồn tại == mới),
    // không thì fail-closed với lý do thật.
    let report_fresh = report_path.exists();
    if !report_fresh {
        let reason = match &result {
            Ok(r) if r.exit_code == 0 => {
                "OWASP dependency-check ran but produced no fresh report — is the plugin configured?"
                    .to_string()
            }
            Ok(r) => format!("dependencyCheckAnalyze exited with code {}", r.exit_code),
            Err(e) => format!("dependencyCheckAnalyze failed: {e}"),
        };
        return Ok(AuditReport::scanner_failed(
            "owasp-dependency-check",
            reason,
        ));
    }

    parse_owasp_dependency_check_json(&std::fs::read_to_string(&report_path)?)
}

/// Typed OWASP dependency-check report schema — verified against the
/// official JSON reporter format (dependencies[].packages[].package.id
/// purl, vulnerabilities[].cvssv3/cvssv2, projectReportDate etc.).
#[derive(Debug, Deserialize)]
struct OwaspReport {
    dependencies: Vec<OwaspDependency>,
}

#[derive(Debug, Deserialize)]
struct OwaspDependency {
    #[serde(default)]
    packages: Vec<OwaspPackageWrapper>,
    #[serde(default)]
    vulnerabilities: Vec<OwaspVulnerability>,
}

#[derive(Debug, Deserialize)]
struct OwaspPackageWrapper {
    package: OwaspPackage,
}

#[derive(Debug, Deserialize)]
struct OwaspPackage {
    /// Package URL, e.g. `pkg:maven/com.example/lib@1.0`.
    id: String,
}

#[derive(Debug, Deserialize)]
struct OwaspVulnerability {
    name: String,
    #[serde(default)]
    severity: Option<String>,
    cvssv3: Option<OwaspCvss>,
    cvssv2: Option<OwaspCvss>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OwaspCvss {
    /// baseScore — may be absent on partial CVSS records.
    /// baseScore — có thể thiếu trên bản ghi CVSS không đầy đủ.
    #[serde(default, rename = "baseScore")]
    base_score: Option<f64>,
}

/// Parse an OWASP dependency-check JSON report into an audit report.
/// Parse report JSON của dependency-check thành audit report.
pub(crate) fn parse_owasp_dependency_check_json(raw: &str) -> MgResult<AuditReport> {
    let report: OwaspReport = serde_json::from_str(raw)
        .map_err(|e| MgError::Other(format!("invalid dependency-check JSON: {e}")))?;

    let mut vulnerabilities = Vec::new();
    for dep in &report.dependencies {
        // A dep without a purl cannot be attributed — count it audited but
        // raise on findings we cannot place; never silently drop them.
        // Dep không có purl thì không gán finding được — đếm là đã audit
        // nhưng finding không đặt được thì phải nêu, không bỏ âm thầm.
        let (pkg_name, pkg_version) = dep
            .packages
            .first()
            .map(|w| purl_name_version(&w.package.id))
            .unwrap_or_else(|| (PackageName::new("artifact").expect("static name"), None));
        for vuln in &dep.vulnerabilities {
            let severity_level = owasp_severity(vuln);
            let description = vuln.description.as_deref().unwrap_or("");
            vulnerabilities.push(
                Vulnerability {
                    package: PackageId::new(
                        pkg_name.clone(),
                        pkg_version.clone().unwrap_or_else(|| Version::new(0, 0, 0)),
                    ),
                    title: format!("{}: {}", vuln.name, truncate_utf8(description, 200)),
                    severity: vuln
                        .severity
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                    cve: vuln.name.clone(),
                    severity_level,
                    patched_versions: None,
                    url: None,
                    scanner: None,
                    ecosystem: None,
                    evidence_at: None,
                }
                .with_evidence("owasp-dependency-check", "kotlin/jvm"),
            );
        }
    }

    let count = vulnerabilities.len();
    Ok(AuditReport {
        packages_audited: report.dependencies.len(),
        vulnerability_count: count,
        vulnerabilities,
        scanner_status: ScannerStatus::Available,
    })
}

/// Split a purl like `pkg:maven/com.example/lib@1.0` into (name, version).
/// The version after '@' is the AFFECTED version — carrying it makes the
/// finding actionable instead of a fake 0.0.0.
/// Tách purl `pkg:maven/com.example/lib@1.0` thành (name, version).
/// Version sau '@' là version BỊ ẢNH HƯỞNG — giữ nó làm finding có ích,
/// không thay bằng 0.0.0 giả.
fn purl_name_version(purl: &str) -> (PackageName, Option<Version>) {
    let (id_part, version) = match purl.rsplit_once('@') {
        Some((id, v)) => (id, Version::parse(v).ok()),
        None => (purl, None),
    };
    // Artifact tail may contain '::' qualifiers (pkg:npm/name@1.0?qualifier)
    // — strip them before taking the tail.
    let artifact = id_part
        .rsplit('/')
        .next()
        .unwrap_or("artifact")
        .split('?')
        .next()
        .unwrap_or("artifact");
    let name = PackageName::new(artifact)
        .unwrap_or_else(|_| PackageName::new("artifact").expect("static fallback name is valid"));
    (name, version)
}

/// Map dependency-check severity: CVSS v3/v2 base score wins; else the
/// severity string. Bands follow CVSS qualitative ratings.
/// Map severity dependency-check: ưu tiên base score CVSS v3/v2; còn lại
/// theo chuỗi severity. Bảng theo xếp loại định tính CVSS.
fn owasp_severity(vuln: &OwaspVulnerability) -> VulnerabilitySeverity {
    let score = vuln
        .cvssv3
        .as_ref()
        .and_then(|c| c.base_score)
        .or_else(|| vuln.cvssv2.as_ref().and_then(|c| c.base_score));
    match score {
        Some(s) if s >= 9.0 => VulnerabilitySeverity::Critical,
        Some(s) if s >= 7.0 => VulnerabilitySeverity::High,
        Some(s) if s >= 4.0 => VulnerabilitySeverity::Medium,
        Some(s) if s > 0.0 => VulnerabilitySeverity::Low,
        Some(_) => VulnerabilitySeverity::Info,
        None => VulnerabilitySeverity::from_str(vuln.severity.as_deref().unwrap_or("")),
    }
}

/// Truncate to at most `max_chars` chars, respecting char (not byte)
/// boundaries — advisory text is external data and may contain any UTF-8.
/// Cắt tối đa `max_chars` ký tự, đúng ranh giới ký tự (không phải byte) —
/// text advisory là dữ liệu ngoài, có thể chứa UTF-8 bất kỳ.
fn truncate_utf8(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

pub async fn audit_swift(project_root: &Path) -> MgResult<AuditReport> {
    // P2 2026-09-10: Package.resolved pins → OSV.dev `swift` ecosystem
    // — a REAL CVE scan via the shared OSV client.
    // Package.resolved ghim → OSV.dev ecosystem `swift` — scan CVE THẬT
    // qua OSV client dùng chung.
    mgc_audit::scanners::audit_swift_spam(project_root).await
}

pub async fn audit_cocoapods(project_root: &Path) -> MgResult<AuditReport> {
    // P2 2026-09-10: Podfile.lock pins → OSV.dev `pods` ecosystem.
    // Podfile.lock ghim → OSV.dev ecosystem `pods`.
    mgc_audit::scanners::audit_cocoapods_osv(project_root).await
}

pub async fn audit_multi(project_root: &Path) -> MgResult<AuditReport> {
    // Aggregate EVERY detected manifest — never first-match (Tech Lead
    // 2026-09-09: a multi-language project like React Native must merge
    // every ecosystem result into one report).
    // Tổng hợp MỌI manifest nhận diện — không first-match: project đa
    // ngôn ngữ (vd React Native) phải gộp kết quả mọi ecosystem.
    let mut plan = mgc_audit::AuditPlan::new();

    if project_root.join("pubspec.yaml").exists() {
        let root = project_root.to_path_buf();
        plan.add_step(mgc_audit::ScanStep {
            ecosystem: "flutter",
            scanner: "pub-osv",
            run: Box::new(move || {
                let root = root.clone();
                Box::pin(async move { audit_flutter(&root).await })
            }),
        });
    }

    // JS lane (P2 React Native aggregate 2026-09-10): mgc.lock /
    // bun.lock / deno.lock pins ride the npm Bulk Advisory flow — the
    // SAME pipeline as the web core. Without a JS lockfile the step is
    // absent (an un-manifested lane never fakes a clean step).
    // Lane JS (aggregate React Native): ghim mgc.lock / bun.lock /
    // deno.lock đi qua npm Bulk Advisory — CÙNG pipeline với core web.
    // Thiếu lockfile JS thì bỏ hẳn step (lane không manifest không bịa
    // step sạch).
    let js_locks_exist = project_root.join("mgc.lock").is_file()
        || project_root.join("bun.lock").is_file()
        || project_root.join("deno.lock").is_file();
    if js_locks_exist {
        let root = project_root.to_path_buf();
        plan.add_step(mgc_audit::ScanStep {
            ecosystem: "web/javascript",
            scanner: "npm-bulk-advisory",
            run: Box::new(move || {
                let root = root.clone();
                Box::pin(async move { audit_app_js_deps(&root).await })
            }),
        });
    }

    let has_gradle = project_root.join("build.gradle").exists()
        || project_root.join("build.gradle.kts").exists();
    if has_gradle {
        let root = project_root.to_path_buf();
        plan.add_step(mgc_audit::ScanStep {
            ecosystem: "kotlin",
            scanner: "owasp-dependency-check",
            run: Box::new(move || {
                let root = root.clone();
                Box::pin(async move { audit_kotlin(&root).await })
            }),
        });
    }

    // iOS lanes (P2 2026-09-10): SPM + CocoaPods — a React Native app
    // with Pods merges iOS findings into the same aggregate.
    // Lane iOS: SPM + CocoaPods — app React Native có Pods gộp finding
    // iOS vào cùng aggregate.
    if find_swift_resolved(project_root).is_some() {
        let root = project_root.to_path_buf();
        plan.add_step(mgc_audit::ScanStep {
            ecosystem: "swift",
            scanner: "osv-dev-api",
            run: Box::new(move || {
                let root = root.clone();
                Box::pin(async move { audit_swift(&root).await })
            }),
        });
    }
    if project_root.join("Podfile.lock").is_file() {
        let root = project_root.to_path_buf();
        plan.add_step(mgc_audit::ScanStep {
            ecosystem: "objc/cocoapods",
            scanner: "osv-dev-api",
            run: Box::new(move || {
                let root = root.clone();
                Box::pin(async move { audit_cocoapods(&root).await })
            }),
        });
    }

    if plan.is_empty() {
        return Ok(AuditReport::unsupported_ecosystem(
            "app/multi (no recognized app manifest found: pubspec.yaml, build.gradle)",
        ));
    }

    plan.execute().await
}

fn find_swift_resolved(project_root: &Path) -> Option<std::path::PathBuf> {
    let candidates = [
        "Package.resolved".to_string(),
        "ios/Package.resolved".to_string(),
    ];
    candidates
        .iter()
        .map(|c| project_root.join(c))
        .find(|p| p.is_file())
}

/// JS dependencies of an RN/multi app via the npm Bulk Advisory flow
/// (shared readers + the registry endpoint default). Runs the same
/// network path as the web core audit.
/// Dependency JS của app RN/multi qua npm Bulk Advisory (bộ đọc dùng
/// chung + endpoint registry mặc định) — cùng đường mạng với audit core
/// web.
async fn audit_app_js_deps(project_root: &Path) -> MgResult<AuditReport> {
    mgc_web_adapter::audit::run_audit(project_root, mgc_web_adapter::DEFAULT_NPM_REGISTRY).await
}

// SILENT-SKIP FIX (REVIEW 2026-09-10): this test module existed as an
// ORPHAN file (src/audit/test/scanner_test.rs was never declared) — the
// parser regression tests NEVER ran. Wiring it in makes the whole
// Kotlin OWASP + flutter-health lane enforceable in CI.
// FIX SKIP ÂM THẦM: module test này từng là file MỒ CÔI (chưa từng được
// khai báo) — regression test của parser chưa bao giờ chạy. Nối vào để
// lane Kotlin OWASP + flutter-health thực thi được trong CI.
#[cfg(test)]
#[path = "test/scanner_test.rs"]
mod scanner_test;
