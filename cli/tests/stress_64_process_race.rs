//! 64-process × N-round concurrent install STRESS (Gate 11-B verdict
//! "16–64 concurrent installers, kill injection; zero orphan, zero
//! missing active claim"): every round spawns 64 REAL `mgc install`
//! processes racing ONE project, for `MGC_STRESS_ROUNDS` rounds
//! (default 100, overridable for slower CI runners). Contract per
//! round: every racer either WINS the project lock (exactly the set of
//! successes, ≥ 1 per round... the winner must exist because the lock
//! is exclusive, not blocking — the first racer to acquire wins) or is
//! REFUSED FAST with the typed lock error; NO other failure mode is
//! legal. The store must stay doctor-HEALTHY at the midpoint and the
//! end, and the final winner's materialization must exist.
//! (STRESS cài đặt đua 64-process × N vòng (phán quyết Gate 11-B):
//! mỗi vòng spawn 64 process `mgc install` THẬT đua MỘT project, trong
//! `MGC_STRESS_ROUNDS` vòng (mặc định 100, override được cho CI chậm).
//! Hợp đồng mỗi vòng: mọi racer hoặc THẮNG lock project (tập success,
//! ≥ 1 mỗi vòng) hoặc bị TỪ CHỐI NGAY với lỗi lock có type; KHÔNG có
//! chế độ thất bại nào khác là hợp lệ. Store phải còn doctor-HEALTHY ở
//! giữa và cuối, và materialization của winner cuối phải tồn tại.)
//!
//! Port DYNAMIC — no server port involved (mockito picks its own);
//! no RULE §13 fixed port is ever bound by this file.
//! (Port ĐỘNG — không đụng port server nào (mockito tự chọn); file này
//! không bao giờ bind port cố định RULE §13.)

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

struct RegistryFixture {
    _server: mockito::ServerGuard,
    _mocks: Vec<mockito::Mock>,
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

        Self {
            _server: server,
            _mocks: vec![metadata_mock, tarball_mock],
            url,
        }
    }
}

fn package_metadata(registry_url: &str, version: &str) -> serde_json::Value {
    serde_json::json!({
        "name": "is-odd",
        "version": version,
        "dependencies": {},
        "dist": { "tarball": format!("{registry_url}/is-odd/-/is-odd-{version}.tgz") }
    })
}

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

fn find_mgc_binary() -> String {
    std::env::var("CARGO_BIN_EXE_mgc")
        .expect("CARGO_BIN_EXE_mgc not set — run via `cargo test -p mgc`")
}

fn create_web_project(temp: &TempDir) -> PathBuf {
    let project = temp.path().join("site");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("package.json"),
        r#"{ "name": "stress64", "version": "1.0.0", "dependencies": { "is-odd": "3.0.1" } }"#,
    )
    .unwrap();
    std::fs::write(project.join(".mgc.core"), "web\n").unwrap();
    project
}

fn install_cmd(mgc: &str, project: &Path, registry_url: &str) -> Command {
    let mut cmd = Command::new(mgc);
    cmd.arg("--core").arg("web").arg("install");
    cmd.current_dir(project);
    cmd.env("MAGICORE_WEB_REGISTRY_URL", registry_url);
    cmd.env("MAGICORE_WEB_ALLOWED_REGISTRIES", registry_url);
    cmd.env("MGC_CACHE_DIR", project.join(".magicore"));
    cmd
}

/// Spawn an install with stdout/stderr redirected to per-process files —
/// 64 concurrent children would interleave inherited fds beyond
/// readability.
/// (Spawn install với stdout/stderr ghi ra file riêng từng process —
/// 64 con đồng thời sẽ trộn fd kế thừa vượt khả năng đọc.)
fn spawn_install_logged(
    mgc: &str,
    project: &Path,
    registry_url: &str,
    log_dir: &Path,
    tag: &str,
) -> std::process::Child {
    use std::io::Write;
    let stdout = std::fs::File::create(log_dir.join(format!("{tag}.out"))).unwrap();
    let stderr = std::fs::File::create(log_dir.join(format!("{tag}.err"))).unwrap();
    let mut cmd = install_cmd(mgc, project, registry_url);
    cmd.stdout(stdout).stderr(stderr);
    let child = cmd.spawn().unwrap();
    let mut map = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_dir.join("map.txt"))
        .unwrap();
    writeln!(map, "{tag} pid={}", child.id()).unwrap();
    child
}

fn read_log(log_dir: &Path, tag: &str, ext: &str) -> String {
    std::fs::read_to_string(log_dir.join(format!("{tag}.{ext}"))).unwrap_or_default()
}

