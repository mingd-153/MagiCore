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
//! - Maven `pom.xml` — `<dependency>` groupId/artifactId/version triplets
//!   (fixed versions pin; `${property}`/missing versions skip honestly).
//!
//! Scanner CVE dependency Java/Kotlin qua OSV.dev (ecosystem `Maven`,
//! tên `group:artifact`). Đọc verification-metadata.xml của Gradle và
//! pom.xml của Maven — khóa dòng như các lock reader khác, không cần
//! parser XML/Gradle đầy đủ.

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

/// Java audit: gradle verification metadata → OSV `Maven` queries,
/// falling back to Maven pom.xml dependencies (Spring Boot and plain
/// Maven projects). Without any lockfile the honest unsupported state
/// stays.
/// Audit Java: metadata verification Gradle, rơi về dependency pom.xml
/// → truy vấn OSV `Maven`. Thiếu lockfile giữ unsupported trung thực.
pub async fn audit_java(project_root: &Path) -> MgResult<AuditReport> {
    let verification = project_root
        .join("gradle")
        .join("verification-metadata.xml");
    if verification.is_file() {
        let raw = std::fs::read_to_string(&verification).map_err(|e| {
            mgc_types::MgError::Other(format!("read verification-metadata.xml: {e}"))
        })?;
        return query_maven_pins(&read_gradle_verification_metadata(&raw)).await;
    }
    // Maven/Spring lane: pom.xml dependencies carry exact versions for
    // fixed pins; property/versionless entries skip honestly.
    // Lane Maven/Spring: dependency pom.xml ghim version cố định; entry
    // property/thiếu version skip có ghi nhận.
    let pom = project_root.join("pom.xml");
    if pom.is_file() {
        let raw = std::fs::read_to_string(&pom)
            .map_err(|e| mgc_types::MgError::Other(format!("read pom.xml: {e}")))?;
        let mut report = query_maven_pins(&read_pom_gavs(&raw)).await?;
        // P0-4: pom.xml lists DIRECT dependencies only — the transitive
        // graph (parents, BOMs, profiles, dependencyManagement) is NOT
        // resolved by the line reader, so a pom-only scan can never be
        // Available-clean: force Partial with the coverage note, keeping
        // every finding. (The verification-metadata.xml path above IS a
        // resolved lockfile and keeps its own status.)
        // pom.xml chỉ liệt kê direct dependency — scan pom-only không
        // bao giờ Available: ép Partial giữ nguyên findings.
        if matches!(
            report.scanner_status,
            mgc_types::adapter::ScannerStatus::Available
        ) {
            report.scanner_status = mgc_types::adapter::ScannerStatus::Partial {
                scanned: report.packages_audited,
                skipped: 0,
                reasons: vec![
                    "pom.xml covers direct dependencies only — transitive graph (parents/BOMs/profiles) unresolved; use gradle verification-metadata.xml for a resolved-graph audit"
                        .to_string(),
                ],
            };
        }
        return Ok(report);
    }
    let has_gradle = project_root.join("build.gradle").is_file()
        || project_root.join("build.gradle.kts").is_file();
    Ok(AuditReport::unsupported_ecosystem(if has_gradle {
        "lib/java (gradle project without verification-metadata.xml — run `gradle dependencies --write-verification-metadata sha256` first)"
    } else {
        "lib/java (no gradle project or pom.xml detected)"
    }))
}

/// Query OSV for pins with the shared skipped→Partial tail.
/// Query OSV cho ghim, kèm đuôi skipped→Partial chung.
async fn query_maven_pins(pins: &(Vec<OsvPin>, Vec<String>)) -> MgResult<AuditReport> {
    let (pins, skipped) = pins;
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
                reasons: skipped.clone(),
            },
        });
    }
    let mut report = audit_osv_pins(pins).await?;
    if !skipped.is_empty() {
        report.scanner_status = ScannerStatus::Partial {
            scanned: pins.len(),
            skipped: skipped.len(),
            reasons: skipped.clone(),
        };
    }
    Ok(report)
}

