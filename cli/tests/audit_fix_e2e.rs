//! audit --fix E2E (P0-1 proof): the fix path must actually RUN.
//! A vulnerable fixture + `--fix` must mutate through the transaction
//! gateway (manifest + lock rewritten, re-audit clean, exit 0); without
//! --fix the same fixture exits 1; a failed fix rolls back; a Failed
//! scanner never auto-fixes.
//!
//! (E2E audit --fix: mutation thật qua gateway, rollback khi lỗi,
//! scanner hỏng không tự fix. Hermetic qua mockito.)

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn find_mgc_binary() -> String {
    std::env::var("CARGO_BIN_EXE_mgc")
        .expect("CARGO_BIN_EXE_mgc not set — run via `cargo test -p mgc`")
}

fn npm_tarball(name: &str, version: &str) -> Vec<u8> {
    let package_json = serde_json::json!({
        "name": name,
        "version": version,
        "main": "index.js",
    })
    .to_string();
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_size(package_json.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive
        .append_data(&mut header, "package/package.json", package_json.as_bytes())
        .unwrap();
    let encoder = archive.into_inner().unwrap();
    encoder.finish().unwrap()
}

struct AuditFixture {
    _server: mockito::ServerGuard,
    _mocks: Vec<mockito::Mock>,
    url: String,
}

impl AuditFixture {
    /// Registry serving mgc-audit-vuln 1.0.0 (vulnerable) + 1.0.1
    /// (clean), plus an advisory binding <1.0.1. Set `break_tarball`
    /// to make the 1.0.1 artifact fail (fix-failure path), or
    /// `break_advisories` for a 500 advisory API (Failed scanner path).
    fn new(break_tarball: bool, break_advisories: bool) -> Self {
        let mut server = mockito::Server::new();
        let url = server.url();
        let mut mocks = Vec::new();
        // ONE metadata document listing BOTH versions (latest 1.0.1) —
        // separate per-version mocks would shadow each other.
        // (MỘT metadata cho cả hai version — mock riêng sẽ che nhau.)
        let mut versions = serde_json::Map::new();
        for version in ["1.0.0", "1.0.1"] {
            versions.insert(
                version.to_string(),
                serde_json::json!({
                    "name": "mgc-audit-vuln",
                    "version": version,
                    "dependencies": {},
                    "dist": { "tarball": format!("{url}/mgc-audit-vuln/-/{version}.tgz") },
                }),
            );
        }
        let metadata = serde_json::json!({
            "name": "mgc-audit-vuln",
            "dist-tags": { "latest": "1.0.1" },
            "versions": versions,
        });
        mocks.push(
            server
                .mock("GET", "/mgc-audit-vuln")
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(metadata.to_string())
                .create(),
        );
        for version in ["1.0.0", "1.0.1"] {
            if break_tarball && version == "1.0.1" {
                mocks.push(
                    server
                        .mock("GET", "/mgc-audit-vuln/-/1.0.1.tgz")
                        .with_status(500)
                        .with_body("broken artifact")
                        .create(),
                );
            } else {
                mocks.push(
                    server
                        .mock("GET", format!("/mgc-audit-vuln/-/{version}.tgz").as_str())
                        .with_status(200)
                        .with_header("content-type", "application/octet-stream")
                        .with_body(npm_tarball("mgc-audit-vuln", version))
                        .create(),
                );
            }
        }
        if break_advisories {
            mocks.push(
                server
                    .mock("POST", "/-/npm/v1/security/advisories/bulk")
                    .with_status(500)
                    .with_body("advisory backend down")
                    .create(),
            );
        } else {
            let advisory = serde_json::json!({
                "mgc-audit-vuln": [{
                    "id": 1234567,
                    "url": "https://example.com/advisories/1234567",
                    "title": "Test XSS in fixture",
                    "severity": "high",
                    "vulnerable_versions": "<1.0.1",
                }],
            });
            mocks.push(
                server
                    .mock("POST", "/-/npm/v1/security/advisories/bulk")
                    .with_status(200)
                    .with_header("content-type", "application/json")
                    .with_body(advisory.to_string())
                    .create(),
            );
        }
        Self {
            _server: server,
            _mocks: mocks,
            url,
        }
    }
}

fn setup_project(temp: &TempDir) -> PathBuf {
    let project = temp.path().join("site");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("package.json"),
        r#"{ "name": "audited", "version": "1.0.0", "dependencies": { "mgc-audit-vuln": "1.0.0" } }"#,
    )
    .unwrap();
    std::fs::write(project.join(".mgc.core"), "web\n").unwrap();
    project
}

