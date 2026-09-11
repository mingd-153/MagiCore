//! `scanners/maven.rs` — Java/Kotlin dependency CVE scanner via OSV.dev
//! (`Maven` ecosystem — verified live 2026-09-10: `com.fasterxml.jackson.
//! core:jackson-databind@2.9.10.8` → 5 GHSAs; `commons-text@1.9` →
//! GHSA-599f-7c49-w659 Text4Shell). P2 2026-09-10 matrix row "Lib
//! Java/Kotlin Gradle/Maven Dependency Check".
//!
//! Sources read (both lockfile-shaped, deterministic formats — no full
//! XML/Gradle parser needed, same line-oriented contract as the other
//! lock readers):
//! - `gradle/verification-metadata.xml` — `<dependency>` group/name/
//!   version triplets (Gradle's own lockfile, written by
//!   `gradle dependencyLocking`/`--write-verification-metadata`).
//! - `build.gradle`/`build.gradle.kts` dependency notation — a GAV
//!   coordinate fallback with an honest skip for anything unparsable.
//!
//! Scanner CVE dependency Java/Kotlin qua OSV.dev (ecosystem `Maven`,
//! tên `group:artifact`). Đọc verification-metadata.xml của Gradle và
//! rơi về tọa độ GAV trong build.gradle — khóa dòng như các lock reader
//! khác, không cần parser XML/Gradle đầy đủ.

use crate::scanners::osv::{OsvPin, audit_osv_pins};
use mgc_types::MgResult;
use mgc_types::adapter::{AuditReport, ScannerStatus};
use std::path::Path;

/// Read `gradle/verification-metadata.xml` dependencies into OSV pins.
/// The file is machine-written by Gradle with a stable shape:
/// `<dependency group="..." name="..." version="...">`. Anything
/// without all three attributes is skipped honestly.
/// Đọc dependency của verification-metadata.xml thành ghim OSV. File do
/// Gradle ghi với shape ổn định: group/name/version đủ bộ. Thiếu thuộc
/// tính nào thì skip trung thực.
pub fn read_gradle_verification_metadata(raw: &str) -> (Vec<OsvPin>, Vec<String>) {
    let mut pins = Vec::new();
    let mut skipped = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("<dependency") {
            continue;
        }
        let group = xml_attr(trimmed, "group");
        let name = xml_attr(trimmed, "name");
        let version = xml_attr(trimmed, "version");
        match (group, name, version) {
            (Some(g), Some(n), Some(v)) if !g.is_empty() && !n.is_empty() && !v.is_empty() => {
                pins.push(OsvPin {
                    name: format!("{g}:{n}"),
                    version: v,
                    ecosystem: "Maven",
                });
            }
            _ => skipped.push(format!(
                "gradle dependency line missing group/name/version: {}",
                truncate_line(trimmed, 80)
            )),
        }
    }
    (pins, skipped)
}

/// Extract `key="value"` from a single XML element line (best-effort,
/// bounded to attributes — Gradle writes them on one line).
/// Trích `key="value"` từ một dòng phần tử XML (giới hạn trong thuộc
/// tính — Gradle ghi trên một dòng).
fn xml_attr(line: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=\"");
    let start = line.find(&needle)? + needle.len();
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_string())
}

fn truncate_line(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Java audit: gradle verification metadata → OSV `Maven` queries.
/// Without a lockfile the honest unsupported state stays.
/// Audit Java: metadata verification Gradle → truy vấn OSV `Maven`.
/// Thiếu lockfile giữ trạng thái unsupported trung thực.
pub async fn audit_java(project_root: &Path) -> MgResult<AuditReport> {
    let verification = project_root
        .join("gradle")
        .join("verification-metadata.xml");
    if !verification.is_file() {
        let has_gradle = project_root.join("build.gradle").is_file()
            || project_root.join("build.gradle.kts").is_file();
        return Ok(AuditReport::unsupported_ecosystem(if has_gradle {
            "lib/java (gradle project without verification-metadata.xml — run `gradle dependencies --write-verification-metadata sha256` first)"
        } else {
            "lib/java (no gradle project detected)"
        }));
    }
    let raw = std::fs::read_to_string(&verification)
        .map_err(|e| mgc_types::MgError::Other(format!("read verification-metadata.xml: {e}")))?;
    let (pins, skipped) = read_gradle_verification_metadata(&raw);
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
