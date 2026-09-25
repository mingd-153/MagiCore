//! `scanners/mod.rs` — MGC-owned OSV scanners plus Bun/Deno lockfile
//! readers, policy scanners, and legacy report parsers.
//! Scanner OSV do MGC sở hữu, bộ đọc lock Bun/Deno và scanner policy.

pub mod bun_deno;
pub mod dart;
pub mod dotnet;
pub mod gh_actions;
pub mod govulncheck;
pub mod maven;
pub mod osv;
pub mod rust;
pub mod terraform;

pub use bun_deno::{BunDenoRead, NpmPin, read_bun_lock, read_deno_lock, read_js_lockfiles};
pub use dart::{audit_flutter_osv, read_pubspec_lock};
pub use dotnet::{audit_dotnet, collect_dotnet_pins, read_packages_lock, read_solution_projects};
pub use gh_actions::{WorkflowFinding, audit_github_actions, scan_workflow_text};
pub use govulncheck::{audit_go, parse_govulncheck_json, read_go_mod_requires};
pub use maven::{audit_java, read_gradle_verification_metadata, read_pom_gavs};
pub use osv::{
    OsvPin, audit_cocoapods_osv, audit_osv_pins, audit_swift_spam, read_podfile_lock,
    read_swift_resolved, swift_resolved_path,
};
pub use rust::{audit_rust, read_cargo_lock_pins};
pub use terraform::{audit_terraform_lock, parse_terraform_lock};

use mgc_types::adapter::{AuditReport, FindingClass, Vulnerability, VulnerabilitySeverity};
use mgc_types::{MgError, MgResult};
use serde::Deserialize;
use std::path::Path;

// ---------------------------------------------------------------------------
// Typed serde schemas — mirror the REAL tool output, verified against
// `cargo audit --json --no-fetch` (cargo-audit 0.22.2, 2026-09-09) and the
// official pip-audit JSON format. Parsers fail closed on schema mismatch.
// Schema typed — khớp output THẬT của tool, kiểm chứng bằng cargo-audit
// 0.22.2 chạy thật; lệch schema thì fail-closed.
// ---------------------------------------------------------------------------

/// Top-level `cargo audit --json` report. Unknown top-level keys
/// (database/settings/warnings) are allowed — cargo-audit adds report
/// metadata over time; nested finding schemas stay strict so a real
/// schema change inside `advisory`/`package` still fails closed.
/// Report top-level `cargo audit --json`. Cho phép key lạ ở top-level
/// (database/settings/warnings) — cargo-audit thêm metadata theo phiên
/// bản; schema finding lồng bên trong vẫn chặt để thay đổi thật trong
/// advisory/package vẫn fail-closed.
#[derive(Debug, Deserialize)]
struct CargoAuditReport {
    lockfile: Option<CargoLockfile>,
    vulnerabilities: CargoVulnerabilities,
}

