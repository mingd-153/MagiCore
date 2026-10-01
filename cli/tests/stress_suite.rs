//! Stress suite for concurrent installs and dependency-state edge cases.
//!
//! Test scenarios:
//! 1. 10 concurrent installs (parallelism stress)
//! 2. Process kill mid-install (graceful recovery)
//! 3. Corrupted CAS entries (integrity check)
//! 4. Lockfile tamper (detect + reject)
//! 5. Disk full simulation (graceful error)
//! 6. Network timeout (offline resilience)
//! 7. Race conditions (concurrent add/remove)

#![allow(clippy::unwrap_used)] // Test code

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn find_mgc_binary() -> String {
    // CARGO_BIN_EXE_mgc: cargo-provided path to the JUST-BUILT binary —
    // immune to stale target/debug/mgc picking up an older build.
    // CARGO_BIN_EXE_mgc: cargo trỏ tới binary VỪA BUILD — tránh lẫm
    // target/debug/mgc cũ là binary lỗi thời.
    std::env::var("CARGO_BIN_EXE_mgc")
        .expect("CARGO_BIN_EXE_mgc not set — run via `cargo test -p mgc`")
}

/// Run a CLI child with an isolated per-test home and CAS root.
/// (Chạy CLI con với HOME và CAS riêng cho từng test.)
fn isolated_command(mgc: &str, home: &Path) -> Command {
    // Keep all child CLI state out of the developer's real global store.
    // (Cô lập state tiến trình con khỏi store thật của developer.)
    fs::create_dir_all(home).expect("create isolated test home");
    let mut command = Command::new(mgc);
    command
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("MAGICORE_STORE_ROOT", home.join(".magicore/store/v3"));
    command
}

/// Find a regular CAS blob without following symlinks.
/// (Tìm blob CAS thường mà không đi theo symlink.)
fn first_regular_blob(root: &Path) -> Option<std::path::PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).ok()? {
            let entry = entry.ok()?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).ok()?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_file() {
                return Some(path);
            }
            if metadata.is_dir() {
                pending.push(path);
            }
        }
    }
    None
}

/// Assert that a CAS blob's actual digest matches its address.
/// (Xác nhận digest nội dung blob khớp địa chỉ CAS.)
fn assert_blob_matches_address(blob: &Path) {
    let bytes = fs::read(blob).expect("CAS blob must be readable");
    let expected_hash = blob
        .file_name()
        .and_then(|name| name.to_str())
        .expect("CAS blob filename must contain its digest");
    assert_eq!(
        blake3::hash(&bytes).to_hex().as_str(),
        expected_hash,
        "CAS bytes must match their content-addressed path"
    );
}

/// Create a deterministic project that depends only on the in-process
/// registry fixture. (Tạo project tất định chỉ phụ thuộc registry fixture.)
fn create_fixture_web_project(root: &Path, dependency_range: &str) {
    fs::write(
        root.join("package.json"),
        serde_json::json!({
            "name": "stress-fixture",
            "version": "1.0.0",
            "dependencies": { "is-odd": dependency_range }
        })
        .to_string(),
    )
    .unwrap();
    fs::write(root.join(".mgc.core"), "web\n").unwrap();
}