/// Read Maven pom.xml `<dependency>` blocks into OSV pins (`Maven`
/// ecosystem, `group:artifact` names). The block is accumulated until
/// `</dependency>` so one-tag-per-line and single-line layouts both
/// parse; tags are extracted per-name from the block text. A fixed
/// version becomes a pin; property versions (`${...}`), missing
/// versions, or missing coordinates skip HONESTLY (resolving the full
/// Maven model — parents, BOMs, profiles — is out of scope for the
/// line reader, and guessing would fake coverage).
/// Đọc block `<dependency>` trong pom.xml thành ghim OSV. Tích lũy tới
/// `</dependency>` nên cả hai layout đều parse; version cố định thành
/// ghim; version property/thiếu thì skip có ghi nhận (không đoán mò).
pub fn read_pom_gavs(raw: &str) -> (Vec<OsvPin>, Vec<String>) {
    let mut pins = Vec::new();
    let mut skipped = Vec::new();
    let mut block = String::new();
    let mut in_dep = false;
    // dependencyManagement only DECLARES versions for children — those
    // pins are not necessarily in the build, so they skip honestly
    // instead of producing findings for unshipped code.
    // dependencyManagement chỉ KHAI BÁO version cho module con — ghim đó
    // chưa chắc có trong build nên skip có ghi nhận.
    let mut in_mgmt = false;
    // XML comments are not dependencies — a commented-out block must
    // never become a pin (false positive) nor vanish silently; comment
    // spans are stripped before any tag matching.
    // Comment XML không phải dependency — block bị comment không được
    // thành ghim.
    let mut in_comment = false;
    for line in raw.lines() {
        let mut text = line.trim().to_string();
        if in_comment {
            match text.split_once("-->") {
                Some((_, rest)) => {
                    in_comment = false;
                    text = rest.trim().to_string();
                }
                None => continue,
            }
        }
        while let Some((before, rest)) = text
            .split_once("<!--")
            .map(|(b, r)| (b.to_string(), r.to_string()))
        {
            match rest.split_once("-->") {
                // Single-line comment: drop the span, keep the tails.
                // Comment một dòng: bỏ span, giữ hai đầu.
                Some((_, after)) => {
                    text = format!("{before}{after}");
                }
                // Unclosed opener: keep the head, rest is comment.
                // Opener không đóng: giữ đầu, phần còn lại là comment.
                None => {
                    text = before;
                    in_comment = true;
                    break;
                }
            }
        }
        let trimmed = text.trim();
        if trimmed.contains("<dependencyManagement") {
            in_mgmt = true;
        }
        if trimmed.contains("</dependencyManagement>") {
            in_mgmt = false;
            continue;
        }
        // `<dependency>` may open mid-line (single-line dependency
        // elements); `<dependencies>` and `<dependencyManagement>` do NOT
        // contain the exact `<dependency>`/`<dependency ` openers, so a
        // contains-check is safe.
        // `<dependency>` có thể mở giữa dòng; `<dependencies>` và
        // `<dependencyManagement>` không chứa opener chính xác nên
        // contains-check an toàn.
        if trimmed.contains("<dependency>") || trimmed.contains("<dependency ") {
            in_dep = true;
            block.clear();
        }
        if in_dep {
            block.push_str(trimmed);
            block.push('\n');
        }
        if in_dep && trimmed.contains("</dependency>") {
            in_dep = false;
            if in_mgmt {
                skipped.push(
                    "pom dependencyManagement entry is version-only (not resolved into this build)"
                        .to_string(),
                );
                continue;
            }
            match pom_block_pin(&block) {
                Ok(pin) => pins.push(pin),
                Err(reason) => skipped.push(reason),
            }
        }
    }
    (pins, skipped)
}

/// One accumulated `<dependency>` block → pin or honest skip reason.
/// Một block dependency → ghim hoặc lý do skip.
fn pom_block_pin(block: &str) -> Result<OsvPin, String> {
    let tag = |name: &str| {
        let open = format!("<{name}>");
        let close = format!("</{name}>");
        let start = block.find(open.as_str())? + open.len();
        let end = block[start..].find(close.as_str())? + start;
        Some(block[start..end].trim().to_string())
    };
    let group = tag("groupId").filter(|s| !s.is_empty());
    let name = tag("artifactId").filter(|s| !s.is_empty());
    let version = tag("version").filter(|s| !s.is_empty());
    match (group, name, version) {
        (Some(g), Some(n), Some(v)) if !v.contains('$') => Ok(OsvPin {
            name: format!("{g}:{n}"),
            version: v,
            ecosystem: "Maven",
        }),
        (Some(g), Some(n), Some(v)) => Err(format!(
            "pom dependency {g}:{n} has a property version ({v}) — full Maven model out of scope"
        )),
        _ => Err(format!(
            "pom dependency block missing groupId/artifactId/version: {}",
            block.chars().take(80).collect::<String>()
        )),
    }
}
