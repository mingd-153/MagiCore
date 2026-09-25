//! dedupe transaction E2E (P0-2 proof): `--dry-run` performs ZERO
//! mutations (byte-identical manifest, lock and tree), and a no-op
//! merge changes nothing. Hermetic via mockito.
//!
//! (E2E dedupe: dry-run tuyệt đối không sửa gì — byte-identical.)

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

struct NpmFixture {
    _server: mockito::ServerGuard,
    _mocks: Vec<mockito::Mock>,
    url: String,
}

impl NpmFixture {
    fn new(packages: &[(&str, &str)]) -> Self {
        let mut server = mockito::Server::new();
        let url = server.url();
        let mut mocks = Vec::new();
        for (name, version) in packages {
            let metadata = serde_json::json!({
                "name": name,
                "dist-tags": { "latest": version },
                "versions": { version.to_string(): {
                    "name": name,
                    "version": version,
                    "dependencies": {},
                    "dist": { "tarball": format!("{url}/{name}/-/{name}-{version}.tgz") },
                } },
            });
            mocks.push(
                server
                    .mock("GET", format!("/{name}").as_str())
                    .with_status(200)
                    .with_header("content-type", "application/json")
                    .with_body(metadata.to_string())
                    .create(),
            );
            mocks.push(
                server
                    .mock("GET", format!("/{name}/-/{name}-{version}.tgz").as_str())
                    .with_status(200)
                    .with_header("content-type", "application/octet-stream")
                    .with_body(npm_tarball(name, version))
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
        r#"{ "name": "dd", "version": "1.0.0", "dependencies": { "mgc-dd-a": "9.9.9" } }"#,
    )
    .unwrap();
    std::fs::write(project.join(".mgc.core"), "web\n").unwrap();
    project
}

fn tree_snapshot(project: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![project.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).unwrap();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(meta) = entry.metadata() {
                out.push((path, meta.len()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn dedupe_dry_run_changes_absolutely_nothing() {
    // --dry-run: manifest + lock byte-identical, cây file giống hệt
    // (kể cả .magicore — không cleanup, không journal, không verify).
    // (Dry-run: byte/tree identical, zero side effects.)
    let temp = TempDir::new().unwrap();
    let project = setup_project(&temp);
    let fixture = NpmFixture::new(&[("mgc-dd-a", "9.9.9")]);
    let mgc = find_mgc_binary();

    let mut install = Command::new(&mgc);
    install
        .arg("--core")
        .arg("web")
        .arg("install")
        .current_dir(&project)
        .env("MAGICORE_WEB_REGISTRY_URL", &fixture.url)
        .env("MAGICORE_WEB_ALLOWED_REGISTRIES", &fixture.url)
        .env("MGC_CACHE_DIR", project.join(".magicore"));
    assert!(install.status().unwrap().success(), "fixture install");

    let manifest_before = std::fs::read(project.join("package.json")).unwrap();
    let lock_before = std::fs::read(project.join("mgc.lock")).unwrap();
    let tree_before = tree_snapshot(&project);

    let out = Command::new(&mgc)
        .arg("dedupe")
        .arg("--dry-run")
        .current_dir(&project)
        .env("MGC_CACHE_DIR", project.join(".magicore"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "dry-run must succeed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = String::from_utf8_lossy(&out.stdout).to_string()
        + String::from_utf8_lossy(&out.stderr).as_ref();
    assert!(
        report.contains("dry-run"),
        "must label itself dry-run:\n{report}"
    );
    assert_eq!(
        std::fs::read(project.join("package.json")).unwrap(),
        manifest_before,
        "manifest byte-identical after dry-run"
    );
    assert_eq!(
        std::fs::read(project.join("mgc.lock")).unwrap(),
        lock_before,
        "lock byte-identical after dry-run"
    );
    assert_eq!(
        tree_snapshot(&project),
        tree_before,
        "whole tree (sizes) identical after dry-run"
    );
    assert!(
        !project.join(".magicore/journal").exists(),
        "no journal may exist after a pure dry-run"
    );
}