fn mgc_cmd(mgc: &str, project: &Path, registry_url: &str, log_dir: &Path, tag: &str) -> Command {
    let stdout = std::fs::File::create(log_dir.join(format!("{tag}.out"))).unwrap();
    let stderr = std::fs::File::create(log_dir.join(format!("{tag}.err"))).unwrap();
    let mut cmd = Command::new(mgc);
    cmd.current_dir(project)
        .env("MAGICORE_WEB_REGISTRY_URL", registry_url)
        .env("MAGICORE_WEB_ALLOWED_REGISTRIES", registry_url)
        .env("MGC_CACHE_DIR", project.join(".magicore"))
        .env("MGC_AUDIT_STRICT", "0")
        .env_remove("CI")
        .stdout(stdout)
        .stderr(stderr);
    cmd
}

fn read_log(log_dir: &Path, tag: &str, ext: &str) -> String {
    std::fs::read_to_string(log_dir.join(format!("{tag}.{ext}"))).unwrap_or_default()
}

fn install_locked(mgc: &str, project: &Path, url: &str, log_dir: &Path) {
    let mut cmd = mgc_cmd(mgc, project, url, log_dir, "install");
    cmd.arg("--core").arg("web").arg("install");
    let status = cmd.status().unwrap();
    assert!(
        status.success(),
        "fixture install must succeed:\n{}",
        read_log(log_dir, "install", "err")
    );
    assert!(
        project.join("mgc.lock").exists(),
        "install must write mgc.lock (audit reads it)"
    );
}

#[test]
fn audit_without_fix_reports_findings_and_changes_nothing() {
    // Vulnerable fixture, không --fix → exit 1, không đổi gì.
    // (Findings without --fix: exit 1, byte-identical project.)
    let temp = TempDir::new().unwrap();
    let project = setup_project(&temp);
    let fixture = AuditFixture::new(false, false);
    let mgc = find_mgc_binary();
    let log_dir = temp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();
    install_locked(&mgc, &project, &fixture.url, &log_dir);

    let manifest_before = std::fs::read(project.join("package.json")).unwrap();
    let lock_before = std::fs::read(project.join("mgc.lock")).unwrap();
    let mut cmd = mgc_cmd(&mgc, &project, &fixture.url, &log_dir, "audit");
    cmd.arg("audit");
    let status = cmd.status().unwrap();
    assert!(
        !status.success(),
        "vulnerable fixture without --fix must exit non-zero"
    );
    assert_eq!(
        std::fs::read(project.join("package.json")).unwrap(),
        manifest_before,
        "audit without --fix must not touch the manifest"
    );
    assert_eq!(
        std::fs::read(project.join("mgc.lock")).unwrap(),
        lock_before,
        "audit without --fix must not touch the lock"
    );
}

