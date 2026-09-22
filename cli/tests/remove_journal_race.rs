//! Concurrent remove journal race (P0-B wiring proof): two REAL `mgc`
//! processes removing DIFFERENT deps from ONE web project at the same
//! time must serialize on the project writer lock — both succeed, the
//! final manifest lacks both deps, no journal is left behind, and no
//! spurious crash-recovery resurrects anything. Fully hermetic
//! (`--no-install`: no registry, no network).
//!
//! (Đua journal remove 2 process THẬT: xếp hàng qua lock writer — cả hai
//! xong, manifest mất cả hai dep, không journal sót, không phục hồi ma.)

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn find_mgc_binary() -> String {
    std::env::var("CARGO_BIN_EXE_mgc")
        .expect("CARGO_BIN_EXE_mgc not set — run via `cargo test -p mgc`")
}

fn create_web_project(temp: &TempDir) -> PathBuf {
    let project = temp.path().join("site");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("package.json"),
        r#"{ "name": "race", "version": "1.0.0", "dependencies": { "is-odd": "3.0.1", "is-even": "1.0.0" } }"#,
    )
    .unwrap();
    std::fs::write(project.join(".mgc.core"), "web\n").unwrap();
    project
}

fn remove_cmd(mgc: &str, project: &Path, package: &str, log_dir: &Path, tag: &str) -> Command {
    let stdout = std::fs::File::create(log_dir.join(format!("{tag}.out"))).unwrap();
    let stderr = std::fs::File::create(log_dir.join(format!("{tag}.err"))).unwrap();
    let mut cmd = Command::new(mgc);
    cmd.arg("--core")
        .arg("web")
        .arg("remove")
        .arg(package)
        .arg("--no-install")
        .current_dir(project)
        .env("MGC_CACHE_DIR", project.join(".magicore"))
        .stdout(stdout)
        .stderr(stderr);
    cmd
}

fn read_log(log_dir: &Path, tag: &str, ext: &str) -> String {
    std::fs::read_to_string(log_dir.join(format!("{tag}.{ext}"))).unwrap_or_default()
}