#[test]
fn test_10_concurrent_installs() {
    // P1.2 STRESS: 10 concurrent installs for bounded local/CI runtime.
    // Tests: parallelism, cache safety, no deadlocks

    println!("\n=== 10 Concurrent Installs Stress Test ===");

    let mgc = find_mgc_binary();
    let temp_base = TempDir::new().unwrap();
    let registry = RegistryFixture::new();
    let isolated_home = temp_base.path().join("isolated-home");
    let results = Arc::new(Mutex::new(Vec::new()));

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let mgc = mgc.clone();
            let temp_base = temp_base.path().to_path_buf();
            let registry_url = registry.url.clone();
            let isolated_home = isolated_home.clone();
            let results = Arc::clone(&results);

            thread::spawn(move || {
                let project_dir = temp_base.join(format!("project_{}", i));
                fs::create_dir_all(&project_dir).unwrap();
                create_fixture_web_project(&project_dir, "3.0.1");

                let start = std::time::Instant::now();
                let output =
                    install_cmd_with_home(&mgc, &project_dir, &registry_url, &isolated_home)
                        .output()
                        .expect("Failed to run mgc install");

                let duration = start.elapsed();
                let success = output.status.success();

                results.lock().unwrap().push((i, success, duration));

                if !success {
                    eprintln!(
                        "Project {} failed:\n{}",
                        i,
                        String::from_utf8_lossy(&output.stderr)
                    );
                }

                success
            })
        })
        .collect();

    // Wait for all threads
    let outcomes: Vec<bool> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    let results = results.lock().unwrap();
    let successful = outcomes.iter().filter(|&&s| s).count();
    let failed = outcomes.len() - successful;

    println!("\n=== Results ===");
    println!("Total: 10");
    println!("Successful: {}", successful);
    println!("Failed: {}", failed);

    if !results.is_empty() {
        let avg_duration: Duration =
            results.iter().map(|(_, _, d)| *d).sum::<Duration>() / results.len() as u32;
        println!("Average duration: {:?}", avg_duration);
    }

    assert!(
        successful == 10,
        "all hermetic concurrent installs must succeed: {}/10",
        successful
    );

    println!(
        "10 concurrent installs: {}% success rate ({}/ 10)",
        (successful * 100) / 10,
        successful
    );
}

#[test]
fn test_corrupted_cas_detection() {
    // P1.2 STRESS: Corrupted CAS entry detection
    // Tests: integrity check, graceful recovery

    println!("\n=== Corrupted CAS Detection Test ===");

    let temp = TempDir::new().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let registry = RegistryFixture::new();
    fs::write(
        project.join("package.json"),
        r#"{"name":"cas-corruption-test","version":"1.0.0","dependencies":{"is-odd":"3.0.1"}}"#,
    )
    .unwrap();
    fs::write(project.join(".mgc.core"), "web\n").unwrap();
    let mgc = find_mgc_binary();

    // Step 1: Normal install to populate CAS
    let output1 = install_cmd(&mgc, &project, &registry.url)
        .output()
        .expect("Failed to run mgc install");

    assert!(
        output1.status.success(),
        "Initial install failed:\n{}",
        String::from_utf8_lossy(&output1.stderr)
    );

    println!("Initial install successful");

    // Step 2: Corrupt one blob in this project's isolated CAS, never the
    // developer's global store. (Làm hỏng blob trong CAS cô lập của project.)
    let cas_blobs = project.join(".magicore/cache/web/cas/files/blake3");
    let blob = first_regular_blob(&cas_blobs)
        .expect("install must produce a regular CAS blob for the fixture package");
    fs::write(&blob, b"CORRUPTED_DATA").expect("corrupt isolated CAS blob");
    println!("Corrupted isolated CAS blob: {:?}", blob);

    // Force the next install to consume the cache instead of taking an
    // already-materialized node_modules fast path. (Bắt buộc install sau
    // phải dùng cache, không được đi đường tắt node_modules có sẵn.)
    fs::remove_dir_all(project.join("node_modules"))
        .expect("remove project materialization before cache-reuse check");

    // Step 3: Try install again with corrupted CAS
    let output2 = install_cmd(&mgc, &project, &registry.url)
        .output()
        .expect("Failed to run mgc install");

    if output2.status.success() {
        let repaired =
            fs::read(&blob).expect("CAS blob should exist after successful install recovery");
        assert_ne!(
            repaired, b"CORRUPTED_DATA",
            "successful install must not leave corrupted bytes in the CAS"
        );
        assert_blob_matches_address(&blob);
        println!("Corrupt CAS blob was rejected and repaired from verified data");
    } else {
        let stderr = String::from_utf8_lossy(&output2.stderr);
        assert!(
            stderr.contains("integrity")
                || stderr.contains("checksum")
                || stderr.contains("corrupt")
                || stderr.contains("hash mismatch"),
            "failed install must report the cache-integrity cause:\n{stderr}"
        );
        assert!(
            !project.join("node_modules/is-odd/index.js").exists(),
            "failed reinstall must not materialize the corrupted package"
        );
        println!("Corrupt CAS blob was rejected; reinstall failed closed without materialization");

        // The first attempt quarantines the bad blob. A clean retry must
        // recover from verified package data and restore the CAS address.
        // (Lần đầu cách ly blob lỗi; retry sạch phải khôi phục từ dữ liệu
        // package đã xác minh và phục hồi đúng địa chỉ CAS.)
        let retry = install_cmd(&mgc, &project, &registry.url)
            .output()
            .expect("retry mgc install after corrupt blob quarantine");
        assert!(
            retry.status.success(),
            "install retry should succeed after corrupt blob quarantine:\n{}",
            String::from_utf8_lossy(&retry.stderr)
        );
        assert_blob_matches_address(&blob);
        assert!(project.join("node_modules/is-odd/index.js").is_file());
        println!("Retry repaired the quarantined CAS blob from verified data");
    }
}

