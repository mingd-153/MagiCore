//! `scanners/osv.rs` — Shared OSV.dev API scanner (P2 2026-09-10) for
//! ecosystems that have NO native CLI scanner yet: Swift/SPM
//! (`swift` Package.resolved), CocoaPods (`pods` Podfile.lock),
//! Dart/pub (`pub` pubspec.lock — see `dart.rs`).
//!
//! One POST /v1/query per pinned package {package: {ecosystem, name},
//! version}; the response lists OSV ids. Each id is then fetched from
//! /v1/vulns/{id} for severity + aliases (CVE). The parser is typed
//! and fail-closed: an unshapeable response is an error, never a fake
//! clean. OSV carries no severity label in the query response — the
//! vuln detail endpoint's `severity` CVSS vector drives the level;
//! absent both → honest Info.
//!
//! Scanner OSV.dev dùng chung cho ecosystem chưa có CLI scanner riêng:
//! Swift/SPM (Package.resolved), CocoaPods (Podfile.lock), Dart/pub.
//! Một POST /v1/query mỗi package ghim; response liệt kê id OSV; mỗi
//! id lấy chi tiết từ /v1/vulns/{id} để có severity + alias CVE.
//! Parser typed fail-closed: response không đúng shape là lỗi, không
//! bao giờ bịa sạch.

use mgc_types::adapter::{AuditReport, ScannerStatus, Vulnerability, VulnerabilitySeverity};
use mgc_types::{MgError, MgResult, PackageId, PackageName, Version};
use serde::Deserialize;
use std::path::Path;

/// OSV.dev API base (public instance; overridable via MGC_OSV_API_BASE
/// for offline/failure-lane testing — the 10-test/lane tool-failure
/// evidence needs a deterministic dead endpoint, not a flaky network).
/// Cơ sở API OSV.dev (bản công cộng; ghi đè qua MGC_OSV_API_BASE cho
/// test offline/failure-lane — evidence tool-failure cần endpoint chết
/// tất định, không phải mạng chập chờn).
fn osv_api_base() -> String {
    std::env::var("MGC_OSV_API_BASE").unwrap_or_else(|_| "https://api.osv.dev/v1".to_string())
}

/// One pinned dependency to query.
/// Một dependency ghim để truy vấn.
#[derive(Debug, Clone)]
pub struct OsvPin {
    pub name: String,
    pub version: String,
    pub ecosystem: &'static str,
}

/// Query response: OSV ids for ONE package+version.
/// Response query: các id OSV cho MỘT package+version.
#[derive(Debug, Deserialize)]
struct OsvQueryResponse {
    #[serde(default)]
    vulns: Vec<OsvIdOnly>,
}

#[derive(Debug, Deserialize)]
struct OsvIdOnly {
    id: String,
}

/// Vuln detail (subset of the OSV schema we consume).
/// Chi tiết vuln (tập con schema OSV ta dùng).
#[derive(Debug, Deserialize)]
struct OsvVuln {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    details: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    severity: Vec<OsvSeverity>,
    #[serde(default, rename = "database_specific")]
    database_specific: Option<OsvDbSpecific>,
    #[serde(default)]
    references: Vec<OsvReference>,
}