#[test]
fn audit_fix_mutates_and_reaudits_clean() {
    // Vulnerable fixture + --fix → mutation qua gateway, re-audit sạch,
    // exit 0, manifest lên 1.0.1, hết journal.
    // (--fix mutates through the gateway and exits on the POST report.)
    let temp = TempDir::new().unwrap();
    let project = setup_project(&temp);
    let fixture = AuditFixture::new(false, false);
    let mgc = find_mgc_binary();
    let log_dir = temp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();
    install_locked(&mgc, &project, &fixture.url, &log_dir);

    let mut cmd = mgc_cmd(&mgc, &project, &fixture.url, &log_dir, "fix");
    cmd.arg("audit").arg("--fix");
    let status = cmd.status().unwrap();
    assert!(
        status.success(),
        "fixed project must re-audit clean (exit 0):\n{}",
        read_log(&log_dir, "fix", "err")
    );
    let manifest = std::fs::read_to_string(project.join("package.json")).unwrap();
    assert!(
        !manifest.contains("\"mgc-audit-vuln\": \"1.0.0\""),
        "fix must lift the vulnerable pin from the manifest:\n{manifest}"
    );
    let lock = std::fs::read_to_string(project.join("mgc.lock")).unwrap();
    assert!(
        lock.contains("1.0.1"),
        "fix must resolve the clean version into the lock:\n{lock}"
    );
    assert!(
        !project
            .join(".magicore/journal/dependency-mutation/journal.json")
            .exists(),
        "no journal may survive a successful fix"
    );
}

#[test]
fn audit_fix_failure_rolls_back_manifest_and_lock() {
    // Fix lỗi resolve (offline ép buộc sau khi manifest đã star) →
    // manifest + lock về pre byte-identical, exit lỗi, hết journal.
    // (Failed fix rolls back to the pre-image.)
    let temp = TempDir::new().unwrap();
    let project = setup_project(&temp);
    let fixture = AuditFixture::new(false, false);
    let mgc = find_mgc_binary();
    let log_dir = temp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();
    install_locked(&mgc, &project, &fixture.url, &log_dir);
    let lock_before = std::fs::read(project.join("mgc.lock")).unwrap();

    let mut cmd = mgc_cmd(&mgc, &project, &fixture.url, &log_dir, "fix");
    cmd.arg("audit").arg("--fix").env("MGC_OFFLINE_MODE", "1");
    let status = cmd.status().unwrap();
    assert!(!status.success(), "failed fix must exit non-zero");
    let manifest = std::fs::read_to_string(project.join("package.json")).unwrap();
    assert!(
        manifest.contains("1.0.0"),
        "rolled-back manifest must keep 1.0.0:\n{manifest}"
    );
    assert!(
        !manifest.contains("1.0.1"),
        "failed bump must not persist:\n{manifest}"
    );
    assert_eq!(
        std::fs::read(project.join("mgc.lock")).unwrap(),
        lock_before,
        "rolled-back lock must be byte-identical"
    );
    assert!(
        !project
            .join(".magicore/journal/dependency-mutation/journal.json")
            .exists(),
        "no journal may survive the rollback"
    );
}

#[test]
fn failed_scanner_never_autofixes() {
    // Advisory API 500 → scanner Failed: --fix KHÔNG được sửa gì, exit
    // theo contract local (0 + warning), manifest nguyên.
    // (Failed scanner never auto-fixes, whatever --fix says.)
    let temp = TempDir::new().unwrap();
    let project = setup_project(&temp);
    let fixture = AuditFixture::new(false, true);
    let mgc = find_mgc_binary();
    let log_dir = temp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();
    install_locked(&mgc, &project, &fixture.url, &log_dir);
    let manifest_before = std::fs::read(project.join("package.json")).unwrap();

    let mut cmd = mgc_cmd(&mgc, &project, &fixture.url, &log_dir, "fix");
    cmd.arg("audit").arg("--fix");
    let status = cmd.status().unwrap();
    assert!(
        status.success(),
        "failed scanner in local mode exits 0 (UNVERIFIED escape hatch):\n{}",
        read_log(&log_dir, "fix", "err")
    );
    assert_eq!(
        std::fs::read(project.join("package.json")).unwrap(),
        manifest_before,
        "failed scanner must not mutate the manifest"
    );
    assert!(
        !project.join(".magicore/journal").exists(),
        "no journal may exist without a mutation"
    );
}