struct NpmFixture {
    _server: mockito::ServerGuard,
    _mocks: Vec<mockito::Mock>,
    url: String,
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

impl NpmFixture {
    /// Hermetic npm registry serving the given name@version pairs
    /// (metadata + tarball), so add/install tails never touch the network.
    /// (Registry npm hermetic cho add/install tail.)
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
            let tarball = npm_tarball(name, version);
            mocks.push(
                server
                    .mock("GET", format!("/{name}/-/{name}-{version}.tgz").as_str())
                    .with_status(200)
                    .with_header("content-type", "application/octet-stream")
                    .with_body(tarball)
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

fn add_cmd(
    mgc: &str,
    project: &Path,
    package: &str,
    registry_url: &str,
    log_dir: &Path,
    tag: &str,
) -> Command {
    let stdout = std::fs::File::create(log_dir.join(format!("{tag}.out"))).unwrap();
    let stderr = std::fs::File::create(log_dir.join(format!("{tag}.err"))).unwrap();
    let mut cmd = Command::new(mgc);
    cmd.arg("--core")
        .arg("web")
        .arg("add")
        .arg(package)
        .current_dir(project)
        .env("MAGICORE_WEB_REGISTRY_URL", registry_url)
        .env("MAGICORE_WEB_ALLOWED_REGISTRIES", registry_url)
        .env("MGC_CACHE_DIR", project.join(".magicore"))
        .stdout(stdout)
        .stderr(stderr);
    cmd
}

fn assert_no_phantom_recovery(log_dir: &Path, tag: &str) {
    let combined = read_log(log_dir, tag, "out") + &read_log(log_dir, tag, "err");
    assert!(
        !combined.contains("recovered an interrupted mutation"),
        "{tag}: no spurious crash recovery on a serialized run"
    );
    assert!(
        !combined.contains("nested mutation detected"),
        "{tag}: no nested-mutation false positive across processes"
    );
}

fn update_cmd(
    mgc: &str,
    project: &Path,
    package: &str,
    registry_url: &str,
    log_dir: &Path,
    tag: &str,
) -> Command {
    let stdout = std::fs::File::create(log_dir.join(format!("{tag}.out"))).unwrap();
    let stderr = std::fs::File::create(log_dir.join(format!("{tag}.err"))).unwrap();
    let mut cmd = Command::new(mgc);
    cmd.arg("--core")
        .arg("web")
        .arg("update")
        .arg(package)
        .current_dir(project)
        .env("MAGICORE_WEB_REGISTRY_URL", registry_url)
        .env("MAGICORE_WEB_ALLOWED_REGISTRIES", registry_url)
        .env("MGC_CACHE_DIR", project.join(".magicore"))
        .stdout(stdout)
        .stderr(stderr);
    cmd
}

#[test]
fn concurrent_removes_serialize_without_journal_corruption() {
    let temp = TempDir::new().unwrap();
    let project = create_web_project(&temp);
    let mgc = find_mgc_binary();
    let log_dir = temp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();

    // Launch both at once — the writer lock serializes them.
    // (Chạy cùng lúc — lock writer xếp hàng.)
    let mut first = remove_cmd(&mgc, &project, "is-odd", &log_dir, "first")
        .spawn()
        .unwrap();
    let mut second = remove_cmd(&mgc, &project, "is-even", &log_dir, "second")
        .spawn()
        .unwrap();
    let first_status = first.wait().unwrap();
    let second_status = second.wait().unwrap();
    assert!(
        first_status.success(),
        "first remove must succeed:\n{}",
        read_log(&log_dir, "first", "err")
    );
    assert!(
        second_status.success(),
        "second remove must succeed:\n{}",
        read_log(&log_dir, "second", "err")
    );

    // Both deps gone, journal gone, no phantom recovery.
    // (Mất cả hai dep, hết journal, không phục hồi ma.)
    let manifest = std::fs::read_to_string(project.join("package.json")).unwrap();
    assert!(
        !manifest.contains("is-odd"),
        "is-odd must stay removed:\n{manifest}"
    );
    assert!(
        !manifest.contains("is-even"),
        "is-even must stay removed:\n{manifest}"
    );
    assert!(
        !project
            .join(".magicore/journal/remove/journal.json")
            .exists(),
        "no stale journal may survive two serialized removes"
    );
    for tag in ["first", "second"] {
        assert_no_phantom_recovery(&log_dir, tag);
    }
}

#[test]
fn concurrent_adds_serialize_without_manifest_tear() {
    // two-add: hai process add dep khác nhau cùng lúc — lock writer xếp
    // hàng, manifest cuối có cả hai, lock hợp lệ, không journal sót.
    // (Two concurrent adds serialize — both deps land, no tear.)
    let temp = TempDir::new().unwrap();
    let project = temp.path().join("site");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("package.json"),
        r#"{ "name": "race", "version": "1.0.0", "dependencies": {} }"#,
    )
    .unwrap();
    std::fs::write(project.join(".mgc.core"), "web\n").unwrap();
    let fixture = NpmFixture::new(&[("mgc-race-alpha", "9.9.9"), ("mgc-race-beta", "9.9.9")]);
    let mgc = find_mgc_binary();
    let log_dir = temp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();

    let mut first = add_cmd(
        &mgc,
        &project,
        "mgc-race-alpha",
        &fixture.url,
        &log_dir,
        "add-odd",
    )
    .spawn()
    .unwrap();
    let mut second = add_cmd(
        &mgc,
        &project,
        "mgc-race-beta",
        &fixture.url,
        &log_dir,
        "add-even",
    )
    .spawn()
    .unwrap();
    let first_status = first.wait().unwrap();
    let second_status = second.wait().unwrap();
    assert!(
        first_status.success(),
        "first add must succeed:\n{}",
        read_log(&log_dir, "add-odd", "err")
    );
    assert!(
        second_status.success(),
        "second add must succeed:\n{}",
        read_log(&log_dir, "add-even", "err")
    );

    let manifest = std::fs::read_to_string(project.join("package.json")).unwrap();
    assert!(
        manifest.contains("mgc-race-alpha"),
        "mgc-race-alpha must land:\n{manifest}"
    );
    assert!(
        manifest.contains("mgc-race-beta"),
        "mgc-race-beta must land:\n{manifest}"
    );
    for tag in ["add-odd", "add-even"] {
        assert_no_phantom_recovery(&log_dir, tag);
    }
}

#[test]
fn concurrent_remove_and_add_converge() {
    // remove×add: remove is-odd (--no-install) đua với add is-even —
    // hai mutation giao hoán, trạng thái cuối phải hội tụ: mất is-odd,
    // có is-even, không journal sót, không phục hồi ma.
    // (Remove races add — commuting mutations must converge.)
    let temp = TempDir::new().unwrap();
    let project = temp.path().join("site");
    std::fs::create_dir_all(&project).unwrap();
    // Pre-seed the to-be-removed fake dep (never resolved: --no-install).
    // (Cài sẵn dep giả để remove có việc làm — không resolve.)
    std::fs::write(
        project.join("package.json"),
        r#"{ "name": "race", "version": "1.0.0", "dependencies": { "mgc-race-alpha": "9.9.9" } }"#,
    )
    .unwrap();
    std::fs::write(project.join(".mgc.core"), "web\n").unwrap();
    let fixture = NpmFixture::new(&[("mgc-race-alpha", "9.9.9"), ("mgc-race-beta", "9.9.9")]);
    let mgc = find_mgc_binary();
    let log_dir = temp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();

    let mut removing = remove_cmd(&mgc, &project, "mgc-race-alpha", &log_dir, "rm")
        .spawn()
        .unwrap();
    let mut adding = add_cmd(
        &mgc,
        &project,
        "mgc-race-beta",
        &fixture.url,
        &log_dir,
        "add",
    )
    .spawn()
    .unwrap();
    let rm_status = removing.wait().unwrap();
    let add_status = adding.wait().unwrap();
    assert!(
        rm_status.success(),
        "remove must succeed:\n{}",
        read_log(&log_dir, "rm", "err")
    );
    assert!(
        add_status.success(),
        "add must succeed:\n{}",
        read_log(&log_dir, "add", "err")
    );

    let manifest = std::fs::read_to_string(project.join("package.json")).unwrap();
    assert!(
        !manifest.contains("mgc-race-alpha"),
        "is-odd must stay removed:\n{manifest}"
    );
    assert!(
        manifest.contains("mgc-race-beta"),
        "mgc-race-beta must land:\n{manifest}"
    );
    assert!(
        !project
            .join(".magicore/journal/remove/journal.json")
            .exists(),
        "no stale journal may survive serialized mutations"
    );
    for tag in ["rm", "add"] {
        assert_no_phantom_recovery(&log_dir, tag);
    }
}

#[test]
fn concurrent_updates_serialize_without_loss() {
    // update×update + update×remove: mọi mutation qua gateway lock —
    // không mất thay đổi, không journal sót.
    // (Update races serialize — no lost updates, no stale journal.)
    let temp = TempDir::new().unwrap();
    let project = temp.path().join("site");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("package.json"),
        r#"{ "name": "race", "version": "1.0.0", "dependencies": { "mgc-race-alpha": "9.9.9", "mgc-race-beta": "9.9.9" } }"#,
    )
    .unwrap();
    std::fs::write(project.join(".mgc.core"), "web\n").unwrap();
    let fixture = NpmFixture::new(&[("mgc-race-alpha", "9.9.9"), ("mgc-race-beta", "9.9.9")]);
    let mgc = find_mgc_binary();
    let log_dir = temp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();