#[derive(Debug, Deserialize)]
struct OsvSeverity {
    #[serde(default)]
    score: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    r#type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OsvDbSpecific {
    #[serde(default)]
    severity: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OsvReference {
    #[serde(default)]
    url: Option<String>,
}

/// Query OSV for ONE pin: POST /v1/query → ids, then GET /v1/vulns/{id}
/// per id for the full record. Any non-2xx or unshapeable payload is a
/// hard error (fail-closed).
/// Truy vấn OSV cho MỘT ghim: POST /v1/query → ids, rồi GET
/// /v1/vulns/{id} từng id lấy bản ghi đầy đủ. Non-2xx hoặc payload
/// không đúng shape là lỗi cứng (fail-closed).
async fn query_pin(client: &mgc_http::HttpClient, pin: &OsvPin) -> MgResult<Vec<Vulnerability>> {
    let body = serde_json::json!({
        "package": {"ecosystem": pin.ecosystem, "name": pin.name},
        "version": pin.version,
    });
    // The shared HttpClient owns transport (RULE §8 no direct reqwest):
    // serialize the JSON body to bytes; the client is preconfigured
    // with the JSON content type by the caller.
    // HttpClient dùng chung giữ transport (RULE §8 không reqwest trực
    // tiếp): serialize body JSON ra bytes; client đã được caller cấu
    // hình sẵn content-type JSON.
    let body_bytes = serde_json::to_vec(&body).map_err(|e| {
        MgError::Other(format!(
            "osv query body for '{}' unserializable: {e}",
            pin.name
        ))
    })?;
    let response = client
        .post(format!("{}/query", osv_api_base()).as_str(), body_bytes)
        .await
        .map_err(|e| MgError::Network(format!("osv query '{}' failed: {e}", pin.name)))?;
    if !response.status().is_success() {
        return Err(MgError::Network(format!(
            "osv query '{}' returned {}",
            pin.name,
            response.status()
        )));
    }
    let ids: OsvQueryResponse = response.json().await.map_err(|e| {
        MgError::Other(format!(
            "osv query response for '{}' unparseable: {e}",
            pin.name
        ))
    })?;
    if ids.vulns.is_empty() {
        return Ok(vec![]);
    }

    let mut findings = Vec::new();
    for id in &ids.vulns {
        let detail = client
            .get(format!("{}/vulns/{}", osv_api_base(), id.id).as_str())
            .await
            .map_err(|e| MgError::Network(format!("osv vuln '{}' fetch failed: {e}", id.id)))?;
        if !detail.status().is_success() {
            return Err(MgError::Network(format!(
                "osv vuln '{}' returned {}",
                id.id,
                detail.status()
            )));
        }
        let vuln: OsvVuln = detail
            .json()
            .await
            .map_err(|e| MgError::Other(format!("osv vuln '{}' unparseable: {e}", id.id)))?;

        let (severity, level) = osv_severity(&vuln);
        let cve = vuln
            .aliases
            .iter()
            .find(|a| a.starts_with("CVE-"))
            .cloned()
            .unwrap_or_else(|| vuln.id.clone());
        let summary = if vuln.summary.is_empty() {
            vuln.details.clone()
        } else {
            vuln.summary.clone()
        };
        let Ok(name) = PackageName::new(pin.name.clone()) else {
            return Err(MgError::Other(format!(
                "osv pin name '{}' cannot be represented as a package name",
                pin.name
            )));
        };
        let Ok(version) = Version::parse(&pin.version) else {
            return Err(MgError::Other(format!(
                "osv pin version '{}' unparseable as semver",
                pin.version
            )));
        };
        findings.push(
            Vulnerability {
                package: PackageId::new(name, version),
                title: format!("{}: {}", vuln.id, truncate(&summary, 200)),
                severity,
                cve,
                severity_level: level,
                patched_versions: None,
                url: vuln
                    .references
                    .iter()
                    .find_map(|r| r.url.clone())
                    .or_else(|| Some(format!("{}/vulns/{}", osv_api_base(), vuln.id))),
                scanner: None,
                ecosystem: None,
                evidence_at: None,
            }
            .with_evidence("osv-dev-api", pin.ecosystem),
        );
    }
    Ok(findings)
}

/// Severity: database_specific label > CVSS vector (shared base
/// implementation) > honest Info.
/// Severity: nhãn database_specific > vector CVSS (dùng chung bản cơ
/// sở) > Info trung thực.
fn osv_severity(vuln: &OsvVuln) -> (String, VulnerabilitySeverity) {
    if let Some(label) = vuln
        .database_specific
        .as_ref()
        .and_then(|d| d.severity.as_deref())
    {
        let level = match label.to_uppercase().as_str() {
            "CRITICAL" => VulnerabilitySeverity::Critical,
            "HIGH" => VulnerabilitySeverity::High,
            "MODERATE" | "MEDIUM" => VulnerabilitySeverity::Medium,
            "LOW" => VulnerabilitySeverity::Low,
            _ => VulnerabilitySeverity::Info,
        };
        return (label.to_lowercase(), level);
    }
    if let Some(level) = vuln
        .severity
        .iter()
        .find_map(|s| s.score.as_deref())
        .and_then(crate::scanners::cvss_base_severity_for_go)
    {
        return (level.as_str().to_string(), level);
    }
    ("info".to_string(), VulnerabilitySeverity::Info)
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        text.chars().take(max_chars).collect()
    }
}

