//! `output/cyclonedx.rs` — CycloneDX 1.5 JSON output embedding audit
//! findings as a Vulnerable BOM (Tech Lead P1 2026-09-09). Only findings
//! from an AVAILABLE scan are emitted — an unverified report never
//! invents CycloneDX vulnerabilities.
//! Output CycloneDX 1.5 JSON — finding nhúng thành Vulnerable BOM. Chỉ
//! finding từ scan Available mới được emit; report chưa xác thực không
//! bịa vulnerability trong CycloneDX.

use mgc_types::adapter::{AuditReport, ScannerStatus, Vulnerability, VulnerabilitySeverity};
use serde::Serialize;

/// CycloneDX spec version emitted by this module.
/// Phiên bản spec CycloneDX mà module này phát hành.
pub const CYCLONEDX_SPEC_VERSION: &str = "1.5";
/// BOM format tag mandated by the CycloneDX schema.
/// Thẻ bomFormat mà schema CycloneDX bắt buộc.
pub const CYCLONEDX_BOM_FORMAT: &str = "CycloneDX";

/// Mapping table from package ecosystem tag to purl type prefix
/// (RULE §12: one central table, not scattered literals).
/// Bảng map tag ecosystem → tiền tố purl (RULE §12: một bảng tập trung,
/// không rải literal).
const PURL_TYPES: &[(&str, &str)] = &[
    ("rust", "cargo"),
    ("python", "pypi"),
    ("web/javascript", "npm"),
    ("kotlin", "maven"),
    ("model-artifact", "generic"),
];

/// Default purl type when the ecosystem tag is missing/unknown.
/// Purl type mặc định khi thiếu/không biết tag ecosystem.
const PURL_TYPE_DEFAULT: &str = "generic";

#[derive(Serialize)]
pub struct CycloneDxBom {
    #[serde(rename = "bomFormat")]
    pub bom_format: String,
    #[serde(rename = "specVersion")]
    pub spec_version: String,
    #[serde(rename = "serialNumber")]
    pub serial_number: String,
    pub version: u32,
    pub metadata: CycloneDxMetadata,
    /// One component per AFFECTED package (not per finding) — a package
    /// hit by two advisories appears once with two vulnerabilities.
    /// Một component cho mỗi package BỊ ẢNH HƯỞNG (không phải mỗi
    /// finding) — package dính 2 advisory xuất hiện 1 lần với 2 vuln.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<CycloneDxComponent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub vulnerabilities: Vec<CycloneDxVulnerability>,
}

#[derive(Serialize)]
pub struct CycloneDxMetadata {
    pub timestamp: String,
    pub tools: CycloneDxTools,
}

/// CycloneDX 1.5 `tools` is a STRUCT with `components` (1.4 used an
/// array) — emitted per the 1.5 schema so strict validators accept it.
/// `tools` của CycloneDX 1.5 là STRUCT chứa `components` (1.4 là mảng) —
/// phát hành theo schema 1.5 để validator chặt chấp nhận.
#[derive(Serialize)]
pub struct CycloneDxTools {
    pub components: Vec<CycloneDxTool>,
}

#[derive(Serialize)]
pub struct CycloneDxTool {
    #[serde(rename = "type")]
    pub tool_type: &'static str,
    pub vendor: &'static str,
    pub name: &'static str,
    pub version: String,
    /// UNVERIFIED marker rides here on non-Available states — loud, not
    /// silent (RULE §11: escape/limit states always announce themselves).
    /// Đánh dấu UNVERIFIED gắn tại đây khi non-Available — rõ ràng, không
    /// im lặng (RULE §11: trạng thái giới hạn luôn tự báo).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<CycloneDxProperty>,
}

#[derive(Serialize)]
pub struct CycloneDxComponent {
    #[serde(rename = "type")]
    pub component_type: &'static str,
    #[serde(rename = "bom-ref")]
    pub bom_ref: String,
    pub name: String,
    pub version: String,
    pub purl: String,
}