#[derive(Debug, Deserialize)]
struct CargoLockfile {
    #[serde(rename = "dependency-count")]
    dependency_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct CargoVulnerabilities {
    found: bool,
    count: Option<usize>,
    #[serde(default)]
    list: Vec<CargoAuditFinding>,
}

/// One finding in `vulnerabilities.list` — advisory metadata lives INSIDE
/// `advisory`, version info in `package`, fix range in `versions`. Unknown
/// sibling keys (`affected` etc.) are tolerated: the strict inner structs
/// catch the schema changes that matter.
/// Một finding trong `vulnerabilities.list` — metadata nằm TRONG advisory,
/// version trong `package`, range fix trong `versions`. Key anh em lạ
/// (`affected`...) được dung thứ: struct con chặt vẫn bắt được thay đổi
/// schema quan trọng.
#[derive(Debug, Deserialize)]
struct CargoAuditFinding {
    advisory: CargoAdvisory,
    versions: Option<CargoVersions>,
    package: CargoPackage,
}

/// Advisory metadata — strictness lives in the REQUIRED fields below; new
/// advisory keys from future cargo-audit versions are ignored so version
/// bumps do not fake-error real audits (deny_unknown_fields removed 2026-09-09
/// after real output carried `package`, `date`, `references`...).
/// Metadata advisory — tính chặt nằm ở các field BẮT BUỘC dưới đây; key mới
/// của cargo-audit sau này bị bỏ qua để nâng phiên bản không gây lỗi giả
/// (bỏ deny_unknown_fields 2026-09-09 vì output thật có package, date,
/// references...).
#[derive(Debug, Deserialize)]
struct CargoAdvisory {
    id: String,
    title: String,
    /// CVSS vector (e.g. "CVSS:3.1/AV:N/AC:H/...") — cargo-audit reports the
    /// vector, not a severity label; parsed via CVSS base metrics.
    /// Vector CVSS — cargo-audit báo vector chứ không phải nhãn severity;
    /// parse bằng base metrics CVSS.
    cvss: Option<String>,
    #[serde(default)]
    severity: Option<String>,
    url: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CargoVersions {
    #[serde(default)]
    patched: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: String,
    version: String,
}

/// Parsed cargo-audit result: findings + the real dependency count.
/// Kết quả parse cargo-audit: finding + số dependency thật đã audit.
pub struct CargoAuditParse {
    pub vulnerabilities: Vec<Vulnerability>,
    pub packages_audited: usize,
}

/// Public CVSS bridge for sibling scanner modules (govulncheck) — ONE
/// CVSS implementation for every ecosystem (RULE: no copy-paste logic).
/// Cầu CVSS public cho các module scanner anh em (govulncheck) — MỘT bản
/// implement CVSS cho mọi ecosystem (RULE: không copy-paste logic).
pub(crate) fn cvss_base_severity_for_go(vector: &str) -> Option<VulnerabilitySeverity> {
    cvss_base_severity(vector)
}

/// Parse cargo-audit JSON output (real schema, fail-closed).
/// Parse JSON output của cargo-audit (schema thật, fail-closed).
///
/// Format: https://github.com/rustsec/rustsec/blob/main/cargo-audit/README.md#json-output
pub fn parse_cargo_audit_json(json: &str) -> MgResult<CargoAuditParse> {
    let json_start = json.find('{').unwrap_or(0);
    let report: CargoAuditReport = serde_json::from_str(json[json_start..].trim())
        .map_err(|e| MgError::Other(format!("invalid cargo-audit JSON: {e}")))?;

    let packages_audited = report
        .lockfile
        .and_then(|l| l.dependency_count)
        .unwrap_or(0);

    let mut vulns = Vec::new();
    for finding in &report.vulnerabilities.list {
        // Malformed finding → schema error, NEVER silently skipped: a dropped
        // security finding is a false-clean report.
        // Finding lỗi schema → error, KHÔNG bỏ âm thầm: mất finding bảo
        // mật đồng nghĩa báo sạch giả.
        let Ok(pkg_name) = mgc_types::PackageName::new(&finding.package.name) else {
            return Err(MgError::Other(format!(
                "cargo-audit finding for '{}' has a package name mgc cannot represent",
                finding.package.name
            )));
        };
        let Ok(version) = mgc_types::Version::parse(&finding.package.version) else {
            return Err(MgError::Other(format!(
                "cargo-audit finding for '{}' has an unparseable version '{}'",
                finding.package.name, finding.package.version
            )));
        };
        let (severity_str, severity_level) = cargo_severity(finding);
        vulns.push(
            Vulnerability {
                package: mgc_types::PackageId::new(pkg_name, version),
                title: format!("{}: {}", finding.advisory.id, finding.advisory.title),
                severity: severity_str,
                cve: finding
                    .advisory
                    .aliases
                    .iter()
                    .find(|a| a.starts_with("CVE-"))
                    .cloned()
                    .unwrap_or_else(|| finding.advisory.id.clone()),
                severity_level,
                patched_versions: (!finding
                    .versions
                    .as_ref()
                    .map(|v| v.patched.is_empty())
                    .unwrap_or(true))
                .then(|| {
                    finding
                        .versions
                        .as_ref()
                        .map(|v| v.patched.join(", "))
                        .unwrap_or_default()
                }),
                url: finding.advisory.url.clone(),
                scanner: None,
                ecosystem: None,
                evidence_at: None,
                finding_class: FindingClass::Vulnerability,
            }
            .with_evidence("cargo-audit", "rust"),
        );
    }

    // Cross-check: `count` (when present) must match the list we parsed — a
    // mismatch means an upstream schema change, fail closed instead of
    // reporting an incomplete result.
    // Đối chiếu: `count` (nếu có) phải khớp list đã parse — lệch nghĩa là
    // upstream đổi schema, fail-closed thay vì báo thiếu.
    if let Some(count) = report.vulnerabilities.count
        && count != report.vulnerabilities.list.len()
    {
        return Err(MgError::Other(format!(
            "cargo-audit JSON inconsistent: count says {} but list holds {}",
            count,
            report.vulnerabilities.list.len()
        )));
    }
    if report.vulnerabilities.found != !report.vulnerabilities.list.is_empty() {
        return Err(MgError::Other(
            "cargo-audit JSON inconsistent: found flag disagrees with list contents".to_string(),
        ));
    }

    Ok(CargoAuditParse {
        vulnerabilities: vulns,
        packages_audited,
    })
}

/// Map cargo-audit advisory severity: `advisory.severity` label wins; else
/// derive from the CVSS vector's base metrics; else Info (honest default).
/// Map severity: ưu tiên `advisory.severity`; nếu không có thì tính từ CVSS
/// vector; còn lại Info (trung thực).
fn cargo_severity(finding: &CargoAuditFinding) -> (String, VulnerabilitySeverity) {
    if let Some(label) = &finding.advisory.severity
        && !label.is_empty()
    {
        let level = VulnerabilitySeverity::from_str(label);
        return (label.to_lowercase(), level);
    }
    if let Some(level) = finding
        .advisory
        .cvss
        .as_deref()
        .and_then(cvss_base_severity)
    {
        let label = level.as_str().to_string();
        return (label.clone(), level);
    }
    ("info".to_string(), VulnerabilitySeverity::Info)
}

/// Extract base severity from a CVSS v3 vector string — implement the CVSS
/// v3.1 base-score formula (ISS → ISC → Exploitability → roundup bands)
/// exactly as the official calculator does. No external crate: keeps the
/// lockfile stable and the mapping auditable in one place.
/// Trích severity từ chuỗi CVSS v3 — thực thi công thức base-score CVSS
/// v3.1 (ISS → ISC → Exploitability → roundup) đúng như calculator chính
/// thức. Không dùng crate ngoài: lockfile ổn định, mapping kiểm tra được.
fn cvss_base_severity(vector: &str) -> Option<VulnerabilitySeverity> {
    // "CVSS:3.1/AV:N/..." → drop "CVSS:" prefix, then the version segment
    // up to the FIRST '/' (the version "3.1" contains no ':' — splitting on
    // ':' would swallow the AV metric).
    // "CVSS:3.1/AV:N/..." → bỏ prefix "CVSS:", rồi đoạn version tới '/'
    // ĐẦU TIÊN (version "3.1" không chứa ':' — tách theo ':' sẽ nuốt
    // metric AV).
    let metrics: std::collections::HashMap<&str, &str> = vector
        .trim()
        .strip_prefix("CVSS:")
        .and_then(|rest| rest.split_once('/'))
        .map(|(_, body)| body.split('/').filter_map(|p| p.split_once(':')).collect())?;

    let metric = |key: &str| metrics.get(key).copied();
    let weight = |m: Option<&str>, table: &[(&str, f64)]| {
        m.and_then(|m| table.iter().find(|(k, _)| *k == m).map(|(_, w)| *w))
    };

    // CVSS v3.1 metric weights.
    let av = weight(
        metric("AV"),
        &[("N", 0.85), ("A", 0.62), ("L", 0.55), ("P", 0.2)],
    )?;
    let ac = weight(metric("AC"), &[("L", 0.77), ("H", 0.44)])?;
    let ui = weight(metric("UI"), &[("N", 0.85), ("R", 0.62)])?;
    let scope_changed = metric("S") == Some("C");
    let pr = match (metric("PR"), scope_changed) {
        (Some("N"), _) => 0.85,
        (Some("L"), false) => 0.62,
        (Some("L"), true) => 0.68,
        (Some("H"), false) => 0.27,
        (Some("H"), true) => 0.5,
        _ => return None,
    };
    let c = weight(metric("C"), &[("H", 0.56), ("L", 0.22), ("N", 0.0)])?;
    let i = weight(metric("I"), &[("H", 0.56), ("L", 0.22), ("N", 0.0)])?;
    let a = weight(metric("A"), &[("H", 0.56), ("L", 0.22), ("N", 0.0)])?;

    // Impact Sub-Score, Impact, Exploitability per CVSS v3.1 spec §7.1.
    let iss = 1.0 - (1.0 - c) * (1.0 - i) * (1.0 - a);
    let (isc, multiplier) = if scope_changed {
        (7.52 * (iss - 0.029) - 3.25 * (iss - 0.02).powi(15), 1.08)
    } else {
        (6.42 * iss, 1.0)
    };
    let exploitability = 8.22 * av * ac * pr * ui;
    if isc <= 0.0 {
        return Some(VulnerabilitySeverity::Info);
    }
    let score = (multiplier * (isc + exploitability)).min(10.0);
    Some(cvss_rating((score * 10.0).round() / 10.0))
}

/// CVSS v3.1 qualitative rating bands.
/// Bảng xếp loại định tính CVSS v3.1.
fn cvss_rating(score: f64) -> VulnerabilitySeverity {
    match score {
        s if s >= 9.0 => VulnerabilitySeverity::Critical,
        s if s >= 7.0 => VulnerabilitySeverity::High,
        s if s >= 4.0 => VulnerabilitySeverity::Medium,
        s if s > 0.0 => VulnerabilitySeverity::Low,
        _ => VulnerabilitySeverity::Info,
    }
}

/// Typed pip-audit dependency entry — official JSON format is a TOP-LEVEL
/// ARRAY of dependencies, each carrying its own `vulns` list.
/// Entry dependency của pip-audit — JSON chính thức là MẢNG TOP-LEVEL các
/// dependency, mỗi dependency có list `vulns` riêng.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PipAuditDependency {
    name: String,
    version: String,
    #[serde(default)]
    vulns: Vec<PipAuditVuln>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PipAuditVuln {
    id: String,
    #[serde(default)]
    fix_versions: Vec<String>,
    description: Option<String>,
    aliases: Option<Vec<String>>,
}

/// Parsed pip-audit result: flattened findings + dependency count audited.
/// Kết quả parse pip-audit: finding + số dependency đã audit.
pub struct PipAuditParse {
    pub vulnerabilities: Vec<Vulnerability>,
    pub packages_audited: usize,
}

/// Parse pip-audit JSON output (official schema: top-level array of
/// dependencies with nested `vulns`, fail-closed).
/// Parse JSON pip-audit (schema chính thức: mảng top-level dependency với
/// `vulns` lồng nhau, fail-closed).
///
/// Format: https://pypi.org/project/pip-audit/ (`--format json`)
pub fn parse_pip_audit_json(json: &str) -> MgResult<PipAuditParse> {
    let json_start = json
        .find('[')
        .unwrap_or_else(|| json.find('{').unwrap_or(0));
    // Stream-parse the FIRST complete JSON value from the payload:
    // pip-audit 2.9.0 on some CI runners emits trailing noise after
    // the JSON body (progress/status lines merged into stdout) —
    // serde_json's streaming deserializer consumes exactly one value
    // and ignores anything after, while still failing on a genuinely
    // broken body (fail-closed on corruption, tolerant of suffix junk).
    // Parse-stream GIÁ TRỊ JSON đầu tiên trong payload: pip-audit
    // 2.9.0 trên một số runner CI in thêm rác sau thân JSON (dòng
    // progress/status lẫn vào stdout) — deserializer streaming của
    // serde_json tiêu thụ đúng một giá trị và bỏ qua phần sau, nhưng
    // vẫn fail trên thân JSON thật sự hỏng (fail-closed với hỏng,
    // khoan dung với rác hậu tố).
    let mut stream = serde_json::Deserializer::from_str(json[json_start..].trim())
        .into_iter::<Vec<PipAuditDependency>>();
    let deps: Vec<PipAuditDependency> = stream
        .next()
        .ok_or_else(|| MgError::Other("invalid pip-audit JSON: empty payload".to_string()))?
        .map_err(|e| MgError::Other(format!("invalid pip-audit JSON: {e}")))?;

    let mut vulns = Vec::new();
    let mut rejected = Vec::new();
    for dep in &deps {
        // Skip editable/vcs entries pip-audit reports as `name: null`-style
        // shells — count them as audited, they carry no advisories.
        let Ok(pkg_name) = mgc_types::PackageName::new(&dep.name) else {
            rejected.push(format!("dependency '{}'", dep.name));
            continue;
        };
        let Ok(version) = mgc_types::Version::parse(&dep.version) else {
            rejected.push(format!(
                "dependency '{}' version '{}'",
                dep.name, dep.version
            ));
            continue;
        };
        for vuln in &dep.vulns {
            let description = vuln.description.as_deref().unwrap_or("");
            let title = format!("{}: {}", vuln.id, truncate_utf8(description, 200));
            // pip-audit carries no severity field — record as Info with the
            // advisory id; severity guessing would be dishonest.
            // pip-audit không có trường severity — ghi Info kèm advisory id;
            // đoán bừa severity là không trung thực.
            vulns.push(
                Vulnerability {
                    package: mgc_types::PackageId::new(pkg_name.clone(), version.clone()),
                    title,
                    severity: "info".to_string(),
                    cve: vuln
                        .aliases
                        .as_ref()
                        .and_then(|a| a.iter().find(|x| x.starts_with("CVE-")).cloned())
                        .unwrap_or_else(|| vuln.id.clone()),
                    severity_level: VulnerabilitySeverity::Info,
                    patched_versions: (!vuln.fix_versions.is_empty())
                        .then(|| vuln.fix_versions.join(", ")),
                    url: None,
                    scanner: None,
                    ecosystem: None,
                    evidence_at: None,
                    finding_class: FindingClass::Vulnerability,
                }
                .with_evidence("pip-audit", "python"),
            );
        }
    }

    // Any unparseable dependency is a schema problem, not clean data —
    // surface it instead of dropping findings silently.
    // Dependency không parse được là lỗi schema, không phải dữ liệu sạch —
    // nêu rõ thay vì bỏ finding âm thầm.
    if !rejected.is_empty() {
        return Err(MgError::Other(format!(
            "pip-audit JSON contained {} unparseable entries: {}",
            rejected.len(),
            rejected.join("; ")
        )));
    }

    Ok(PipAuditParse {
        vulnerabilities: vulns,
        packages_audited: deps.len(),
    })
}

/// Truncate to at most `max_chars` chars, respecting char (not byte)
/// boundaries — advisory text is external data and may contain any UTF-8.
/// Cắt tối đa `max_chars` ký tự, đúng ranh giới ký tự (không phải byte) —
/// text advisory là dữ liệu ngoài, có thể chứa UTF-8 bất kỳ.
pub fn truncate_utf8(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

/// Find the canonical requirements file first, then sorted variants.
/// Tìm requirements chuẩn trước, sau đó đến các biến thể theo thứ tự ổn định.
pub fn find_requirements_file(project_root: &Path) -> Option<String> {
    let canonical = project_root.join("requirements.txt");
    if canonical.is_file() {
        return Some("requirements.txt".to_string());
    }
    let mut variants: Vec<String> = std::fs::read_dir(project_root)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| n.starts_with("requirements") && n.ends_with(".txt"))
        .collect();
    variants.sort();
    variants.into_iter().next()
}

/// Find the canonical PEP 751 lockfile first, then sorted named variants.
/// Tìm lockfile PEP 751 chuẩn trước, rồi đến biến thể có tên theo thứ tự.
pub fn find_pylock_file(project_root: &Path) -> Option<String> {
    let canonical = project_root.join("pylock.toml");
    if canonical.is_file() {
        return Some("pylock.toml".to_string());
    }
    let mut variants: Vec<String> = std::fs::read_dir(project_root)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| n == "pylock.toml" || (n.starts_with("pylock.") && n.ends_with(".toml")))
        .collect();
    variants.sort();
    variants.into_iter().next()
}

/// Read MGC-owned Python pins. `None` means no mgc.lock exists; a present but
/// malformed lock is an error and an empty Python graph remains authoritative.
/// Đọc pin Python do MGC sở hữu. `None` là chưa có mgc.lock; lock hỏng là lỗi,
/// còn graph Python rỗng trong lock hợp lệ vẫn là nguồn dữ liệu có thẩm quyền.
pub fn python_pins_from_mgc_lock(project_root: &Path) -> MgResult<Option<Vec<osv::OsvPin>>> {
    let path = project_root.join("mgc.lock");
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(MgError::Other(format!(
                "cannot read MGC lockfile for Python audit at {}: {error}",
                path.display()
            )));
        }
    };
    let lockfile = mgc_lockfile::parser::parse_lockfile(&content)
        .map_err(|error| MgError::Other(format!("invalid mgc.lock for Python audit: {error}")))?;
    Ok(Some(
        lockfile
            .packages
            .iter()
            .filter(|p| p.ecosystem == mgc_lockfile::EcosystemTag::Python)
            .map(|p| osv::OsvPin {
                name: p.name.clone(),
                version: p.version.clone(),
                ecosystem: "PyPI",
            })
            .collect(),
    ))
}