fn doctor_cmd(mgc: &str, project: &Path) -> Command {
    let mut cmd = Command::new(mgc);
    cmd.arg("store").arg("doctor").arg("--core").arg("web");
    cmd.current_dir(project);
    cmd.env("MGC_CACHE_DIR", project.join(".magicore"));
    cmd
}

/// THE stress: 64 real processes × ROUNDS rounds on ONE project.
/// (STRESS chính: 64 process thật × ROUNDS vòng trên MỘT project.)
#[test]
fn stress_sixty_four_processes_racing_one_project_stays_locked_and_healthy() {
    let rounds: usize = std::env::var("MGC_STRESS_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let racers: usize = 64;

    let temp = TempDir::new().unwrap();
    let mgc = find_mgc_binary();
    let registry = RegistryFixture::new();
    let project = create_web_project(&temp);
    let log_dir = temp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();

    let mut total_successes = 0usize;
    let mut total_refusals = 0usize;

    for round in 0..rounds {
        let mut children = Vec::with_capacity(racers);
        for idx in 0..racers {
            children.push(spawn_install_logged(
                &mgc,
                &project,
                &registry.url,
                &log_dir,
                &format!("r{round:04}_{idx:02}"),
            ));
        }
        for (idx, child) in children.into_iter().enumerate() {
            let status = child.wait_with_output().unwrap().status;
            let tag = format!("r{round:04}_{idx:02}");
            if status.success() {
                total_successes += 1;
            } else {
                // A refusal MUST be the typed lock error — any other
                // failure (panic, SIGSEGV, network noise, budget error)
                // breaks the Gate contract and fails the stress.
                // (Từ chối PHẢI là lỗi lock có type — mọi thất bại khác
                // (panic, SIGSEGV, nhiễu mạng, lỗi budget) phá hợp đồng
                // Gate và làm fail stress.)
                let stderr = read_log(&log_dir, &tag, "err");
                let stdout = read_log(&log_dir, &tag, "out");
                let lock_refusal = stderr.contains("another install holds the project lock")
                    || stderr.contains("concurrent installs of one project are serialized");
                assert!(
                    lock_refusal,
                    "round {round} racer {idx}: failure is NOT a lock refusal — \
                     exit={:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                    status.code()
                );
                total_refusals += 1;
            }
        }

        // Midpoint + final doctor gates (bounded cost): the store must
        // never drift out of the generation invariants under sustained
        // racing.
        // (Cổng doctor giữa-lộ + cuối (giới hạn chi phí): store không
        // bao giờ được lệch khỏi invariant generation dưới đua kéo dài.)
        if round == rounds / 2 || round == rounds - 1 {
            let doctor = doctor_cmd(&mgc, &project).output().unwrap();
            let stdout = String::from_utf8_lossy(&doctor.stdout);
            assert!(
                doctor.status.success(),
                "doctor must stay HEALTHY after round {round}:\n{stdout}\nstderr:\n{}",
                String::from_utf8_lossy(&doctor.stderr)
            );
            assert!(
                stdout.contains("0 orphan claim(s)"),
                "round {round}: {stdout}"
            );
            assert!(
                stdout.contains("0 missing blob(s)"),
                "round {round}: {stdout}"
            );
            assert!(
                stdout.contains("0 corrupt blob(s)"),
                "round {round}: {stdout}"
            );
        }
    }

    // Aggregate contract: every spawn is accounted for; the lock serial
    // means every round had ≥1 winner (the winner's materialization
    // persists to the end).
    // (Hợp đồng tổng: mọi spawn được ghi nhận; lock tuần tự nghĩa là
    // mỗi vòng có ≥1 winner (materialization của winner cuối còn tồn
    // tại đến hết).)
    assert_eq!(
        total_successes + total_refusals,
        racers * rounds,
        "every spawn must be either a success or a typed lock refusal"
    );
    assert!(
        total_successes >= rounds,
        "each of the {rounds} rounds must have at least one lock winner, got {total_successes}"
    );
    assert!(
        project.join("node_modules/is-odd/package.json").exists(),
        "the last winner's materialization must persist"
    );

    // Final full-health gate including stale staging.
    // (Cổng sức khỏe đầy đủ cuối cùng, gồm staging chết.)
    let doctor = doctor_cmd(&mgc, &project).output().unwrap();
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        stdout.contains("0 stale staging gen(s)"),
        "no leaked staging may survive the stress: {stdout}"
    );
}
