//! `scanners/govulncheck.rs` — Go vulnerability scanner via govulncheck
//! (Tech Lead P1 2026-09-09 matrix row "Lib/Web Go govulncheck").
//!
//! Parser typed theo STREAMING JSON chính thức của govulncheck
//! (golang.org/x/vuln/internal/govulncheck, protocol v1.0.0): mỗi DÒNG
//! là một Message chứa đúng một field — config | progress | SBOM | osv |
//! finding. `-json` LUÔN exit 0 kể cả có finding, nên exit != 0 là lỗi
//! tool/môi trường — fail-closed.
//!
//! Typed parser for govulncheck's OFFICIAL streaming JSON: each LINE is
//! a Message carrying exactly one field. `-json` ALWAYS exits 0 even
//! with findings, so a non-zero exit is a tool/environment error —
//! fail closed.

use mgc_types::adapter::{AuditReport, ScannerStatus, Vulnerability, VulnerabilitySeverity};
use mgc_types::{MgError, MgResult, PackageId, PackageName, Version};
use serde::Deserialize;
use std::path::Path;

/// `-json` mode always exits 0 (even with findings) per the official
/// contract — anything else is a genuine tool error.
/// Chế độ `-json` luôn exit 0 (kể cả có finding) theo hợp đồng chính
/// thức — khác 0 là lỗi tool thật.
const GOVULNCHECK_OK_EXIT_CODES: [i32; 1] = [0];

/// One streaming JSON line — exactly one field is set (protocol v1.0.0).
/// The `err` field exists in the doc'd handler contract for streams that
/// carry an error object; receiving one is a scanner failure, never clean.
/// Một dòng JSON stream — đúng một field được set. Field `err` tồn tại
/// trong hợp đồng handler; nhận được là lỗi scanner, không bao giờ sạch.
#[derive(Debug, Deserialize)]
struct GovulncheckMessage {
    #[serde(default, rename = "config")]
    _config: Option<serde::de::IgnoredAny>,
    #[serde(default, rename = "progress")]
    _progress: Option<serde::de::IgnoredAny>,
    /// SBOM message — module inventory (capital "SBOM" per the official
    /// JSON tags).
    /// Thông điệp SBOM — danh sách module (viết hoa "SBOM" theo JSON tag
    /// chính thức).
    #[serde(default, rename = "SBOM")]
    sbom: Option<GovulncheckSbom>,
    /// OSV entry — advisory metadata keyed by `id`.
    /// Entry OSV — metadata advisory, khóa theo `id`.
    #[serde(default, rename = "osv")]
    osv: Option<GovulncheckOsv>,
    #[serde(default, rename = "finding")]
    finding: Option<GovulncheckFinding>,
}

/// SBOM message: the exact module set scanned — feeds packages_audited.
/// Thông điệp SBOM: đúng tập module đã quét — dùng cho packages_audited.
#[derive(Debug, Deserialize)]
struct GovulncheckSbom {
    #[serde(default)]
    modules: Vec<GovulncheckModule>,
}

/// One SBOM module — only the count matters for packages_audited; the
/// per-module path/version stay in the payload for schema completeness.
/// Một module SBOM — chỉ SỐ LƯỢNG dùng cho packages_audited; path/
/// version giữ trong payload cho đủ schema.
#[derive(Debug, Deserialize)]
struct GovulncheckModule {
    #[serde(default)]
    #[allow(dead_code)]
    path: String,
    #[serde(default)]
    #[allow(dead_code)]
    version: String,
}

/// OSV advisory entry — the subset of the OSV schema govulncheck emits.
/// Entry advisory OSV — tập con schema OSV mà govulncheck phát hành.
#[derive(Debug, Deserialize)]
struct GovulncheckOsv {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    details: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    severity: Vec<GovulncheckSeverity>,
    #[serde(default)]
    database_specific: Option<GovulncheckDbSpecific>,
    #[serde(default)]
    references: Vec<GovulncheckReference>,
}