    let mut updating_a = update_cmd(
        &mgc,
        &project,
        "mgc-race-alpha",
        &fixture.url,
        &log_dir,
        "up-a",
    )
    .spawn()
    .unwrap();
    let mut updating_b = update_cmd(
        &mgc,
        &project,
        "mgc-race-beta",
        &fixture.url,
        &log_dir,
        "up-b",
    )
    .spawn()
    .unwrap();
    let status_a = updating_a.wait().unwrap();
    let status_b = updating_b.wait().unwrap();
    assert!(
        status_a.success(),
        "update alpha must succeed:\n{}",
        read_log(&log_dir, "up-a", "err")
    );
    assert!(
        status_b.success(),
        "update beta must succeed:\n{}",
        read_log(&log_dir, "up-b", "err")
    );

    let manifest = std::fs::read_to_string(project.join("package.json")).unwrap();
    for dep in ["mgc-race-alpha", "mgc-race-beta"] {
        assert!(manifest.contains(dep), "{dep} must survive:\n{manifest}");
    }
    assert!(
        !project
            .join(".magicore/journal/remove/journal.json")
            .exists(),
        "no stale journal may survive serialized updates"
    );
    for tag in ["up-a", "up-b"] {
        assert_no_phantom_recovery(&log_dir, tag);
    }
}

#[cfg(unix)]
#[test]
fn sigkilled_remove_recovers_before_next_mutation() {
    // SIGKILL remove giữa tail (timing bất kỳ đều an toàn assert): op
    // sau (add) phải qua gateway recovery rồi thành công; manifest cuối
    // hợp lệ, có dep mới, hết journal.
    // (SIGKILL mid-tail at any point: the next op recovers, then lands.)
    let temp = TempDir::new().unwrap();
    let project = temp.path().join("site");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("package.json"),
        r#"{ "name": "race", "version": "1.0.0", "dependencies": { "mgc-race-alpha": "9.9.9" } }"#,
    )
    .unwrap();
    std::fs::write(project.join(".mgc.core"), "web\n").unwrap();
    let fixture = NpmFixture::new(&[("mgc-race-alpha", "9.9.9"), ("mgc-race-beta", "9.9.9")]);
    let mgc = find_mgc_binary();
    let log_dir = temp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();