#[test]
fn test_extra_web_lock_entry_cannot_drive_install() {
    // An orphan package injected into mgc.lock must not make install fetch or
    // materialize a package absent from the project manifest.

    let temp = TempDir::new().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let registry = RegistryFixture::new();
    create_fixture_web_project(&project, "3.0.1");
    let isolated_home = temp.path().join("isolated-home");

    let mgc = find_mgc_binary();

    // Step 1: Normal install to create lockfile
    let output1 = install_cmd_with_home(&mgc, &project, &registry.url, &isolated_home)
        .output()
        .expect("Failed to run mgc install");

    assert!(
        output1.status.success(),
        "Initial install failed:\n{}",
        String::from_utf8_lossy(&output1.stderr)
    );

    let lockfile = project.join("mgc.lock");
    assert!(lockfile.exists(), "Lockfile not created");

    println!("Initial install + lockfile created");

    // Step 2: Add an orphan Web-owned entry pointing at a local tripwire.
    let lock_content = fs::read_to_string(&lockfile).unwrap();
    let mut lock = mgc_lockfile::parser::parse_lockfile(&lock_content).unwrap();
    let mut orphan = mgc_lockfile::Package::new(
        "__tampered__".to_string(),
        "1.0.0".to_string(),
        format!("{}/__tampered__/-/__tampered__-1.0.0.tgz", registry.url),
        "sha512-fake".to_string(),
    );
    orphan.ecosystem = mgc_lockfile::EcosystemTag::Web;
    orphan.owner_core = Some("web".to_string());
    lock.packages.push(orphan);
    let tampered = mgc_lockfile::writer::serialize_lockfile(&lock).unwrap();
    fs::write(&lockfile, &tampered).unwrap();

    // Step 3: Install must use only the manifest's reachable dependency graph.
    let output2 = install_cmd_with_home(&mgc, &project, &registry.url, &isolated_home)
        .output()
        .expect("Failed to run mgc install");
    assert!(
        output2.status.success(),
        "orphan lock entry must not poison an otherwise valid install:\n{}",
        String::from_utf8_lossy(&output2.stderr)
    );
    registry.tampered_tarball.assert();
    let repaired = mgc_lockfile::parser::load_lockfile(&lockfile).unwrap();
    assert!(
        !repaired
            .packages
            .iter()
            .any(|package| package.name == "__tampered__"),
        "unreachable package must be removed from the MGC-owned Web lock slice"
    );
    assert!(project.join("node_modules/is-odd/index.js").is_file());
}

#[test]
fn test_remove_last_dependency_prunes_lock() {
    // Removing the last root dependency must update the manifest, lock,
    // and materialized tree as one coherent result.
    let temp = TempDir::new().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let registry = RegistryFixture::new();
    create_fixture_web_project(&project, "3.0.1");
    let home = temp.path().join("isolated-home");
    let mgc = find_mgc_binary();

    let install = install_cmd_with_home(&mgc, &project, &registry.url, &home)
        .output()
        .expect("run initial fixture install");
    assert!(
        install.status.success(),
        "initial install failed:\n{}",
        String::from_utf8_lossy(&install.stderr)
    );
    assert!(project.join("node_modules/is-odd/index.js").is_file());

    let remove = web_command_with_home(&mgc, &project, &registry.url, &home)
        .arg("remove")
        .arg("is-odd")
        .output()
        .expect("run native mgc remove");
    assert!(
        remove.status.success(),
        "remove last dependency failed:\n{}",
        String::from_utf8_lossy(&remove.stderr)
    );

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(project.join("package.json")).unwrap()).unwrap();
    assert!(manifest["dependencies"].get("is-odd").is_none());
    let lock = mgc_lockfile::parser::load_lockfile(&project.join("mgc.lock")).unwrap();
    assert!(
        !lock.packages.iter().any(|package| {
            package.ecosystem == mgc_lockfile::EcosystemTag::Web && package.name == "is-odd"
        }),
        "removing the final dependency must remove its stale mgc.lock entry"
    );
    assert!(
        !project.join("node_modules/is-odd").exists(),
        "removing the final dependency must remove its materialized package"
    );
}