/// Query OSV for a set of pins; a network failure of ANY pin is a
/// SCANNER FAILURE (fail-closed through the report status, never a
/// hard error and never a partial-clean answer). The CLI finisher
/// turns Failed into the honest UNVERIFIED lane (exit 2 strict).
/// Truy vấn OSV cho một tập ghim; MỘT pin lỗi mạng là SCANNER FAILED
/// (fail-closed qua status report, không phải hard error, không phải
/// trả lời sạch từng phần). Finisher CLI chuyển Failed thành lane
/// UNVERIFIED trung thực (strict exit 2).
pub async fn audit_osv_pins(pins: &[OsvPin]) -> MgResult<AuditReport> {
    if pins.is_empty() {
        return Ok(AuditReport::clean(0));
    }
    // L1-hardcode (RULE §8): core crates never touch reqwest directly —
    // the shared mgc_http::HttpClient owns transport. Preconfigured with
    // the JSON content type for OSV query posts. CI hygiene gate
    // rejects direct reqwest in mgc-audit.
    // L1-hardcode (RULE §8): crate core không chạm reqwest trực tiếp —
    // mgc_http::HttpClient dùng chung giữ transport; cấu hình sẵn
    // content-type JSON cho POST query OSV. Hygiene gate CI từ chối
    // reqwest trực tiếp trong mgc-audit.
    let client = mgc_http::HttpClient::new()
        .map_err(|e| MgError::Network(format!("osv client error: {e}")))?
        .with_auth("Content-Type", "application/json");

    let mut all = Vec::new();
    for pin in pins {
        match query_pin(&client, pin).await {
            Ok(findings) => all.extend(findings),
            Err(e) => {
                return Ok(AuditReport::scanner_failed(
                    "osv-dev-api",
                    format!("query for '{}' failed: {e}", pin.name),
                ));
            }
        }
    }
    Ok(AuditReport {
        packages_audited: pins.len(),
        vulnerability_count: all.len(),
        vulnerabilities: all,
        scanner_status: ScannerStatus::Available,
    })
}

// ---------------------------------------------------------------------------
// Swift Package Manager — Package.resolved (state v1 JSON / v2 JSON).
// ---------------------------------------------------------------------------

/// Package.resolved v2 (Xcode 14+): {"pins": [{"identity", "state":
/// {"version"}}], "version": 2}; v1: {"object": {"pins": [...]}}.
/// Both shapes parse into the same pins.
/// Package.resolved v2: pins trực tiếp; v1: pins trong object — cả hai
/// về cùng một tập ghim.
#[derive(Debug, Deserialize)]
struct SwiftResolved {
    #[serde(default)]
    pins: Vec<SwiftPin>,
    #[serde(default)]
    object: Option<SwiftResolvedV1Object>,
}

#[derive(Debug, Deserialize)]
struct SwiftResolvedV1Object {
    #[serde(default)]
    pins: Vec<SwiftPinV1>,
}

#[derive(Debug, Deserialize)]
struct SwiftPin {
    #[serde(default)]
    identity: String,
    /// v2 pins carry the git location (OSV `SwiftURL` names are git URLs).
    /// Ghim v2 kèm vị trí git (name `SwiftURL` của OSV là URL git).
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    state: Option<SwiftPinState>,
}