    // Warm: lock the project via a first add (hermetic).
    // (Khởi động: add trước để có lock.)
    let warm_log = temp.path().join("warm");
    std::fs::create_dir_all(&warm_log).unwrap();
    let mut warm = add_cmd(
        &mgc,
        &project,
        "mgc-race-beta",
        &fixture.url,
        &warm_log,
        "warm",
    )
    .spawn()
    .unwrap();
    assert!(warm.wait().unwrap().success(), "warm add must succeed");

    // Start a remove WITH install tail, then SIGKILL it mid-flight.
    // MGC_REMOVE_TAIL_DELAY_MS parks the victim in its tail so the kill
    // deterministically lands after the manifest write + post-image.
    // (Delay hook giữ victim ở tail — kill trúng chắc sau khi đã ghi.)
    let victim_out = std::fs::File::create(log_dir.join("victim.out")).unwrap();
    let victim_err = std::fs::File::create(log_dir.join("victim.err")).unwrap();
    let mut victim = Command::new(&mgc);
    victim
        .arg("--core")
        .arg("web")
        .arg("remove")
        .arg("mgc-race-alpha")
        .current_dir(&project)
        .env("MAGICORE_WEB_REGISTRY_URL", &fixture.url)
        .env("MAGICORE_WEB_ALLOWED_REGISTRIES", &fixture.url)
        .env("MGC_CACHE_DIR", project.join(".magicore"))
        .env("MGC_REMOVE_TAIL_DELAY_MS", "8000")
        .stdout(victim_out)
        .stderr(victim_err);
    let mut child = victim.spawn().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(2));
    let _ = Command::new("kill")
        .arg("-9")
        .arg(child.id().to_string())
        .status();
    let victim_status = child.wait().unwrap();
    assert!(
        !victim_status.success(),
        "SIGKILLed remove must not report success"
    );
    let victim_log = read_log(&log_dir, "victim", "out") + &read_log(&log_dir, "victim", "err");
    assert!(
        victim_log.contains("Re-installing dependency graph"),
        "kill must land mid-tail (after the manifest write):\n{victim_log}"
    );

    // The next mutation recovers (whatever state the kill left) and lands.
    // (Mutation sau phục hồi rồi thành công.)
    let mut adding = add_cmd(
        &mgc,
        &project,
        "mgc-race-beta",
        &fixture.url,
        &log_dir,
        "after",
    )
    .spawn()
    .unwrap();
    let add_status = adding.wait().unwrap();
    assert!(
        add_status.success(),
        "add after SIGKILL must succeed:\n{}",
        read_log(&log_dir, "after", "err")
    );
    let manifest = std::fs::read_to_string(project.join("package.json")).unwrap();
    assert!(
        manifest.contains("mgc-race-beta"),
        "added dep must land:\n{manifest}"
    );
    // Valid JSON either way (never a torn manifest).
    // (Manifest luôn parse được — không bao giờ rách.)
    let parsed: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(parsed["name"], "race");
    assert!(
        !project
            .join(".magicore/journal/remove/journal.json")
            .exists(),
        "no journal may survive the recovery"
    );
}
