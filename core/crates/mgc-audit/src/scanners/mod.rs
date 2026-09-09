//! `scanners/mod.rs` — cargo-audit + pip-audit scanners, shared across adapters.
//! Uses cargo-audit for Rust, pip-audit for Python.
//! Scanner audit Rust/Python — cargo-audit cho Rust, pip-audit cho Python.

use mgc_types::adapter::{AuditReport, Vulnerability, VulnerabilitySeverity};
use mgc_types::{MgError, MgResult};
use serde::Deserialize;
use std::path::Path;

/// Exit codes that mean "audit ran fine" for each scanner tool.
/// Finding-vs-error exit contract lives in ONE place per tool (RULE §12).
/// Exit code nghĩa là "tool chạy xong" — contract từng tool gom một chỗ.
const CARGO_AUDIT_OK_EXIT_CODES: [i32; 2] = [0, 1];
const PIP_AUDIT_OK_EXIT_CODES: [i32; 2] = [0, 1];

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
    let deps: Vec<PipAuditDependency> = serde_json::from_str(json[json_start..].trim())
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

/// Audit Rust dependencies using cargo-audit.
/// Audit dependencies Rust dùng cargo-audit.
///
/// Requires cargo-audit to be installed: `cargo install cargo-audit`
pub async fn audit_rust(project_root: &Path) -> MgResult<AuditReport> {
    // Check if cargo-audit is available
    // Kiểm tra cargo-audit có sẵn không
    if which::which("cargo-audit").is_err() {
        return Ok(AuditReport::tool_missing(
            "cargo-audit",
            "cargo install cargo-audit --locked",
        ));
    }

    let args = vec!["audit".to_string(), "--json".to_string()];

    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        // cargo-audit exits 1 when findings exist — that is success.
        // cargo-audit thoát 1 khi có finding — đó là thành công.
        allowed_exit_codes: CARGO_AUDIT_OK_EXIT_CODES.to_vec(),
        ..Default::default()
    };

    let result = match mgc_exec::run::run("cargo", &args, &exec_opts) {
        Ok(result) => result,
        Err(err) => {
            let message = err.to_string();
            if cargo_audit_environment_unavailable(&message) {
                return Ok(AuditReport::scanner_failed(
                    "cargo-audit",
                    format!("environment unavailable: {message}"),
                ));
            }
            return Err(MgError::Other(format!("cargo audit failed: {message}")));
        }
    };

    // Unexpected exit codes still mean failure even with the escape hatch.
    // Exit code ngoài tập cho phép vẫn là thất bại dù có escape hatch.
    if !CARGO_AUDIT_OK_EXIT_CODES.contains(&result.exit_code) {
        if cargo_audit_environment_unavailable(&result.stderr_tail) {
            return Ok(AuditReport::scanner_failed(
                "cargo-audit",
                format!("environment unavailable: {}", result.stderr_tail),
            ));
        }
        return Err(MgError::Other(format!(
            "cargo audit exited with code {}",
            result.exit_code
        )));
    }

    // Parse the real scanner output — compact `--json` payload survives the
    // bounded byte capture. Fail closed on parse errors: never a fake clean.
    // Parse output thật — payload `--json` compact sống sót qua capture giới
    // hạn byte. Lỗi parse thì fail-closed — không bao giờ trả sạch giả.
    let parsed = parse_cargo_audit_json(&result.stdout_tail)?;
    Ok(AuditReport {
        packages_audited: parsed.packages_audited,
        vulnerability_count: parsed.vulnerabilities.len(),
        vulnerabilities: parsed.vulnerabilities,
        scanner_status: mgc_types::adapter::ScannerStatus::Available,
    })
}

fn cargo_audit_environment_unavailable(stderr: &str) -> bool {
    stderr.contains("failed to obtain lock file")
        || stderr.contains("couldn't fetch advisory database")
        || stderr.contains("Permission denied")
}

/// First pinned dependency file pip-audit can consume: `requirements.txt`
/// then `requirements*.txt` variants (dev/constraints etc.).
/// File dependency đã ghim đầu tiên mà pip-audit đọc được: `requirements.txt`
/// rồi tới các biến thể `requirements*.txt` (dev/constraints...).
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

/// First PEP 751 lockfile: `pylock.toml` canonical, then `pylock.*.toml`
/// variants (pylock.dev.toml, pylock.production.toml, ...) — pip-audit's
/// `--locked` discovers them from the project dir (Tech Lead P0-2).
/// Lockfile PEP 751 đầu tiên: `pylock.toml` chuẩn, rồi các biến thể
/// `pylock.*.toml` — pip-audit --locked tự khám phá từ thư mục project.
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