#[derive(Serialize)]
pub struct CycloneDxVulnerability {
    #[serde(rename = "bom-ref")]
    pub bom_ref: String,
    pub id: String,
    pub source: CycloneDxSource,
    pub ratings: Vec<CycloneDxRating>,
    pub description: String,
    /// Affected component bom-refs — always non-empty: a vulnerability
    /// row without an affected component is a contract violation.
    /// Bom-ref của các component dính — luôn khác rỗng: dòng vulnerability
    /// mà không có component nào dính là vi phạm hợp đồng.
    pub affects: Vec<CycloneDxAffect>,
    pub properties: Vec<CycloneDxProperty>,
}

#[derive(Serialize)]
pub struct CycloneDxSource {
    pub name: String,
    pub url: Option<String>,
}

#[derive(Serialize)]
pub struct CycloneDxRating {
    /// CycloneDX 1.5 severity names: none, info, low, medium, high,
    /// critical — same five labels as our contract.
    /// Tên severity CycloneDX 1.5 — trùng năm nhãn của hợp đồng ta.
    pub severity: String,
}

#[derive(Serialize)]
pub struct CycloneDxAffect {
    pub r#ref: String,
}

#[derive(Serialize)]
pub struct CycloneDxProperty {
    pub name: String,
    pub value: String,
}

/// Ecosystem tag → purl type prefix ("rust" → "cargo").
/// Tag ecosystem → tiền tố purl ("rust" → "cargo").
fn purl_type_for(ecosystem: Option<&str>) -> &'static str {
    ecosystem
        .and_then(|tag| PURL_TYPES.iter().find(|(k, _)| *k == tag).map(|(_, v)| *v))
        .unwrap_or(PURL_TYPE_DEFAULT)
}

/// Component bom-ref: `purl:<type>/<name>@<version>` — stable within one
/// report render, unique per affected package.
/// Bom-ref component: `purl:<type>/<name>@<version>` — ổn định trong một
/// lần render, duy nhất theo từng package dính.
fn component_bom_ref(v: &Vulnerability) -> String {
    format!(
        "purl:{}/{}@{}",
        purl_type_for(v.ecosystem.as_deref()),
        v.package.name().as_str(),
        v.package.version()
    )
}