#[derive(Debug, Deserialize)]
struct SwiftPinState {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    revision: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SwiftPinV1 {
    #[serde(default)]
    package: String,
    /// v1 pins carry repositoryURL (same git-URL naming contract).
    /// Ghim v1 kèm repositoryURL (cùng hợp đồng đặt tên URL git).
    #[serde(default, rename = "repositoryURL")]
    repository_url: Option<String>,
    #[serde(default)]
    state: Option<SwiftPinState>,
}

/// Read Package.resolved into OSV pins (`swift` ecosystem). Versionless
/// pins (branch/revision checkouts) are recorded as skipped — they
/// cannot be version-matched against advisories and must never be
/// silently counted clean.
/// Đọc Package.resolved thành ghim OSV (ecosystem `swift`). Ghim không
/// version (checkout branch/revision) được ghi là skipped — không đối
/// chiếu version với advisory được và không được phép đếm sạch âm thầm.
pub fn read_swift_resolved(raw: &str) -> MgResult<(Vec<OsvPin>, Vec<String>)> {
    let resolved: SwiftResolved = serde_json::from_str(raw)
        .map_err(|e| MgError::Other(format!("invalid Package.resolved: {e}")))?;

    let mut pins = Vec::new();
    let mut skipped = Vec::new();
    // v2 pins live at the root; v1 nests under object.pins.
    // Ghim v2 ở root; v1 nằm trong object.pins.
    for pin in resolved.pins {
        push_swift_pin(
            &pin.identity,
            pin.location.as_deref(),
            pin.state.as_ref(),
            &mut pins,
            &mut skipped,
        );
    }
    if let Some(object) = resolved.object {
        for pin in object.pins {
            push_swift_pin(
                &pin.package,
                pin.repository_url.as_deref(),
                pin.state.as_ref(),
                &mut pins,
                &mut skipped,
            );
        }
    }
    Ok((pins, skipped))
}

fn push_swift_pin(
    identity: &str,
    location: Option<&str>,
    state: Option<&SwiftPinState>,
    pins: &mut Vec<OsvPin>,
    skipped: &mut Vec<String>,
) {
    let Some(state) = state else {
        skipped.push(format!("{identity}: no state recorded"));
        return;
    };
    match state.version.as_deref() {
        Some(version) if !version.is_empty() => {
            // OSV's Swift ecosystem is `SwiftURL` (verified live:
            // {"ecosystem":"swift"} → 400 invalid ecosystem). CRITICAL
            // NAMING FIX (2026-09-10): the OSV package NAME is the repo
            // path WITHOUT scheme and .git suffix — `github.com/owner/
            // repo` (verified live: swift-nio-http2@1.40.0 under
            // `github.com/apple/swift-nio-http2` returns GHSA-q3g2 +
            // GHSA-4px2, while the full git URL form `https://github.
            // com/apple/swift-nio-http2.git` returns EMPTY — a silent
            // fake-clean). A location is still required; unparsable
            // locations are skipped honestly, never queried wrong.
            // Ecosystem Swift của OSV là `SwiftURL` (verify sống:
            // "swift" → 400). SỬA TÊN QUAN TRỌNG: tên package OSV là
            // đường dẫn repo KHÔNG scheme KHÔNG đuôi .git —
            // `github.com/owner/repo` (verify sống: tên đúng trả 2
            // GHSA, URL git đầy đủ trả RỖNG — sạch giả âm thầm).
            // Vẫn cần location; location không tách được thì skip
            // trung thực, không query sai.
            let Some(location) = location.filter(|l| !l.is_empty()) else {
                skipped.push(format!(
                    "{identity}: pin carries no git location — OSV SwiftURL queries need the repo URL"
                ));
                return;
            };
            let Some(osv_name) = osv_swifturl_name(location) else {
                skipped.push(format!(
                    "{identity}: location '{location}' is not a github.com/owner/repo URL — cannot build the OSV SwiftURL name"
                ));
                return;
            };
            pins.push(OsvPin {
                name: osv_name,
                version: version.to_string(),
                ecosystem: "SwiftURL",
            });
        }
        _ => skipped.push(format!(
            "{identity}: versionless checkout (branch={}, revision={})",
            state.branch.as_deref().unwrap_or("?"),
            state.revision.as_deref().unwrap_or("?"),
        )),
    }
}

/// Normalize a git URL into the OSV `SwiftURL` package name: strip the
/// scheme, strip `.git`. Only host-preserving paths are kept (github
/// and friends host their advisories by full repo path).
/// Chuẩn hóa URL git thành tên package OSV `SwiftURL`: bỏ scheme, bỏ
/// `.git`. Chỉ giữ đường dẫn còn nguyên host (advisory ghi theo đường
/// dẫn repo đầy đủ).
fn osv_swifturl_name(location: &str) -> Option<String> {
    let path = location
        .strip_prefix("https://")
        .or_else(|| location.strip_prefix("http://"))
        .or_else(|| location.strip_prefix("git://"))
        .or_else(|| location.strip_prefix("ssh://"))
        .or_else(|| location.strip_prefix("git@"))
        .unwrap_or(location);
    // git@github.com:owner/repo.git → github.com/owner/repo.git
    let path = path.replace(':', "/");
    let path = path.strip_suffix(".git").unwrap_or(&path);
    let path = path.trim_end_matches('/');
    // Expect host/owner/repo (exactly 3 non-empty segments).
    // Cần host/owner/repo (đúng 3 đoạn khác rỗng).
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() != 3 {
        return None;
    }
    Some(path.to_string())
}

/// Swift/SPM audit: Package.resolved → OSV `swift` ecosystem queries.
pub async fn audit_swift_spam(project_root: &Path) -> MgResult<AuditReport> {
    let resolved_path = find_upwards(project_root, "Package.resolved");
    let Some(path) = resolved_path else {
        return Ok(AuditReport::unsupported_ecosystem(
            "swift/swiftpm (no Package.resolved found — run swift package resolve first)",
        ));
    };
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| MgError::Other(format!("read Package.resolved: {e}")))?;
    let (pins, skipped) = read_swift_resolved(&raw)?;
    if pins.is_empty() && skipped.is_empty() {
        return Ok(AuditReport::clean(0));
    }
    if pins.is_empty() {
        return Ok(AuditReport {
            packages_audited: 0,
            vulnerability_count: 0,
            vulnerabilities: vec![],
            scanner_status: ScannerStatus::Partial {
                scanned: 0,
                skipped: skipped.len(),
                reasons: skipped,
            },
        });
    }
    let mut report = audit_osv_pins(&pins).await?;
    if !skipped.is_empty() {
        report.scanner_status = ScannerStatus::Partial {
            scanned: pins.len(),
            skipped: skipped.len(),
            reasons: skipped,
        };
    }
    Ok(report)
}