const PYPI_SIMPLE_INDEX: &str = "https://pypi.org/simple";

/// Read uv.lock package pins; unknown registries remain explicitly skipped.
/// Đọc pin từ uv.lock; registry không biết phải được ghi nhận là skipped.
pub fn read_uv_lock_pins(raw: &str) -> MgResult<(Vec<osv::OsvPin>, Vec<String>)> {
    let document: toml::Value = toml::from_str(raw)
        .map_err(|error| MgError::Other(format!("invalid uv.lock TOML: {error}")))?;
    if document.get("version").and_then(toml::Value::as_integer) != Some(1)
        || document
            .get("revision")
            .and_then(toml::Value::as_integer)
            .is_none()
    {
        return Err(MgError::Other(
            "uv.lock has an unsupported or incomplete schema version".to_string(),
        ));
    }
    let packages = match document.get("package") {
        Some(value) => value
            .as_array()
            .ok_or_else(|| MgError::Other("uv.lock package field is not an array".to_string()))?,
        None => return Ok((Vec::new(), Vec::new())),
    };
    let mut pins = std::collections::BTreeSet::new();
    let mut skipped = Vec::new();
    if document
        .get("resolution-markers")
        .and_then(toml::Value::as_array)
        .is_some_and(|markers| !markers.is_empty())
    {
        skipped.push("uv.lock contains platform resolution markers; all locked variants are queried conservatively".to_string());
    }
    for package in packages {
        let name = package
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| MgError::Other("uv.lock package has no valid name".to_string()))?;
        let version = package
            .get("version")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| {
                MgError::Other(format!("uv.lock package '{name}' has no valid version"))
            })?;
        let registry = package
            .get("source")
            .and_then(|source| source.get("registry"))
            .and_then(toml::Value::as_str);
        match registry {
            Some(url) if url.trim_end_matches('/') == PYPI_SIMPLE_INDEX => {
                pins.insert((normalize_python_name(name), version.to_string()));
            }
            Some(url) => skipped.push(format!(
                "uv.lock package '{name}@{version}' uses unsupported registry '{url}'"
            )),
            None => skipped.push(format!(
                "uv.lock package '{name}@{version}' has a non-PyPI or missing source"
            )),
        }
    }
    Ok((
        pins.into_iter()
            .map(|(name, version)| osv::OsvPin {
                name,
                version,
                ecosystem: "PyPI",
            })
            .collect(),
        skipped,
    ))
}