#[test]
fn test_race_condition_add_remove() {
    // Race both operations from an empty project. Whichever acquires the
    // project lock first, the successful serialized operations must leave
    // manifest, lock, and materialized tree in agreement.
    println!("\n=== Race Condition: Concurrent Add/Remove ===");

    let temp = TempDir::new().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let registry = RegistryFixture::new();
    create_fixture_web_project(&project, "*");
    let home = temp.path().join("isolated-home");
    let mgc = find_mgc_binary();
    let barrier = Arc::new(std::sync::Barrier::new(2));

    let mgc1 = mgc.clone();
    let mgc2 = mgc.clone();
    let project1 = project.clone();
    let project2 = project.clone();
    let url1 = registry.url.clone();
    let url2 = registry.url.clone();
    let home1 = home.clone();
    let home2 = home.clone();
    let barrier1 = Arc::clone(&barrier);
    let barrier2 = Arc::clone(&barrier);

    let add = thread::spawn(move || {
        barrier1.wait();
        web_command_with_home(&mgc1, &project1, &url1, &home1)
            .arg("add")
            .arg("is-odd@3.0.1")
            .output()
    });
    let remove = thread::spawn(move || {
        barrier2.wait();
        web_command_with_home(&mgc2, &project2, &url2, &home2)
            .arg("remove")
            .arg("is-odd")
            .output()
    });

    let add = add.join().unwrap().expect("add process must spawn");
    let remove = remove.join().unwrap().expect("remove process must spawn");
    assert!(
        add.status.success(),
        "serialized add failed:\n{}",
        String::from_utf8_lossy(&add.stderr)
    );
    assert!(
        remove.status.success(),
        "serialized remove failed:\n{}",
        String::from_utf8_lossy(&remove.stderr)
    );

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(project.join("package.json")).unwrap())
            .expect("package.json must remain valid after concurrent mutation");
    let declared = manifest["dependencies"].get("is-odd").is_some();
    let lock = mgc_lockfile::parser::load_lockfile(&project.join("mgc.lock"))
        .expect("mgc.lock must remain valid after concurrent mutation");
    let locked = lock.packages.iter().any(|package| {
        package.ecosystem == mgc_lockfile::EcosystemTag::Web && package.name == "is-odd"
    });
    let materialized = project.join("node_modules/is-odd/index.js").is_file();
    assert_eq!(declared, locked, "manifest and lock must agree after race");
    assert_eq!(
        declared, materialized,
        "manifest and node_modules must agree after race"
    );
}

#[test]
#[ignore = "Requires specific disk quota setup"]
fn test_disk_full_graceful_error() {
    // P1.2 STRESS: Disk full simulation
    // Tests: graceful error, no partial state

    println!("\n=== Disk Full Graceful Error Test ===");

    // This test requires manual setup:
    // 1. Create small disk image: hdiutil create -size 10m -fs HFS+ -volname TestDisk test.dmg
    // 2. Mount: hdiutil attach test.dmg
    // 3. Set project root to mounted volume
    // 4. Run test
    // 5. Unmount: hdiutil detach /Volumes/TestDisk

    println!("WARN: MANUAL TEST - requires disk quota/small volume");
    println!("See test source for setup instructions");
}

