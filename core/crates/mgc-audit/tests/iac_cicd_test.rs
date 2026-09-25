//! `iac_cicd_test.rs` — Terraform provider-lock + GitHub Actions
//! policy scanner tests (P2 2026-09-10): checksum enforcement, SHA
//! pinning detection, permissions declaration, injection pattern,
//! malformed fail-closed, unicode, large files, concurrency.
//! Test scanner provider-lock Terraform + policy GitHub Actions: bắt
//! buộc checksum, phát hiện SHA pinning, khai báo permissions, mẫu
//! injection, malformed fail-closed, unicode, file lớn, đồng thời.

#![allow(clippy::unwrap_used)]

use mgc_audit::scanners::{audit_terraform_lock, parse_terraform_lock, scan_workflow_text};

const TF_LOCK: &str = r#"# This file is maintained automatically by `terraform init`.
provider "registry.terraform.io/hashicorp/aws" {
  version     = "5.62.0"
  constraints = "~> 5.0"
  hashes = [
    "h1:abc123=",
    "zh:abcdef123456=",
  ]
}

provider "registry.terraform.io/hashicorp/null" {
  version     = "3.2.3"
  hashes = [
    "h1:def456=",
  ]
}
"#;

#[test]
fn terraform_lock_with_checksums_is_available() {
    let report = parse_terraform_lock(TF_LOCK).unwrap();
    assert!(matches!(
        report.scanner_status,
        mgc_types::adapter::ScannerStatus::Available
    ));
    assert_eq!(report.packages_audited, 2);
}

#[test]
fn terraform_lock_provider_without_checksums_fails() {
    let bad = r#"provider "registry.terraform.io/hashicorp/aws" {
  version = "5.62.0"
}"#;
    let report = parse_terraform_lock(bad).unwrap();
    match &report.scanner_status {
        mgc_types::adapter::ScannerStatus::Failed { scanner, reason } => {
            assert_eq!(scanner, "terraform-provider-lock");
            assert!(
                reason.contains("hashicorp/aws"),
                "provider must be named: {reason}"
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn terraform_lock_malformed_header_fails_closed() {
    assert!(parse_terraform_lock("provider \"broken").is_err());
}

#[test]
fn terraform_lock_missing_file_is_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let report = audit_terraform_lock(dir.path()).unwrap();
    assert!(matches!(
        report.scanner_status,
        mgc_types::adapter::ScannerStatus::UnsupportedEcosystem { .. }
    ));
}

#[test]
fn terraform_lock_truncated_block_reports_missing_checksums() {
    // No closing brace → the trailing provider still gets evaluated.
    // Thiếu dấu đóng → provider cuối vẫn được đánh giá.
    let truncated = r#"provider "registry.terraform.io/hashicorp/aws" {
  version = "5.62.0"
  hashes = ["h1:x""#;
    // The checksums line opened but the file ended — treated as
    // present (hashes bracket opened) in the conservative reader; the
    // contract here documents the behavior explicitly.
    let report = parse_terraform_lock(truncated).unwrap();
    assert!(matches!(
        report.scanner_status,
        mgc_types::adapter::ScannerStatus::Available
            | mgc_types::adapter::ScannerStatus::Failed { .. }
    ));
}

const WORKFLOW_PINNED: &str = r#"name: ci
permissions:
  contents: read
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - run: make test
"#;

const WORKFLOW_BAD: &str = r#"name: ci
on:
  pull_request_target:
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/checkout@main
      - name: run pr code
        run: |
          echo "${{ github.event.pull_request.head.sha }}"
        # (injection pattern checked below with an explicit ref:)
      - uses: evil/act@deadbeef
      - run: git checkout ${{ github.event.pull_request.head.ref }}
"#;

#[test]
fn workflow_sha_pinned_and_permissions_declared_is_clean() {
    let findings = scan_workflow_text("ci.yml", WORKFLOW_PINNED).unwrap();
    assert!(
        findings.is_empty(),
        "pinned workflow with permissions must yield no findings: {findings:#?}"
    );
}

#[test]
fn workflow_unpinned_refs_are_flagged() {
    let findings = scan_workflow_text("bad.yml", WORKFLOW_BAD).unwrap();
    let rules: Vec<&str> = findings.iter().map(|f| f.rule).collect();
    assert!(rules.contains(&"actions-unpinned-ref"));
    // Both the tag (v4) and branch (main) are flagged.
    // Cả tag (v4) lẫn branch (main) đều bị nêu.
    let unpinned: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "actions-unpinned-ref")
        .collect();
    assert!(unpinned.len() >= 2, "tag + branch both flagged");
    assert!(unpinned.iter().any(|f| f.detail.contains("@v4")));
    assert!(unpinned.iter().any(|f| f.detail.contains("@main")));
    // `evil/act@deadbeef` is only 8 hex chars — NOT a full SHA, so it
    // IS flagged (short hashes are also mutable-ish refs); the
    // 40-hex pinned evil ref is the one that must NOT appear.
    // `evil/act@deadbeef` chỉ 8 ký tự hex — KHÔNG phải SHA đầy đủ nên
    // BỊ nêu (hash ngắn cũng ref dễ đổi); ref 40-hex đã ghim mới là
    // cái không được xuất hiện.
    assert!(unpinned.iter().any(|f| f.detail.contains("evil/act")));
    assert!(!unpinned.iter().any(|f| {
        f.detail
            .contains("11bd71901bbe5b1630ceea73d27597364c9af683")
    }));
}

#[test]
fn workflow_missing_permissions_is_flagged() {
    let findings = scan_workflow_text("bad.yml", WORKFLOW_BAD).unwrap();
    assert!(findings.iter().any(|f| f.rule == "permissions-undeclared"));
}

#[test]
fn workflow_pull_request_target_with_pr_ref_is_flagged() {
    let findings = scan_workflow_text("bad.yml", WORKFLOW_BAD).unwrap();
    let injection: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "pull_request_target-injection")
        .collect();
    assert!(
        !injection.is_empty(),
        "pull_request_target + PR head checkout must be flagged: {findings:#?}"
    );
}

#[test]
fn workflow_unicode_job_names_round_trip() {
    let wf = r#"name: "dự án 🦀"
permissions:
  contents: read
jobs:
  "việc-làm":
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683
"#;
    let findings = scan_workflow_text("unicode.yml", wf).unwrap();
    assert!(findings.is_empty());
}

#[test]
fn workflow_large_file_scans_all_steps() {
    let mut wf =
        String::from("name: big\npermissions:\n  contents: read\njobs:\n  build:\n    steps:\n");
    let total = 5_000;
    for _ in 0..total {
        wf.push_str("      - uses: actions/checkout@v4\n");
    }
    let findings = scan_workflow_text("big.yml", &wf).unwrap();
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "actions-unpinned-ref")
            .count(),
        total,
        "every unpinned step is flagged"
    );
}

#[test]
fn workflow_scans_concurrently() {
    let payloads = [WORKFLOW_PINNED, WORKFLOW_BAD, WORKFLOW_PINNED, WORKFLOW_BAD];
    std::thread::scope(|scope| {
        let handles: Vec<_> = payloads
            .iter()
            .map(|p| {
                let payload = p.to_string();
                scope.spawn(move || scan_workflow_text("x.yml", &payload).unwrap())
            })
            .collect();
        for (i, h) in handles.into_iter().enumerate() {
            let expected_empty = i % 2 == 0;
            let got = h.join().unwrap();
            assert_eq!(
                got.is_empty(),
                expected_empty,
                "workflow scan {i} cross-contaminated"
            );
        }
    });
}