/// Read exact pins from requirements files; coverage is always Partial.
/// Đọc pin chính xác từ requirements; độ phủ luôn là Partial.
pub fn read_requirements_pins(project_root: &Path) -> MgResult<(Vec<osv::OsvPin>, Vec<String>)> {
    let mut paths: Vec<_> = std::fs::read_dir(project_root)
        .map_err(|error| MgError::Other(format!("read project directory: {error}")))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("requirements") && name.ends_with(".txt"))
        })
        .collect();
    paths.sort();
    let mut pins = std::collections::BTreeSet::new();
    let mut skipped =
        vec!["requirements files do not prove a complete resolved dependency graph".to_string()];
    for path in paths {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("requirements.txt");
        let raw = std::fs::read_to_string(&path)
            .map_err(|error| MgError::Other(format!("read {file_name}: {error}")))?;
        for (index, line) in raw.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() || line.starts_with("--hash=") {
                continue;
            }
            if line.starts_with('-') {
                skipped.push(format!(
                    "{file_name}:{} uses an unsupported include/option",
                    index + 1
                ));
                continue;
            }
            let requirement = line
                .split(';')
                .next()
                .unwrap_or("")
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim();
            let name_end = requirement
                .find(|ch: char| !ch.is_ascii_alphanumeric() && !"._-[]".contains(ch))
                .unwrap_or(requirement.len());
            let raw_name = &requirement[..name_end];
            let package_name = raw_name.split('[').next().unwrap_or("").trim();
            let constraint = requirement[name_end..].trim();
            let version = constraint
                .strip_prefix("===")
                .or_else(|| constraint.strip_prefix("=="))
                .map(str::trim);
            match (package_name.is_empty(), version) {
                (false, Some(version)) if !version.is_empty() && !version.contains('*') => {
                    pins.insert((normalize_python_name(package_name), version.to_string()));
                    if line.contains(';') {
                        skipped.push(format!(
                            "{file_name}:{} has an environment marker; the query includes it conservatively",
                            index + 1
                        ));
                    }
                }
                _ => skipped.push(format!(
                    "{file_name}:{} is not an exact package version pin",
                    index + 1
                )),
            }
        }
    }
    Ok((
        pins.into_iter()
            .map(|(name, version)| osv::OsvPin {
                name,
                version,
                ecosystem: "PyPI",
            })
            .collect(),
        skipped,
    ))
}