#[test]
fn test_network_timeout_offline_mode() {
    // P1.2 STRESS: Network timeout resilience
    // Tests: offline mode works, stale metadata warning

    println!("\n=== Network Timeout / Offline Mode Test ===");

    let temp = TempDir::new().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let isolated_home = temp.path().join("isolated-home");

    let mgc = find_mgc_binary();
    let registry = RegistryFixture::new();
    create_fixture_web_project(&project, "3.0.1");

    // Step 1: Install from the hermetic registry to populate cache + lock.
    let output1 = install_cmd_with_home(&mgc, &project, &registry.url, &isolated_home)
        .output()
        .expect("Failed to run mgc install");

    assert!(
        output1.status.success(),
        "Initial fixture install failed:\n{}",
        String::from_utf8_lossy(&output1.stderr)
    );
    println!("Initial fixture install successful");

    // Step 2: Point at an unreachable registry and prove the warm offline
    // install succeeds without depending on public network state.
    // (Trỏ sang registry không thể truy cập và chứng minh offline install
    // chạy được từ cache ấm, không phụ thuộc mạng công cộng.)
    let output2 = install_cmd_with_home(&mgc, &project, "http://127.0.0.1:9", &isolated_home)
        .arg("--offline")
        .output()
        .expect("Failed to run mgc install --offline");

    assert!(
        output2.status.success(),
        "warm offline reinstall should succeed with unreachable registry:\n{}",
        String::from_utf8_lossy(&output2.stderr)
    );
    assert!(project.join("node_modules/is-odd/index.js").is_file());
    println!("Offline reinstall succeeded from verified warm cache");
}

// In-process mock npm registry — mirrors cli/tests/kill_injection_matrix.rs.
// (Registry npm giả in-process — phản chiếu cli/tests/kill_injection_matrix.rs.)
// R2 flake root cause: the old manifest pulled 4 real packages from the
// PUBLIC npmjs.org registry; under full-suite load, slow resolution delayed
// the READY park past the 60s poll. A single tiny mock package keeps the
// handshake deterministic and network-free.
// (Nguyên nhân flake R2: manifest cũ kéo 4 package thật từ registry
// npmjs.org CÔNG CỘNG; dưới tải full-suite, resolve chậm đẩy điểm đỗ READY
// qua mốc poll 60s. Một package giả nhỏ giúp handshake tất định, không mạng.)

/// Mock registry fixture serving exactly one package: is-odd 3.0.1.
/// (Fixture registry giả phục vụ đúng một package: is-odd 3.0.1.)
struct RegistryFixture {
    _server: mockito::ServerGuard,
    _mocks: Vec<mockito::Mock>,
    tampered_tarball: mockito::Mock,
    url: String,
}

impl RegistryFixture {
    fn new() -> Self {
        let mut server = mockito::Server::new();
        let url = server.url();
        let metadata = serde_json::json!({
            "name": "is-odd",
            "dist-tags": { "latest": "3.0.1" },
            "versions": { "3.0.1": package_metadata(&url, "3.0.1") }
        });
        let metadata_mock = server
            .mock("GET", "/is-odd")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(metadata.to_string())
            .expect_at_least(1)
            .create();
        let tarball_mock = server
            .mock("GET", "/is-odd/-/is-odd-3.0.1.tgz")
            .with_status(200)
            .with_header("content-type", "application/octet-stream")
            .with_body(package_tarball())
            .expect_at_least(1)
            .create();
        let tampered_tarball = server
            .mock("GET", "/__tampered__/-/__tampered__-1.0.0.tgz")
            .with_status(500)
            .expect(0)
            .create();

        Self {
            _server: server,
            _mocks: vec![metadata_mock, tarball_mock],
            tampered_tarball,
            url,
        }
    }
}

/// Minimal packument for the mock is-odd version.
/// (Packument tối thiểu cho phiên bản is-odd giả.)
fn package_metadata(registry_url: &str, version: &str) -> serde_json::Value {
    serde_json::json!({
        "name": "is-odd",
        "version": version,
        "dependencies": {},
        "dist": { "tarball": format!("{registry_url}/is-odd/-/is-odd-{version}.tgz") }
    })
}

