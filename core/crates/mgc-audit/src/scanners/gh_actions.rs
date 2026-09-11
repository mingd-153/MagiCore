//! `scanners/gh_actions.rs` — GitHub Actions workflow pinning scanner
//! (P2 2026-09-10, matrix row "CI/CD GitHub Actions action SHA pinning,
//! permissions, injection").
//!
//! Scans `.github/workflows/*.yml` for the three contract classes:
//! 1. `uses:` actions pinned to a TAG/BRANCH instead of a full 40-hex
//!    SHA (mutable ref = supply-chain hijack vector).
//! 2. Top-level `permissions:` absent (default write in some contexts)
//!    — recorded as a WARNING-grade finding, not an error.
//! 3. `pull_request_target` + explicit checkout of the PR head —
//!    classic injection escape.
//!
//! The reader is a conservative YAML-subset scanner (no yaml dep):
//! workflows are generated shapes, and the checks match on `key:` line
//! prefixes at their real indentation levels.
//!
//! Scanner pinning workflow GitHub Actions: quét `uses:` ghim theo
//! TAG/BRANCH thay vì SHA 40-hex đầy đủ; `permissions:` vắng mặt;
//! `pull_request_target` checkout head PR. Bộ đọc là tập con YAML bảo
//! thủ (không dependency yaml): workflow là shape do máy sinh và các
//! kiểm tra khớp theo tiền tố `key:` đúng mức thụt lề thật.

use mgc_types::adapter::{AuditReport, ScannerStatus};
use mgc_types::{MgError, MgResult};
use std::path::Path;

/// Public finding shape for workflow policy violations.
/// Hình finding public cho vi phạm policy workflow.
#[derive(Debug, Clone)]
pub struct WorkflowFinding {
    pub file: String,
    pub line: usize,
    pub rule: &'static str,
    pub detail: String,
}

/// Scan every workflow under .github/workflows. No workflows → honest
/// unsupported for the cicd lane (nothing to verify).
/// Quét mọi workflow dưới .github/workflows. Không có workflow →
/// unsupported trung thực cho lane cicd (không có gì để kiểm).
pub fn audit_github_actions(project_root: &Path) -> MgResult<AuditReport> {
    let workflows_dir = project_root.join(".github").join("workflows");
    if !workflows_dir.is_dir() {
        return Ok(AuditReport::unsupported_ecosystem(
            "cicd/github-actions (no .github/workflows directory found)",
        ));
    }
    let mut findings = Vec::new();
    let mut scanned = 0usize;
    let mut entries: Vec<_> = std::fs::read_dir(&workflows_dir)
        .map_err(|e| MgError::Other(format!("read workflows dir: {e}")))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("yml")
                || p.extension().and_then(|e| e.to_str()) == Some("yaml")
        })
        .collect();
    entries.sort();
    for path in &entries {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workflow.yml")
            .to_string();
        let raw = std::fs::read_to_string(path)
            .map_err(|e| MgError::Other(format!("read {}: {e}", path.display())))?;
        findings.extend(scan_workflow_text(&file_name, &raw)?);
        scanned += 1;
    }
    Ok(build_report(scanned, findings))
}