#[derive(Debug, Deserialize)]
struct GovulncheckSeverity {
    /// CVSS vector string ("CVSS:3.1/AV:...").
    /// Chuỗi vector CVSS ("CVSS:3.1/AV:...").
    score: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GovulncheckDbSpecific {
    /// Go vulndb severity label: HIGH | MODERATE | LOW | (TEMPORARY...).
    /// Nhãn severity của Go vulndb: HIGH | MODERATE | LOW | (TEMPORARY...).
    #[serde(default)]
    severity: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GovulncheckReference {
    #[serde(default)]
    url: Option<String>,
}

/// One finding: `osv` id + trace; the FIRST trace frame carries the
/// affected module/version.
/// Một finding: id `osv` + trace; frame ĐẦU của trace chứa module/version
/// bị dính.
#[derive(Debug, Deserialize)]
struct GovulncheckFinding {
    #[serde(default)]
    osv: String,
    #[serde(default)]
    fixed_version: Option<String>,
    #[serde(default)]
    trace: Vec<GovulncheckFrame>,
}

#[derive(Debug, Deserialize)]
struct GovulncheckFrame {
    #[serde(default)]
    module: String,
    #[serde(default)]
    version: String,
}

/// Parsed govulncheck stream: deduped findings + module count.
/// Kết quả parse stream govulncheck: finding khử trùng + số module.
pub struct GovulncheckParse {
    pub vulnerabilities: Vec<Vulnerability>,
    pub packages_audited: usize,
}

/// Parse the govulncheck JSON stream (fail-closed): findings join their
/// OSV entries by id; every finding WITHOUT a matching OSV entry is a
/// contract violation, never a skip. Accepts BOTH encodings: pretty
/// (multi-line objects, govulncheck ≥1.8) and one-message-per-line —
/// StreamDeserializer walks concatenated JSON values regardless of
/// whitespace.
/// Parse stream JSON govulncheck (fail-closed): finding nối với entry
/// OSV theo id; finding MÀ KHÔNG có entry OSV tương ứng là vi phạm hợp
/// đồng, không bỏ. Nhận CẢ HAI kiểu: pretty (object nhiều dòng,
/// govulncheck ≥1.8) và mỗi-message-mỗi-dòng — StreamDeserializer đọc
/// chuỗi JSON nối tiếp bất kể khoảng trắng.
pub fn parse_govulncheck_json(raw: &str) -> MgResult<GovulncheckParse> {
    let mut osv_entries: std::collections::HashMap<String, GovulncheckOsv> =
        std::collections::HashMap::new();
    let mut findings: Vec<GovulncheckFinding> = Vec::new();
    let mut packages_audited = 0usize;

    let stream = serde_json::Deserializer::from_str(raw).into_iter::<GovulncheckMessage>();
    for message in stream {
        let message =
            message.map_err(|e| MgError::Other(format!("invalid govulncheck JSON: {e}")))?;

        if let Some(sbom) = message.sbom {
            packages_audited = sbom.modules.len();
        }
        if let Some(osv) = message.osv {
            osv_entries.insert(osv.id.clone(), osv);
        }
        if let Some(finding) = message.finding {
            findings.push(finding);
        }
    }

    // Every finding MUST reference a known OSV id — a dangling reference
    // means the stream was truncated or malformed: fail closed.
    // Mọi finding PHẢI trỏ tới id OSV đã thấy — reference lơ lửng nghĩa
    // là stream bị cắt/malformed: fail-closed.
    let mut vulnerabilities: Vec<Vulnerability> = Vec::new();
    let mut seen_osv_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for finding in &findings {
        if finding.osv.is_empty() {
            return Err(MgError::Other(
                "govulncheck finding without an OSV id — stream contract violation".to_string(),
            ));
        }
        // Dedupe: one OSV id → one row (govulncheck emits module-, package-
        // AND symbol-level findings for the same vulnerability).
        // Khử trùng: một id OSV → một dòng (govulncheck phát finding ở cả
        // level module/package/symbol cho cùng một lỗ hổng).
        if !seen_osv_ids.insert(finding.osv.clone()) {
            continue;
        }
        let osv = osv_entries.get(&finding.osv).ok_or_else(|| {
            MgError::Other(format!(
                "govulncheck finding references OSV '{}' that never appeared in the stream",
                finding.osv
            ))
        })?;

        // First trace frame names the affected module + version; Go module
        // versions may be pseudo-versions ("v1.0.0-20200101...") that are
        // NOT plain semver — keep the finding with a 0.0.0 carrier and
        // the real version recorded in the title (never dropped).
        // Frame đầu của trace nêu module + version dính; version module Go
        // có thể là pseudo-version ("v1.0.0-20200101...") KHÔNG phải
        // semver thuần — giữ finding với carrier 0.0.0 và version thật ghi
        // trong title (không bao giờ bỏ).
        let frame = finding.trace.first();
        let module_path = frame.map(|f| f.module.as_str()).unwrap_or("");
        let module_version = frame.map(|f| f.version.as_str()).unwrap_or("");
        let version = Version::parse(module_version.trim_start_matches('v'))
            .unwrap_or_else(|_| Version::new(0, 0, 0));

        // Go module paths contain multiple slashes ("golang.org/x/text")
        // that PackageName (npm-scoped, 1 slash) cannot hold. Keep the
        // FULL path in the title + use the LAST NON-EMPTY path segment as
        // the display name — the finding survives with all its
        // information, an empty/missing path degrades to an honest
        // "unknown-module" carrier.
        // Đường dẫn module Go chứa nhiều slash ("golang.org/x/text") mà
        // PackageName (npm-scoped, 1 slash) không giữ được. Giữ path
        // ĐẦY ĐỦ trong title + dùng đoạn CUỐI KHÁC RỖNG làm tên hiển
        // thị — finding sống sót với trọn thông tin; path rỗng/thiếu thì
        // rơi về carrier "unknown-module" trung thực.
        let display_name = module_path
            .rsplit('/')
            .find(|s| !s.is_empty())
            .unwrap_or("unknown-module");
        let Ok(pkg_name) = PackageName::new(display_name.to_string()) else {
            return Err(MgError::Other(format!(
                "govulncheck module path '{module_path}' cannot be represented as a package name"
            )));
        };

        let summary = if osv.summary.is_empty() {
            osv.details.clone()
        } else {
            osv.summary.clone()
        };
        // Real module version rides the title when it is not plain semver.
        // Version module thật ghi trong title khi nó không phải semver thuần.
        let title_suffix = if module_path.is_empty() {
            String::new()
        } else if module_version.is_empty() {
            format!(" (module {module_path})")
        } else {
            format!(" (module {module_path}@{module_version})")
        };
        let (severity_str, severity_level) = govulncheck_severity(osv);

        vulnerabilities.push(
            Vulnerability {
                package: PackageId::new(pkg_name, version),
                title: format!(
                    "{}: {}{}",
                    osv.id,
                    truncate_chars(&summary, 200),
                    title_suffix
                ),
                severity: severity_str,
                cve: osv
                    .aliases
                    .iter()
                    .find(|a| a.starts_with("CVE-"))
                    .cloned()
                    .unwrap_or_else(|| osv.id.clone()),
                severity_level,
                patched_versions: finding
                    .fixed_version
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .map(str::to_string),
                url: osv
                    .references
                    .iter()
                    .find_map(|r| r.url.clone())
                    .or_else(|| Some(format!("https://pkg.go.dev/vuln/{}", osv.id))),
                scanner: None,
                ecosystem: None,
                evidence_at: None,
            }
            .with_evidence("govulncheck", "go"),
        );
    }

    Ok(GovulncheckParse {
        vulnerabilities,
        packages_audited,
    })
}

/// Severity mapping: Go vulndb label wins (HIGH/MODERATE/LOW); else derive
/// from a CVSS vector; else Info — honest default, no guessing.
/// Map severity: ưu tiên nhãn Go vulndb; còn lại tính từ vector CVSS;
/// không có thì Info — mặc định trung thực, không đoán.
fn govulncheck_severity(osv: &GovulncheckOsv) -> (String, VulnerabilitySeverity) {
    if let Some(label) = osv
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
        return (label.to_uppercase().to_lowercase(), level);
    }
    if let Some(level) = osv
        .severity
        .iter()
        .find_map(|s| s.score.as_deref())
        .and_then(crate::scanners::cvss_base_severity_for_go)
    {
        let label = level.as_str().to_string();
        return (label, level);
    }
    ("info".to_string(), VulnerabilitySeverity::Info)
}

/// Truncate to `max_chars` respecting char boundaries (advisory text is
/// external UTF-8 data).
/// Cắt tối đa `max_chars` ký tự đúng ranh giới (text advisory là dữ liệu
/// UTF-8 ngoài).
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

/// Audit Go dependencies using govulncheck (source mode, `./...`).
/// Requires: govulncheck on PATH, go.mod in the project root.
/// Audit dependency Go bằng govulncheck (source mode, `./...`).
pub async fn audit_go(project_root: &Path) -> MgResult<AuditReport> {
    if !project_root.join("go.mod").is_file() {
        return Ok(AuditReport::scanner_failed(
            "govulncheck",
            "no go.mod found — the Go scanner only audits Go modules",
        ));
    }
    if which::which("govulncheck").is_err() {
        return Ok(AuditReport::tool_missing(
            "govulncheck",
            "go install golang.org/x/vuln/cmd/govulncheck@latest (official Go vulnerability scanner)",
        ));
    }

    let args = vec!["-json".to_string(), "./...".to_string()];
    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        // Full capture: govulncheck streams findings THROUGHOUT the
        // payload — a line-capped tail would silently drop them.
        // Capture đầy đủ: govulncheck phát finding RẢI KHẮP payload —
        // tail giới hạn dòng sẽ bỏ finding âm thầm.
        capture_full_stdout: true,
        ..Default::default()
    };

    let result = mgc_exec::run::run("govulncheck", &args, &exec_opts)
        .map_err(|e| MgError::Other(format!("govulncheck failed: {e}")))?;

    // `-json` exits 0 even WITH findings — anything else is a tool error.
    // `-json` exit 0 kể cả CÓ finding — khác 0 là lỗi tool.
    if !GOVULNCHECK_OK_EXIT_CODES.contains(&result.exit_code) {
        return Ok(AuditReport::scanner_failed(
            "govulncheck",
            format!(
                "govulncheck exited with code {}: {}",
                result.exit_code, result.stderr_tail
            ),
        ));
    }

    let parsed = parse_govulncheck_json(&result.stdout_full)?;
    Ok(AuditReport {
        packages_audited: parsed.packages_audited,
        vulnerability_count: parsed.vulnerabilities.len(),
        vulnerabilities: parsed.vulnerabilities,
        scanner_status: ScannerStatus::Available,
    })
}