/// Search a file upwards from a root (Package.resolved lives in
/// Package.resolved, xcodeproj/..., or DerivedData-adjacent spots).
/// Tìm file theo chiều lên từ root.
fn find_upwards(root: &Path, name: &str) -> Option<std::path::PathBuf> {
    let mut current = root.to_path_buf();
    loop {
        let candidate = current.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !current.pop() {
            return None;
        }
    }
}

// ---------------------------------------------------------------------------
// CocoaPods — Podfile.lock (PODS: section, deterministic indentation).
// ---------------------------------------------------------------------------

/// Read Podfile.lock into OSV pins (`pods` ecosystem = OSV name for
/// CocoaPods). DEPENDENCIES/pods with local :path or git sources are
/// skipped honestly (no registry advisory mapping).
/// Đọc Podfile.lock thành ghim OSV (ecosystem `pods` — tên OSV cho
/// CocoaPods). Pod nguồn local :path hoặc git được bỏ trung thực
/// (không mapping advisory registry).
pub fn read_podfile_lock(raw: &str) -> MgResult<(Vec<OsvPin>, Vec<String>)> {
    let mut pins = Vec::new();
    let mut skipped = Vec::new();
    let mut in_pods = false;
    for line in raw.lines() {
        let trimmed = line.trim_end();
        if trimmed == "PODS:" {
            in_pods = true;
            continue;
        }
        if in_pods {
            if trimmed.is_empty() || !trimmed.starts_with(' ') {
                // Section ended (blank line or non-indented key like
                // DEPENDENCIES:).
                // Section hết (dòng trống hoặc key không thụt lề như
                // DEPENDENCIES:).
                break;
            }
            let entry = trimmed.trim();
            // Top-level pods are "- Name (version)" at the first indent
            // level; sub-dependencies nest deeper (more leading spaces
            // relative to the pod line). Compare indentation: top-level
            // pod lines carry exactly the "- " bullet.
            // Pod cấp gốc là "- Name (version)" thụt lề đầu tiên; dep
            // con thụt sâu hơn. So sánh thụt lề: dòng pod gốc mang
            // đúng bullet "- ".
            let indent = line.len() - line.trim_start().len();
            if indent > 2 {
                // Deeper-indented lines are sub-deps of the enclosing
                // pod — their versions belong to the parent listing,
                // not separate pins.
                // Dòng thụt sâu hơn là dep con của pod bao quanh —
                // version của chúng thuộc listing cha, không phải ghim
                // riêng.
                continue;
            }
            let Some(entry) = entry.strip_prefix("- ") else {
                skipped.push(format!("{entry}: PODS entry must start with '- '"));
                continue;
            };
            // Top-level pod: "Name (1.2.3)" or "Name/Sub (1.2.3)" or
            // "Name/Variant (1.2.3):" with a trailing dep list colon.
            // Pod cấp gốc: "Name (1.2.3)" / "Name/Sub (1.2.3)" /
            // "Name/Variant (1.2.3):" kèm dấu hai chấm danh sách dep.
            let entry = entry.trim_end_matches(':');
            let Some(open) = entry.rfind('(') else {
                skipped.push(format!("{entry}: no version in Podfile.lock entry"));
                continue;
            };
            let Some(close) = entry[open..].find(')').map(|i| open + i) else {
                skipped.push(format!("{entry}: malformed version parens"));
                continue;
            };
            let name = entry[..open].trim().to_string();
            let version = entry[open + 1..close].trim().to_string();
            if version.contains(':') || version.contains("git") {
                skipped.push(format!("{name}: local/git pod has no advisory mapping"));
                continue;
            }
            if name.is_empty() || version.is_empty() {
                skipped.push(format!("{entry}: empty name or version"));
                continue;
            }
            pins.push(OsvPin {
                name,
                version,
                ecosystem: "pods",
            });
        }
    }
    Ok((pins, skipped))
}