/// One workflow's text → findings.
/// Text một workflow → finding.
pub fn scan_workflow_text(file: &str, raw: &str) -> MgResult<Vec<WorkflowFinding>> {
    let mut findings = Vec::new();
    let mut has_top_permissions = false;
    let mut saw_pull_request_target = false;
    let mut event_block_indent: Option<usize> = None;

    for (idx, line) in raw.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        // Step bullets ("- uses:") are common — strip once for the key
        // checks that must see through them.
        // Bullet step ("- uses:") phổ biến — bỏ một lần cho các kiểm
        // tra key cần nhìn xuyên qua.
        let key_line = trimmed.strip_prefix("- ").unwrap_or(trimmed);

        // `uses:` at any step level — check the ref shape.
        if let Some(uses) = key_line.strip_prefix("uses:") {
            // Trailing comments ("...@SHA # v4.2.2") document the tag —
            // the pin check reads the REF, not the annotation.
            // Comment cuối ("...@SHA # v4.2.2") ghi chú tag — kiểm tra
            // ghim đọc REF, không đọc chú thích.
            let action_ref = uses
                .split('#')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('\'')
                .trim_matches('"');
            // Shape: owner/repo@ref — a 40-hex ref is SHA-pinned;
            // anything else (tag, branch) is mutable.
            let ref_part = action_ref.rsplit('@').next().unwrap_or("");
            if !action_ref.contains('@')
                || action_ref.ends_with('@')
                || (ref_part.len() != 40 || !ref_part.chars().all(|c| c.is_ascii_hexdigit()))
            {
                findings.push(WorkflowFinding {
                    file: file.to_string(),
                    line: line_no,
                    rule: "actions-unpinned-ref",
                    detail: format!(
                        "'{action_ref}' is not pinned to a full 40-hex commit SHA — mutable refs can be hijacked"
                    ),
                });
            }
        }

        // Top-level `permissions:` (indent 0).
        if indent == 0 && trimmed.starts_with("permissions:") {
            has_top_permissions = true;
        }

        // `on:` block — detect pull_request_target trigger.
        if indent == 0 && trimmed.starts_with("on:") {
            event_block_indent = Some(indent);
        }
        if let Some(block_indent) = event_block_indent
            && indent == block_indent + 2
            && trimmed == "pull_request_target:"
        {
            saw_pull_request_target = true;
        }
        // Leave the event block when a NEW top-level key appears (the
        // `on:` line itself must not reset the block it just opened —
        // only a LATER indent-0 key closes it).
        // Rời event block khi key cấp gốc MỚI xuất hiện (chính dòng
        // `on:` không được đóng block nó vừa mở — chỉ key indent-0
        // SAU ĐÓ mới đóng).
        if let Some(block_indent) = event_block_indent
            && indent <= block_indent
            && !trimmed.is_empty()
            && !trimmed.starts_with("on:")
            && !trimmed.starts_with("pull_request_target")
        {
            event_block_indent = None;
        }

        // pull_request_target + untrusted PR expressions: the workflow
        // runs with base secrets while evaluating PR-controlled
        // expressions — the classic injection escape.
        // pull_request_target + expression PR không tin cậy: workflow
        // chạy với secret base khi diễn đạt expression do PR kiểm
        // soát — lối thoát injection kinh điển.
        if saw_pull_request_target
            && (key_line.starts_with("run:") || key_line.starts_with("ref:"))
            && key_line.contains("github.event.pull_request")
        {
            findings.push(WorkflowFinding {
                file: file.to_string(),
                line: line_no,
                rule: "pull_request_target-injection",
                detail: format!(
                    "pull_request_target workflow evaluates untrusted PR expressions ({}) — runs with base secrets",
                    key_line.trim()
                ),
            });
        }
    }

    if !has_top_permissions {
        findings.push(WorkflowFinding {
            file: file.to_string(),
            line: 0,
            rule: "permissions-undeclared",
            detail: "workflow has no top-level `permissions:` block — defaults may grant write"
                .to_string(),
        });
    }
    Ok(findings)
}

/// Findings → report. Policy violations are severity Info findings (the
/// lane reports them for review; it is not a CVE database).
/// Finding → report. Vi phạm policy là finding Info (lane báo để rà;
/// không phải database CVE).
fn build_report(scanned: usize, findings: Vec<WorkflowFinding>) -> AuditReport {
    use mgc_types::adapter::{Vulnerability, VulnerabilitySeverity};
    use mgc_types::{PackageId, PackageName, Version};

    let mut vulnerabilities = Vec::new();
    for f in &findings {
        // Package "identity" for a workflow finding: the workflow file
        // itself (version carries the line number).
        // "Danh tính" package của finding workflow: chính file workflow
        // (version mang số dòng).
        let Ok(name) = PackageName::new(f.file.clone()) else {
            continue;
        };
        vulnerabilities.push(
            Vulnerability {
                package: PackageId::new(name, Version::new(f.line as u64, 0, 0)),
                title: format!("{}: {}", f.rule, f.detail),
                severity: "info".to_string(),
                cve: f.rule.to_string(),
                severity_level: VulnerabilitySeverity::Info,
                patched_versions: None,
                url: None,
                scanner: None,
                ecosystem: None,
                evidence_at: None,
            }
            .with_evidence("github-actions-policy", "cicd/github-actions"),
        );
    }
    let count = vulnerabilities.len();
    AuditReport {
        packages_audited: scanned,
        vulnerability_count: count,
        vulnerabilities,
        scanner_status: ScannerStatus::Available,
    }
}