/// Build a CycloneDX Vulnerable BOM from an audit report. Non-Available
/// states yield a BOM with ZERO vulnerabilities PLUS an UNVERIFIED
/// property on the tool — honest, never a fabricated empty-clean.
/// Dựng Vulnerable BOM CycloneDX từ report. Trạng thái non-Available cho
/// BOM 0 vulnerability kèm property UNVERIFIED trên tool — trung thực,
/// không bịa sạch rỗng.
pub fn to_cyclonedx(report: &AuditReport) -> CycloneDxBom {
    let verified = matches!(report.scanner_status, ScannerStatus::Available);

    // Deterministic serial from the report CONTENTS — the same report
    // renders the same serial (reproducible output for CI diffing). It
    // identifies this render, not a security boundary.
    // Serial all định từ NỘI DUNG report — cùng report cho cùng serial
    // (output tái lập được để CI diff). Nó định danh bản render, không
    // phải ranh giới bảo mật.
    let mut serial_input = format!(
        "mgc-audit|{}|{}|{}",
        report.packages_audited, report.vulnerability_count, verified
    );
    for v in &report.vulnerabilities {
        serial_input.push_str(&format!("|{}:{}", v.cve, v.package.name().as_str()));
    }
    let serial_hash = stable_serial_hash(&serial_input);

    let mut components: Vec<CycloneDxComponent> = Vec::new();
    let mut vulnerabilities: Vec<CycloneDxVulnerability> = Vec::new();

    if verified {
        for v in &report.vulnerabilities {
            let bom_ref = component_bom_ref(v);
            let purl_type = purl_type_for(v.ecosystem.as_deref());
            let name = v.package.name().as_str();
            let version = v.package.version().to_string();

            // One component per affected package — dedupe on bom-ref.
            // Một component cho mỗi package dính — khử trùng theo bom-ref.
            if !components.iter().any(|c| c.bom_ref == bom_ref) {
                components.push(CycloneDxComponent {
                    component_type: "library",
                    bom_ref: bom_ref.clone(),
                    name: name.to_string(),
                    version: version.clone(),
                    purl: format!("pkg:{purl_type}/{name}@{version}"),
                });
            }

            let scanner_name = v.scanner.as_deref().unwrap_or("unknown");
            vulnerabilities.push(CycloneDxVulnerability {
                bom_ref: format!("vuln:{}:{}", v.cve, bom_ref),
                id: v.cve.clone(),
                source: CycloneDxSource {
                    name: scanner_name.to_string(),
                    url: v.url.clone(),
                },
                ratings: vec![CycloneDxRating {
                    severity: cyclonedx_severity(&v.severity_level).to_string(),
                }],
                description: v.title.clone(),
                affects: vec![CycloneDxAffect { r#ref: bom_ref }],
                properties: vec![
                    CycloneDxProperty {
                        name: "magicore:scanner".to_string(),
                        value: scanner_name.to_string(),
                    },
                    CycloneDxProperty {
                        name: "magicore:evidence_at".to_string(),
                        value: v.evidence_at.clone().unwrap_or_default(),
                    },
                ],
            });
        }
    }

    let tool_properties = (!verified).then(|| {
        vec![CycloneDxProperty {
            name: "magicore:status".to_string(),
            value: "UNVERIFIED — scanner did not complete".to_string(),
        }]
    });

    CycloneDxBom {
        bom_format: CYCLONEDX_BOM_FORMAT.to_string(),
        spec_version: CYCLONEDX_SPEC_VERSION.to_string(),
        serial_number: format!("urn:uuid:{serial_hash}"),
        version: 1,
        metadata: CycloneDxMetadata {
            timestamp: mgc_types::adapter::now_rfc3339_public(),
            tools: CycloneDxTools {
                components: vec![CycloneDxTool {
                    tool_type: "application",
                    vendor: "MagiCore",
                    name: "mgc-audit",
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    properties: tool_properties.unwrap_or_default(),
                }],
            },
        },
        components,
        vulnerabilities,
    }
}

/// CycloneDX 1.5 severity label per contract level.
/// Nhãn severity CycloneDX 1.5 theo level của hợp đồng.
fn cyclonedx_severity(level: &VulnerabilitySeverity) -> &'static str {
    match level {
        VulnerabilitySeverity::Critical => "critical",
        VulnerabilitySeverity::High => "high",
        VulnerabilitySeverity::Medium => "medium",
        VulnerabilitySeverity::Low => "low",
        VulnerabilitySeverity::Info => "info",
    }
}

/// Deterministic 32-hex digest from two FNV-1a 64-bit lanes — std-only,
/// no new dependency. Serial format is `xxxxxxxx-xxxx-4xxx-...` style is
/// NOT required; plain 32 hex inside `urn:uuid:` keeps renders diffable
/// and stable across platforms.
/// Digest 32-hex all định từ hai lane FNV-1a 64-bit — chỉ std, không
/// thêm dependency. Serial 32-hex trong `urn:uuid:` giữ bản render diff
/// được và ổn định mọi nền tảng.
fn stable_serial_hash(input: &str) -> String {
    let mut h1: u64 = 0xcbf2_9ce4_8422_2325;
    let mut h2: u64 = 0xcbf2_9ce4_8422_2325 ^ 0x100_0000_01b3;
    for byte in input.as_bytes() {
        h1 ^= u64::from(*byte);
        h1 = h1.wrapping_mul(0x100_0000_01b3);
        h2 ^= u64::from(*byte).rotate_left(8);
        h2 = h2.wrapping_mul(0x100_0000_01b3 ^ 0x9e37_79b9_7f4a_7c15);
    }
    format!("{h1:016x}{h2:016x}")
}
