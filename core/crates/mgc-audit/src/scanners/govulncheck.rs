//! `scanners/govulncheck.rs` — Go OSV scanner plus a compatibility parser
//! for govulncheck streams; the active audit path does not spawn govulncheck.
//! Scanner OSV Go và parser tương thích stream govulncheck; audit hiện tại
//! không spawn govulncheck.
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

use crate::scanners::osv::{OsvPin, audit_osv_pins};
use mgc_types::adapter::{
    AuditReport, FindingClass, ScannerStatus, Vulnerability, VulnerabilitySeverity,
};
use mgc_types::{MgError, MgResult, PackageId, PackageName, Version};
use serde::Deserialize;
use std::path::Path;

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
                finding_class: FindingClass::Vulnerability,
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

/// Audit Go module pins natively through MGC's OSV client.
/// Dùng OSV client của MGC để audit pin Go native.
///
/// Coverage is direct `go.mod` require pins only. `go.sum` transitive
/// build-list parsing and symbol reachability are not implemented, so
/// successful queries remain Partial rather than claiming full coverage.
/// Chỉ phủ pin require trực tiếp trong go.mod; chưa đọc build list
/// transitive trong go.sum hoặc reachability symbol nên kết quả Partial.
pub async fn audit_go(project_root: &Path) -> MgResult<AuditReport> {
    if !project_root.join("go.mod").is_file() {
        return Ok(AuditReport::scanner_failed(
            "mgc-go-osv",
            "no go.mod found — the Go scanner only audits Go modules",
        ));
    }
    audit_go_osv_fallback(project_root).await
}

/// Read go.mod `require` pins into OSV pins (`Go` ecosystem). Both the
/// parenthesized block and single-line requires are read; indirect pins
/// are kept (they are version-exact build-list entries). Non-require
/// directives (toolchain/go/replace/exclude/retract) carry no advisory
/// mapping and are recorded as skipped — never silently counted clean.
/// Versions that are not `v<digits>...` are skipped honestly (OSV Go
/// matches semver-ish versions; a query with a junk version would fail
/// the whole lane).
/// Đọc ghim require trong go.mod thành ghim OSV (ecosystem `Go`). Giữ
/// cả indirect (đúng version build-list). Chỉ thị khác require được ghi
/// skipped. Version không dạng `v<số>...` thì skip trung thực.
pub fn read_go_mod_requires(raw: &str) -> (Vec<OsvPin>, Vec<String>) {
    let mut pins = Vec::new();
    let mut skipped = Vec::new();
    let mut in_require_block = false;
    for line in raw.lines() {
        // Strip `//` comments — versions never contain them.
        // Bỏ comment `//` — version không bao giờ chứa chúng.
        let code = line.split("//").next().unwrap_or("").trim();
        if code.is_empty() {
            continue;
        }
        if code.starts_with("require (") || code == "require(" {
            in_require_block = true;
            continue;
        }
        if in_require_block && code == ")" {
            in_require_block = false;
            continue;
        }
        let directive = if in_require_block {
            Some(code)
        } else if let Some(rest) = code.strip_prefix("require ") {
            let rest = rest.trim();
            if rest.starts_with('(') || rest.is_empty() {
                in_require_block = true;
                None
            } else {
                Some(rest)
            }
        } else {
            if code.starts_with("replace ")
                || code.starts_with("exclude ")
                || code.starts_with("retract")
                || code.starts_with("toolchain ")
                || code.starts_with("go ")
            {
                skipped.push(format!("go.mod directive not advisory-mapped: {code}"));
            }
            None
        };
        let Some(req) = directive else { continue };
        let mut parts = req.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some(module), Some(version))
                if !module.is_empty()
                    && version.starts_with('v')
                    && version.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) =>
            {
                pins.push(OsvPin {
                    name: module.to_string(),
                    version: version.to_string(),
                    ecosystem: "Go",
                });
            }
            _ => skipped.push(format!("go.mod require line without a version pin: {req}")),
        }
    }
    (pins, skipped)
}

/// Query direct require pins and mark the limited coverage honestly.
/// Query pin require trực tiếp và ghi nhận rõ giới hạn phạm vi.
async fn audit_go_osv_fallback(project_root: &Path) -> MgResult<AuditReport> {
    let raw = std::fs::read_to_string(project_root.join("go.mod"))
        .map_err(|e| MgError::Other(format!("read go.mod: {e}")))?;
    let (pins, mut skipped) = read_go_mod_requires(&raw);
    if pins.is_empty() {
        skipped.push(
            "go.mod contains no versioned require pins; transitive coverage is not established"
                .to_string(),
        );
        return Ok(AuditReport {
            packages_audited: 0,
            vulnerability_count: 0,
            vulnerabilities: Vec::new(),
            scanner_status: ScannerStatus::Partial {
                scanned: 0,
                skipped: skipped.len(),
                reasons: skipped,
            },
        });
    }
    skipped.push(
        "OSV fallback covers direct require pins only — go.sum transitives need govulncheck for symbol-level precision"
            .to_string(),
    );
    let mut report = audit_osv_pins(&pins).await?;
    // Findings are real, but coverage is direct-only: Partial, never a
    // full Available. A failed OSV query stays Failed (fail-closed).
    // Finding thật nhưng phủ direct-only: Partial, không Available tròn.
    if matches!(report.scanner_status, ScannerStatus::Available) {
        report.scanner_status = ScannerStatus::Partial {
            scanned: pins.len(),
            skipped: skipped.len(),
            reasons: skipped,
        };
    }
    Ok(report)
}