/// CocoaPods audit: OSV.dev carries NO CocoaPods ecosystem (verified
/// live 2026-09-10: `{"ecosystem":"pods"}` → 400 "invalid ecosystem").
/// The lockfile parse stays (inventory + skip reasons), but the lane is
/// honestly unsupported — no advisory database exists to query yet.
/// Audit CocoaPods: OSV.dev KHÔNG có ecosystem CocoaPods (đã verify
/// sống: "pods" → 400). Parse lockfile giữ nguyên (kiểm kê + lý do
/// skip), nhưng lane trung thực unsupported — chưa có database
/// advisory để truy vấn.
pub async fn audit_cocoapods_osv(project_root: &Path) -> MgResult<AuditReport> {
    let lock_path = project_root.join("Podfile.lock");
    if !lock_path.is_file() {
        return Ok(AuditReport::unsupported_ecosystem(
            "objc/cocoapods (no Podfile.lock found — run pod install first)",
        ));
    }
    let raw = std::fs::read_to_string(&lock_path)
        .map_err(|e| MgError::Other(format!("read Podfile.lock: {e}")))?;
    let (pins, skipped) = read_podfile_lock(&raw)?;
    // No advisory database to query (verified live: OSV rejects the
    // "pods" ecosystem) — report the parsed inventory honestly as
    // unsupported with the pod count and skip reasons.
    // Không có database advisory để truy vấn (đã verify sống: OSV từ
    // chối ecosystem "pods") — báo kiểm kê đã parse trung thực là
    // unsupported kèm số pod và lý do skip.
    let mut reasons = vec![format!(
        "cocoapods: {} pods pinned in Podfile.lock — OSV.dev carries no CocoaPods advisory ecosystem (verified: 'pods' rejected by the live API)",
        pins.len()
    )];
    reasons.extend(skipped);
    Ok(AuditReport {
        packages_audited: pins.len(),
        vulnerability_count: 0,
        vulnerabilities: vec![],
        scanner_status: ScannerStatus::UnsupportedEcosystem {
            ecosystem: format!(
                "objc/cocoapods ({} pods; {} unparsable entries)",
                pins.len(),
                reasons.len() - 1
            ),
        },
    })
}