/// Audit Python dependencies using pip-audit — the PROJECT's dependency
/// set, never the ambient Python environment (Tech Lead P0-3 2026-09-09).
/// Audit dependencies Python bằng pip-audit — tập dependency của PROJECT,
/// không bao giờ audit environment Python ngoài.
///
/// Resolution routing (fail-closed when nothing is auditable):
/// - `requirements*.txt`  → `pip-audit -r <file>` (pinned dep set)
/// - `pylock.toml` or any PEP 751 `pylock.*.toml` (pylock.dev.toml,
///   pylock.production.toml, ...) → `pip-audit --locked <project_root>`
///   (the official CLI contract for lockfile audits)
/// - `uv.lock` → Failed with guidance (pip-audit cannot read uv
///   lockfiles) until a uv-aware scanner lands
/// - `pyproject.toml` WITHOUT a lockfile → Failed: dependencies are not
///   resolved — auditing the ambient environment would prove nothing
///   about this project.
/// - none of the above → Failed with real guidance.
///
/// Định tuyến resolve (fail-closed khi không có gì audit được):
/// - requirements*.txt → pip-audit -r <file> (tập dep đã ghim)
/// - pylock.toml hoặc bất kỳ pylock.*.toml chuẩn PEP 751 (pylock.dev.toml,
///   pylock.production.toml...) → pip-audit --locked <project_root>
///   (CLI chính thức để audit lockfile)
/// - uv.lock → Failed kèm hướng dẫn (pip-audit không đọc uv) tới khi có
///   scanner hiểu uv
/// - pyproject.toml KHÔNG lockfile → Failed: dependency chưa resolve —
///   audit environment ngoài không chứng minh gì cho project này.
/// - không có gì → Failed kèm hướng dẫn thật.
pub async fn audit_python(project_root: &Path) -> MgResult<AuditReport> {
    if which::which("pip-audit").is_err() {
        return Ok(AuditReport::tool_missing(
            "pip-audit",
            "pip install pip-audit (official PyPA vulnerability scanner)",
        ));
    }

    // Pick the audit target — first match wins; explicit over implicit.
    // Chọn target audit — cái nào có trước dùng cái đó; tường minh hơn ngầm định.
    let requirements = find_requirements_file(project_root);
    let pylock = find_pylock_file(project_root);
    let uv_lock = project_root.join("uv.lock");

    let target_args: Vec<String> = if let Some(req) = requirements {
        vec!["-r".to_string(), req]
    } else if pylock.is_some() {
        // Official pip-audit contract for PEP 751 lockfiles: audit the
        // project directory with --locked (NOT -r on a TOML file).
        // Hợp đồng chính thức của pip-audit cho lockfile PEP 751: audit
        // thư mục project bằng --locked (KHÔNG phải -r trên file TOML).
        vec!["--locked".to_string(), ".".to_string()]
    } else if uv_lock.exists() {
        return Ok(AuditReport::scanner_failed(
            "pip-audit",
            "uv.lock found but pip-audit cannot read uv lockfiles — run `uv export --format requirements-txt > requirements.txt` or await uv-native audit support",
        ));
    } else if project_root.join("pyproject.toml").exists() {
        return Ok(AuditReport::scanner_failed(
            "pip-audit",
            "pyproject.toml has no resolved dependency set — generate a PEP 751 lockfile (pylock.toml) or requirements.txt so the audit targets THIS project, not the ambient environment",
        ));
    } else {
        return Ok(AuditReport::scanner_failed(
            "pip-audit",
            "no Python dependency manifest found (requirements*.txt, pylock*.toml, uv.lock, pyproject.toml)",
        ));
    };

    let mut args = vec!["--format".to_string(), "json".to_string()];
    args.extend(target_args);
    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        // pip-audit exits 1 when vulnerabilities are found.
        // pip-audit thoát 1 khi tìm thấy lỗ hổng.
        allowed_exit_codes: PIP_AUDIT_OK_EXIT_CODES.to_vec(),
        ..Default::default()
    };

    let result = mgc_exec::run::run("pip-audit", &args, &exec_opts)
        .map_err(|e| MgError::Other(format!("pip-audit failed: {e}")))?;

    // With allowed_exit_codes=[0,1] any Err (and any exit outside {0,1})
    // is a genuine audit ERROR, not findings — fail closed with the reason.
    // Với allowed_exit_codes=[0,1], mọi Err (và exit ngoài {0,1}) là LỖI
    // audit thật, không phải finding — fail-closed kèm lý do.
    if !PIP_AUDIT_OK_EXIT_CODES.contains(&result.exit_code) {
        return Err(MgError::Other(format!(
            "pip-audit exited with code {}",
            result.exit_code
        )));
    }

    let parsed = parse_pip_audit_json(&result.stdout_tail)?;
    Ok(AuditReport {
        packages_audited: parsed.packages_audited,
        vulnerability_count: parsed.vulnerabilities.len(),
        vulnerabilities: parsed.vulnerabilities,
        scanner_status: mgc_types::adapter::ScannerStatus::Available,
    })
}