/// Deterministic in-memory tarball for is-odd 3.0.1 — no public network.
/// (Tarball in-memory tất định cho is-odd 3.0.1 — không mạng công cộng.)
fn package_tarball() -> Vec<u8> {
    let package_json = serde_json::json!({
        "name": "is-odd",
        "version": "3.0.1",
        "main": "index.js"
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
    let index = "module.exports = function isOdd(n){return Math.abs(n % 2) === 1;};";
    let mut header = tar::Header::new_gnu();
    header.set_size(index.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive
        .append_data(&mut header, "package/index.js", index.as_bytes())
        .unwrap();
    let encoder = archive.into_inner().unwrap();
    encoder.finish().unwrap()
}

/// Install command pinned to the mock registry with the cache inside the
/// temp project — fully hermetic, same shape as kill_injection_matrix.
/// (Lệnh install ghim registry giả với cache nằm trong project tạm — kín
/// hoàn toàn, cùng dạng kill_injection_matrix.)
fn install_cmd(mgc: &str, project: &Path, registry_url: &str) -> Command {
    let home = project.join(".test-home");
    install_cmd_with_home(mgc, project, registry_url, &home)
}

/// Install through the hermetic web lane with an explicit HOME/store owner.
/// (Install lane web kín với HOME/store owner tường minh.)
fn install_cmd_with_home(mgc: &str, project: &Path, registry_url: &str, home: &Path) -> Command {
    let mut command = web_command_with_home(mgc, project, registry_url, home);
    command.arg("install");
    command
}

/// Build a hermetic web command with explicit project and store roots.
/// (Tạo web command kín với project/store root tường minh.)
fn web_command_with_home(mgc: &str, project: &Path, registry_url: &str, home: &Path) -> Command {
    let mut cmd = isolated_command(mgc, home);
    cmd.arg("--core").arg("web");
    cmd.env("MAGICORE_WEB_REGISTRY_URL", registry_url);
    cmd.env("MAGICORE_WEB_ALLOWED_REGISTRIES", registry_url);
    cmd.env("MGC_CACHE_DIR", project.join(".magicore"));
    cmd.current_dir(project);
    cmd
}

/// Failpoint where the killed install parks: the first lifecycle phase, the
/// earliest deterministic observation point (see kill_injection_matrix.rs).
/// (Failpoint để install bị kill đỗ: phase đầu của dòng đời — điểm quan sát
/// sớm nhất tất định (xem kill_injection_matrix.rs).)
const KILL_PHASE: &str = "after-generation-begin";

/// READY-marker poll timeout (seconds) — matches kill_injection_matrix's
/// default. (Timeout poll marker READY (giây) — khớp mặc định của
/// kill_injection_matrix.)
const READY_TIMEOUT_SECS: u64 = 60;

/// Poll interval between marker-file reads (ms) — matches
/// kill_injection_matrix. (Khoảng poll giữa các lần đọc marker file (ms) —
/// khớp kill_injection_matrix.)
const MARKER_POLL_INTERVAL_MS: u64 = 10;

/// Poll the marker file until it contains `READY:<phase>` or times out.
/// The child fsyncs the marker, so a successful read is a reliable
/// readiness signal — no guessed sleeps.
/// (Poll marker file tới khi chứa `READY:<phase>` hoặc hết giờ. Child fsync
/// marker nên đọc thành công là tín hiệu sẵn sàng tin cậy — không đoán trễ.)
fn wait_for_failpoint_ready(marker: &Path, phase: &str, timeout: Duration) -> bool {
    let needle = format!("READY:{phase}");
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if fs::read_to_string(marker)
            .map(|contents| contents.contains(&needle))
            .unwrap_or(false)
        {
            return true;
        }
        thread::sleep(Duration::from_millis(MARKER_POLL_INTERVAL_MS));
    }
    false
}

/// Assert the child died by SIGKILL (Unix) — the deterministic handshake must
/// not be fooled by a child that exited cleanly before the kill landed.
/// (Khẳng định child chết vì SIGKILL (Unix) — handshake tất định không được
/// bị lừa bởi child đã exit sạch trước khi kill kịp rơi.)
#[cfg(unix)]
fn assert_killed_by_signal(status: &std::process::ExitStatus, phase: &str) {
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(
        status.signal(),
        Some(libc::SIGKILL),
        "{phase}: child must die by SIGKILL (not exit cleanly), got {status:?}"
    );
}

#[cfg(not(unix))]
fn assert_killed_by_signal(status: &std::process::ExitStatus, phase: &str) {
    assert!(
        !status.success(),
        "{phase}: child must not exit cleanly after kill, got {status:?}"
    );
}

#[test]
fn test_process_kill_recovery() {
    // P1.2 STRESS: Process kill mid-install recovery
    // Tests: lock cleanup, no corrupted state
    //
    // Gate 11-B.2 aftermath: the original 500ms blind sleep flaked under
    // full-suite load — the child could be killed before `install` even
    // began (the kill exercised nothing) or after it had already finished,
    // so the recovery path was never reliably hit. Replaced with the
    // deterministic failpoint handshake from cli/tests/kill_injection_matrix.rs:
    // the child parks at `after-generation-begin` and fsyncs a READY marker;
    // the test polls the marker, SIGKILLs, and asserts the child truly died
    // by signal.
    //
    // R2 flake fix (Tech Lead verdict 2026-09-16): the manifest used to list
    // 4 real packages (lodash/axios/react/next) resolved against the PUBLIC
    // npmjs.org registry — ~101s standalone, and under full-suite load the
    // READY poll (60s) missed because resolution crawled before the child
    // reached the failpoint. The test now runs against the in-process mockito
    // registry with ONE tiny dep (is-odd ^3.0.1), exactly like
    // kill_injection_matrix.rs: instant resolve, zero public network. The
    // recovery contract is unchanged: leftover lockfile-lock stays WARN-only
    // and the follow-up install (same mock registry) MUST succeed.
    //
    // (Sau Gate 11-B.2: sleep mù 500ms ban đầu hay flake khi full-suite tải
    // cao — child có thể bị kill trước khi `install` kịp bắt đầu (kill chẳng
    // test được gì) hoặc sau khi đã xong, nên đường recovery không bao giờ
    // được chạm tất định. Đã thay bằng handshake failpoint tất định theo mẫu
    // cli/tests/kill_injection_matrix.rs: child đỗ tại
    // `after-generation-begin` và fsync marker READY; test poll marker, rồi
    // SIGKILL và khẳng định child chết thật vì signal.
    //
    // Sửa flake R2 (phán quyết Tech Lead 2026-09-16): manifest cũ liệt kê 4
    // package thật (lodash/axios/react/next) resolve qua registry npmjs.org
    // CÔNG CỘNG — ~101s chạy đơn, và dưới tải full-suite poll READY (60s)
    // miss vì resolve chập chờn trước khi child tới failpoint. Test giờ chạy
    // qua registry mockito in-process với MỘT dep nhỏ (is-odd ^3.0.1), y hệt
    // kill_injection_matrix.rs: resolve tức thì, không mạng công cộng. Hợp
    // đồng recovery giữ nguyên: lockfile-lock sót lại chỉ WARN và install
    // tiếp theo (cùng registry giả) PHẢI thành công.)

    println!("\n=== Process Kill Recovery Test ===");

    let temp = TempDir::new().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(&project).unwrap();

    // Single tiny dep against the mock registry — instant fetch, hermetic.
    // (Một dep nhỏ duy nhất qua registry giả — tải tức thì, kín mạng.)
    fs::write(
        project.join("package.json"),
        r#"{ "name": "kill-test", "version": "1.0.0", "dependencies": { "is-odd": "^3.0.1" } }"#,
    )
    .unwrap();
    fs::write(project.join(".mgc.core"), "web\n").unwrap();

    let mgc = find_mgc_binary();
    let registry = RegistryFixture::new();

    // Start install parked at the failpoint; the child fsyncs the READY
    // marker when it reaches the park — deterministic handshake, no sleep.
    // Child stdout/stderr go to files so a failed handshake dumps the real
    // reason instead of a bare timeout.
    // (Bắt đầu install đỗ tại failpoint; child fsync marker READY khi tới
    // điểm đỗ — handshake tất định, không sleep. stdout/stderr của child ghi
    // ra file để handshake hỏng dump được nguyên nhân thật thay vì timeout
    // trống.)
    let ready_file = temp.path().join("ready");
    let log_dir = project.join("logs");
    fs::create_dir_all(&log_dir).unwrap();
    let stdout_f = fs::File::create(log_dir.join("victim.out")).unwrap();
    let stderr_f = fs::File::create(log_dir.join("victim.err")).unwrap();
    let mut cmd = install_cmd(&mgc, &project, &registry.url);
    cmd.env("MGC_FAILPOINT", KILL_PHASE);
    cmd.env("MGC_FAILPOINT_READY_FILE", &ready_file);
    cmd.stdout(stdout_f).stderr(stderr_f);
    let mut child = cmd.spawn().expect("Failed to spawn mgc install");

    let timeout = Duration::from_secs(READY_TIMEOUT_SECS);
    let ready = wait_for_failpoint_ready(&ready_file, KILL_PHASE, timeout);
    if !ready {
        // Always reap the child before failing — a parked process must not
        // leak past a failed assertion.
        // (Luôn thu hồi child trước khi fail — process đỗ không được rò rỉ
        // sau assertion thất bại.)
        let _ = child.kill();
        let _ = child.wait();
        let stderr = fs::read_to_string(log_dir.join("victim.err")).unwrap_or_default();
        panic!(
            "READY marker not observed within {READY_TIMEOUT_SECS}s at {KILL_PHASE} (failpoint handshake failed)\nstderr:\n{stderr}"
        );
    }

    println!("Killing process mid-install...");
    child.kill().expect("Failed to kill process");
    let status = child.wait().expect("Failed to reap killed child");

    // The handshake must not be fooled by a child that exited cleanly before
    // the kill landed: on Unix it must die by SIGKILL; on Windows a
    // terminated child exits nonzero.
    // (Handshake không được bị lừa bởi child exit sạch trước khi kill rơi:
    // trên Unix phải chết vì SIGKILL; trên Windows child bị terminate thì
    // exit nonzero.)
    assert_killed_by_signal(&status, KILL_PHASE);

    println!("Process killed (died by signal / exited nonzero)");

    // Verify: no lockfile lock remains. WARN-only by contract — the OS
    // auto-releases the flock, so a leftover file is hygiene, not failure.
    // (Kiểm tra: không còn lockfile lock. Chỉ WARN theo hợp đồng — OS tự nhả
    // flock nên file sót là vệ sinh, không phải failure.)
    let lockfile_lock = project.join("mgc.lock.lock");
    if lockfile_lock.exists() {
        println!("WARN: Lock file still exists (should be cleaned by signal handler)");
    } else {
        println!("Lock file cleaned up");
    }

    // Follow-up install against the SAME mock registry must succeed.
    // (Install tiếp theo qua CÙNG registry giả phải thành công.)
    let output2 = install_cmd(&mgc, &project, &registry.url)
        .output()
        .expect("Failed to run mgc install");

    assert!(
        output2.status.success(),
        "Install after process kill failed:\n{}",
        String::from_utf8_lossy(&output2.stderr)
    );

    println!("Recovery after process kill successful");
}

#[test]
fn test_frozen_mode_blocks_lockfile_mutation() {
    // P1.2 STRESS: Frozen mode must fail when lockfile needs update
    // Tests: frozen flag enforcement, CI reproducibility

    println!("\n=== Frozen Mode Lockfile Protection ===");

    let mgc = find_mgc_binary();
    let temp = TempDir::new().unwrap();
    let project = temp.path();
    let isolated_home = temp.path().join("isolated-home");
    let registry = RegistryFixture::new();
    create_fixture_web_project(project, "3.0.1");

    // Step 1: Initial install (creates lockfile)
    println!("Initial install to create lockfile...");
    let install1 = install_cmd_with_home(&mgc, project, &registry.url, &isolated_home)
        .output()
        .expect("Failed mgc install");

    assert!(
        install1.status.success(),
        "Initial install should succeed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&install1.stdout),
        String::from_utf8_lossy(&install1.stderr)
    );
    assert!(
        project.join("mgc.lock").exists(),
        "Lockfile should be created"
    );
    let original_lock = fs::read(project.join("mgc.lock")).unwrap();

    // Step 2: Change the locked package's requirement beyond its pin.
    create_fixture_web_project(project, "^4.0.0");

    // Step 3: Try install --frozen (should FAIL because lockfile outdated)
    println!("Attempting frozen install with modified manifest...");
    let install2 = install_cmd_with_home(&mgc, project, &registry.url, &isolated_home)
        .arg("--frozen")
        .output()
        .expect("Failed mgc install --frozen");

    // Frozen mode MUST fail when lockfile doesn't match manifest
    if install2.status.success() {
        panic!(
            "BUG: Frozen mode allowed lockfile mutation!\n\
             Manifest requirement changed but frozen install succeeded.\n\
             Frozen mode MUST fail when dependencies change."
        );
    }

    let stderr = String::from_utf8_lossy(&install2.stderr);
    println!("Frozen install correctly failed: {}", stderr);

    // Verify error mentions frozen or lockfile
    assert!(
        stderr.contains("frozen") || stderr.contains("lockfile") || stderr.contains("outdated"),
        "Error should mention frozen mode or lockfile mismatch"
    );
    assert_eq!(
        fs::read(project.join("mgc.lock")).unwrap(),
        original_lock,
        "frozen install must not mutate the existing lockfile"
    );

    println!("Frozen mode correctly blocked lockfile mutation");
}