fn normalize_python_name(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    let mut separator = false;
    for ch in name.to_ascii_lowercase().chars() {
        if matches!(ch, '_' | '.' | '-') {
            separator = true;
        } else {
            if separator && !normalized.is_empty() {
                normalized.push('-');
            }
            separator = false;
            normalized.push(ch);
        }
    }
    normalized
}

/// Audit Python pins with MGC's parser and OSV client; no package-manager spawn.
/// Dùng parser và OSV client của MGC để audit Python; không spawn package manager.
pub async fn audit_python(project_root: &Path) -> MgResult<AuditReport> {
    if let Some(owned) = python_pins_from_mgc_lock(project_root)? {
        return osv::audit_osv_pins(&owned).await;
    }
    let uv_lock = project_root.join("uv.lock");
    let (pins, mut skipped, requirements_only) = if uv_lock.is_file() {
        let raw = std::fs::read_to_string(&uv_lock)
            .map_err(|error| MgError::Other(format!("read uv.lock: {error}")))?;
        let (pins, skipped) = read_uv_lock_pins(&raw)?;
        (pins, skipped, false)
    } else if find_requirements_file(project_root).is_some() {
        let (pins, skipped) = read_requirements_pins(project_root)?;
        (pins, skipped, true)
    } else {
        let reason = if project_root.join("pyproject.toml").is_file() {
            "pyproject.toml has no mgc.lock or supported resolved uv.lock; audit is unverified"
        } else if find_pylock_file(project_root).is_some() {
            "PEP 751 pylock is not yet supported by the native Python audit parser"
        } else {
            "no mgc.lock, uv.lock, or requirements*.txt found for Python audit"
        };
        return Ok(AuditReport::scanner_failed("mgc-python-osv", reason));
    };
    let mut report = osv::audit_osv_pins(&pins).await?;
    if requirements_only || !skipped.is_empty() {
        if skipped.is_empty() {
            skipped.push("Python dependency coverage is incomplete".to_string());
        }
        report.scanner_status = mgc_types::adapter::ScannerStatus::Partial {
            scanned: report.packages_audited,
            skipped: skipped.len(),
            reasons: skipped,
        };
    }
    Ok(report)
}
