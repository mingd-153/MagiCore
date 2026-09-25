//! DETERMINISTIC SIGKILL-injection matrix across the install fault timeline
//! (Gate 11-B.2 "failpoint handshake" — the sleep-ladder from B.1 is now
//! REPLACED): each install phase parks via `mgc_store::failpoint::hit`, the
//! test polls the `READY:<phase>` marker (fsync'd by the child), then
//! SIGKILLs the child and asserts it truly died by signal — never a guessed
//! delay. The lifecycle is begin → fetch → CAS publish → claim → materialize
//! → promote → COMMIT, and every phase is exercised deterministically.
//!
//! Contract per phase, on a FRESH project:
//!   1. doctor gates (orphan / missing / corrupt) stay ZERO right after the
//!      kill — a killed install never corrupts the store;
//!   2. the FOLLOW-UP install SUCCEEDS (OS lock auto-release + staging
//!      supersede);
//!   3. the follow-up doctor is CONSISTENT with the stale verdict (0 stale ⇒
//!      exit 0; stale > 0 ⇒ exit nonzero) — a killed install MAY leave a
//!      designed crash footprint, detected and repaired, not an invariant.
//!      `doctor --repair` retires it, and the FINAL doctor is HEALTHY with
//!      0 stale staging; node_modules materialization exists.
//!   4. REPAIR-TWICE idempotency: a second `--repair` after the final doctor
//!      retires 0 more leases and the store stays HEALTHY.
//!
//! Expected stale per phase (classifier semantics, NOT forced into one mold):
//!   `after-generation-begin` → claim-less staging (STALE, leaked begin);
//!   `after-fetch` / `after-cas-publish` → claim-less staging (STALE);
//!   `after-first-claim` onward through `before-commit` → claim-ful staging
//!   whose hashes the baseline's promoted refset already covers (STALE);
//!   `after-generation-flip` / `after-commit` → generation already promoted
//!   (0 stale ⇒ HEALTHY). Any phase the classifier judges otherwise (e.g.
//!   RETAINED over-retention) is asserted per its actual disposition.
//!
//! (MA TRẬN SIGKILL TẤT ĐỊNH qua mọi điểm fault của dòng đời install: mỗi
//! phase đỗ qua `hit`, test poll marker `READY:<phase>` (child fsync), rồi
//! SIGKILL và khẳng định chết THẬT vì signal — không còn đoán trễ. Dòng đời
//! là begin → fetch → publish CAS → claim → materialize → promote → COMMIT,
//! mọi phase được phủ tất định. Hợp đồng mỗi phase, trên project MỚI:
//! (1) cổng doctor (orphan/missing/corrupt) bằng 0 ngay sau kill; (2) install
//! TIẾP THEO THÀNH CÔNG; (3) doctor sau follow-up NHẤT QUÁN với verdict stale
//! (0 stale ⇒ exit 0; stale > 0 ⇒ exit nonzero) — vết crash theo thiết kế
//! được phát hiện và sửa, không phải invariant; `--repair` nghỉ hưu và doctor
//! CUỐI HEALTHY 0 staging chết; materialization tồn tại. (4) Idempotency
//! REPAIR-TWICE: `--repair` lần HAI nghỉ hưu 0 lease, store vẫn HEALTHY.
//! Kỳ vọng stale mỗi phase (theo ngữ nghĩa classifier, KHÔNG ép một khuôn):
//! after-generation-begin → staging không claim (STALE); after-fetch /
//! after-cas-publish → staging không claim (STALE); from after-first-claim
//! tới before-commit → staging có claim mà hash đã được promoted baseline
//! phủ (STALE); after-generation-flip / after-commit → generation đã promoted
//! (0 stale ⇒ HEALTHY). Phase nào classifier đánh giá khác (vd RETAINED giữ
//! thừa) thì assert theo đúng disposition thực tế.)

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// The ten install phases exercised by this matrix, in lifecycle order.
/// (Mười phase install được phủ trong ma trận, theo thứ tự dòng đời.)
const PHASES: &[&str] = &[
    "after-generation-begin",
    "after-first-claim",
    "after-fetch",
    "after-cas-publish",
    "before-materialize",
    "after-materialize",
    "before-promote",
    "after-generation-flip",
    "before-commit",
    "after-commit",
];

/// Default kill count per phase (`MGC_FAILPOINT_REPS` overrides — RULE §12).
/// (Số lần kill mặc định mỗi phase (`MGC_FAILPOINT_REPS` override).)
const DEFAULT_FAILPOINT_REPS: usize = 3;

/// Default readiness-marker poll timeout in seconds.
/// (Timeout poll marker READY mặc định, giây.)
const DEFAULT_READY_TIMEOUT_SECS: u64 = 60;

/// Poll interval between marker-file reads (ms).
/// (Khoảng poll giữa các lần đọc marker file, ms.)
const MARKER_POLL_INTERVAL_MS: u64 = 10;

fn failpoint_reps() -> usize {
    std::env::var("MGC_FAILPOINT_REPS")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|reps| *reps > 0)
        .unwrap_or(DEFAULT_FAILPOINT_REPS)
}

fn failpoint_ready_timeout() -> Duration {
    std::env::var("MGC_FAILPOINT_READY_TIMEOUT_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(DEFAULT_READY_TIMEOUT_SECS))
}

/// Poll the marker file until it contains `READY:<phase>` or the timeout
/// elapses. The child fsyncs the marker, so a successful read is a reliable
/// readiness signal — no guessed sleeps.
/// (Poll marker file tới khi chứa `READY:<phase>` hoặc hết timeout. Child
/// fsync marker nên đọc thành công là tín hiệu sẵn sàng tin cậy — không đoán.)
fn wait_for_failpoint_ready(marker: &Path, phase: &str, timeout: Duration) -> bool {
    let needle = format!("READY:{phase}");
    let start = Instant::now();
    while start.elapsed() < timeout {
        if std::fs::read_to_string(marker)
            .map(|contents| contents.contains(&needle))
            .unwrap_or(false)
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(MARKER_POLL_INTERVAL_MS));
    }
    false
}

/// Assert the child died by SIGKILL (Unix) — the deterministic handshake must
/// not be fooled by a child that exited cleanly before the kill landed.
/// (Khẳng định child chết vì SIGKILL (Unix) — handshake tất định không được
/// bị lừa bởi child đã exit sạch trước khi kill kịp rơi.)
#[cfg(unix)]
fn assert_killed_by_sigkill(status: &std::process::ExitStatus, phase: &str, rep: usize) {
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(
        status.signal(),
        Some(libc::SIGKILL),
        "{phase} rep {rep}: child must die by SIGKILL (not exit cleanly), got {status:?}"
    );
}

#[cfg(not(unix))]
fn assert_killed_by_sigkill(status: &std::process::ExitStatus, phase: &str, rep: usize) {
    assert!(
        !status.success(),
        "{phase} rep {rep}: child must not exit cleanly after kill, got {status:?}"
    );
}

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

fn create_web_project(temp: &TempDir, name: &str) -> PathBuf {
    let project = temp.path().join(name);
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("package.json"),
        r#"{ "name": "victim", "version": "1.0.0", "dependencies": { "is-odd": "3.0.1" } }"#,
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

fn doctor_cmd(mgc: &str, project: &Path) -> Command {
    let mut cmd = Command::new(mgc);
    cmd.arg("store").arg("doctor").arg("--core").arg("web");
    cmd.current_dir(project);
    cmd.env("MGC_CACHE_DIR", project.join(".magicore"));
    cmd
}

fn doctor_repair_cmd(mgc: &str, project: &Path) -> Command {
    let mut cmd = doctor_cmd(mgc, project);
    cmd.arg("--repair");
    cmd
}

fn doctor_gates_zero(mgc: &str, project: &Path, context: &str) {
    let doctor = doctor_cmd(mgc, project).output().unwrap();
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        stdout.contains("0 orphan claim(s)"),
        "{context}: kill injection must not orphan claims: {stdout}"
    );
    assert!(
        stdout.contains("0 missing blob(s)"),
        "{context}: kill injection must not lose blobs: {stdout}"
    );
    assert!(
        stdout.contains("0 corrupt blob(s)"),
        "{context}: kill injection must not corrupt blobs: {stdout}"
    );
}

/// THE matrix: one fresh project per phase × rep; baseline install; spawn a
/// re-install parked at `phase`; poll the fsync'd READY marker; SIGKILL;
/// assert the kill landed; then run the three-phase contract plus
/// repair-twice idempotency.
/// (MA TRẬN: một project mới mỗi phase × rep; install baseline; spawn
/// install-lại đỗ tại `phase`; poll marker READY đã fsync; SIGKILL; khẳng
/// định kill rơi thật; rồi chạy hợp đồng ba pha cộng idempotency repair-twice.)
#[test]
fn failpoint_kill_matrix_never_corrupts_the_store() {
    let reps = failpoint_reps();
    let timeout = failpoint_ready_timeout();
    let temp = TempDir::new().unwrap();
    let mgc = find_mgc_binary();
    let registry = RegistryFixture::new();
    let markers_dir = temp.path().join("markers");
    std::fs::create_dir_all(&markers_dir).unwrap();

    for phase in PHASES {
        for rep in 0..reps {
            let project = create_web_project(&temp, &format!("victim-{phase}-{rep}"));

            // Baseline: a healthy install so the victim races against a
            // populated store (the promote-side fault points need an
            // existing promoted refset to classify against).
            // (Baseline: install khỏe để victim đua trên store đã có — các
            // điểm fault phía promote cần refset promoted hiện hữu để đối chiếu.)
            let out = install_cmd(&mgc, &project, &registry.url)
                .output()
                .unwrap_or_else(|e| panic!("{phase} rep {rep}: baseline spawn failed: {e}"));
            assert!(
                out.status.success(),
                "{phase} rep {rep}: baseline install failed:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );

            // The victim must RE-RUN the full pipeline, not take the warm-
            // install shortcuts that skip past every failpoint: (1) clear
            // node_modules so the install no-op fast path is skipped, and
            // (2) clear the SHARED web cache (extracted-package roots +
            // resolution + tarballs) so extraction actually re-runs and the
            // `after-cas-publish` / `after-first-claim` failpoints can fire.
            // The per-project STORE (store.db + CAS at `.magicore/cache/web`)
            // stays populated so the victim races the promoted refset.
            // (Victim phải CHẠY LẠI trọn pipeline, không đi đường tắt warm-
            // install lướt qua mọi failpoint: (1) xóa node_modules để bỏ qua
            // đường tắt no-op của install, và (2) xóa SHARED web cache (gốc
            // extracted-package + resolution + tarball) để extraction thực sự
            // chạy lại và failpoint `after-cas-publish` / `after-first-claim`
            // kịp bắn. Store per-project (store.db + CAS ở `.magicore/cache/
            // web`) vẫn đầy để victim đua trên refset promoted.)
            std::fs::remove_dir_all(project.join("node_modules"))
                .unwrap_or_else(|e| panic!("{phase} rep {rep}: failed to clear node_modules: {e}"));
            let shared_web_cache = project.join(".magicore").join("web");
            if shared_web_cache.exists() {
                std::fs::remove_dir_all(&shared_web_cache).unwrap_or_else(|e| {
                    panic!("{phase} rep {rep}: failed to clear shared web cache: {e}")
                });
            }

            // Victim: spawn the re-install, handshake on the READY marker,
            // then SIGKILL. Deterministic — no sleep-ladder.
            // (Victim: spawn install-lại, handshake qua marker READY, rồi
            // SIGKILL. Tất định — không còn sleep-ladder.)
            let marker = markers_dir.join(format!("{phase}-{rep}.ready"));
            let log_dir = project.join("logs");
            std::fs::create_dir_all(&log_dir).unwrap();
            let stdout_f = std::fs::File::create(log_dir.join("victim.out")).unwrap();
            let stderr_f = std::fs::File::create(log_dir.join("victim.err")).unwrap();
            let mut cmd = install_cmd(&mgc, &project, &registry.url);
            cmd.env("MGC_FAILPOINT", phase);
            cmd.env("MGC_FAILPOINT_READY_FILE", &marker);
            cmd.stdout(stdout_f).stderr(stderr_f);
            let mut child = cmd
                .spawn()
                .unwrap_or_else(|e| panic!("{phase} rep {rep}: victim spawn failed: {e}"));

            let ready = wait_for_failpoint_ready(&marker, phase, timeout);
            if !ready {
                // Always reap the child before failing — a parked process
                // must not leak past a failed assertion.
                // (Luôn thu hồi child trước khi fail — process đỗ không được
                // rò rỉ sau assertion thất bại.)
                let _ = child.kill();
                let _ = child.wait();
                let stderr =
                    std::fs::read_to_string(log_dir.join("victim.err")).unwrap_or_default();
                panic!(
                    "{phase} rep {rep}: READY marker not observed within {timeout:?}\nstderr:\n{stderr}"
                );
            }
            child.kill().expect("SIGKILL must be deliverable");
            let status = child.wait().unwrap();
            assert_killed_by_sigkill(&status, phase, rep);

            // Phase 1: no corruption right after the kill.
            // (Pha 1: không hỏng ngay sau kill.)
            doctor_gates_zero(&mgc, &project, &format!("{phase} rep {rep} post-kill"));

            // Phase 2: the follow-up install SUCCEEDS.
            // (Pha 2: install kế tiếp THÀNH CÔNG.)
            let out = install_cmd(&mgc, &project, &registry.url)
                .output()
                .unwrap_or_else(|e| panic!("{phase} rep {rep}: follow-up spawn failed: {e}"));
            assert!(
                out.status.success(),
                "{phase} rep {rep}: follow-up install after SIGKILL must succeed:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );

            // Phase 3: doctor CONSISTENT with the stale verdict (0 stale ⇒
            // exit 0; stale > 0 ⇒ exit nonzero), --repair retires it, and the
            // FINAL doctor is HEALTHY with 0 stale staging.
            // (Pha 3: doctor NHẤT QUÁN với verdict stale (0 stale ⇒ exit 0;
            // stale > 0 ⇒ exit nonzero), --repair nghỉ hưu, doctor CUỐI
            // HEALTHY 0 staging chết.)
            let doctor = doctor_cmd(&mgc, &project).output().unwrap();
            let stdout = String::from_utf8_lossy(&doctor.stdout);
            if stdout.contains("0 stale staging gen(s)") {
                assert!(
                    doctor.status.success(),
                    "{phase} rep {rep}: 0 stale staging must be HEALTHY (exit 0):\n{stdout}\nstderr:\n{}",
                    String::from_utf8_lossy(&doctor.stderr)
                );
            } else {
                assert!(
                    !doctor.status.success(),
                    "{phase} rep {rep}: stale > 0 must exit nonzero (consistency), got success:\n{stdout}"
                );
            }

            // (b) --repair retires the stale staging and must succeed.
            // ((b) --repair nghỉ hưu staging stale và phải thành công.)
            let repair = doctor_repair_cmd(&mgc, &project).output().unwrap();
            assert!(
                repair.status.success(),
                "{phase} rep {rep}: doctor --repair must succeed:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&repair.stdout),
                String::from_utf8_lossy(&repair.stderr)
            );

            // (c) final doctor: exit 0, 0 stale staging, HEALTHY.
            // ((c) doctor cuối: exit 0, 0 staging chết, HEALTHY.)
            let final_doctor = doctor_cmd(&mgc, &project).output().unwrap();
            let final_stdout = String::from_utf8_lossy(&final_doctor.stdout);
            assert!(
                final_doctor.status.success()
                    && final_stdout.contains("0 stale staging gen(s)")
                    && final_stdout.contains("HEALTHY"),
                "{phase} rep {rep}: final doctor must be HEALTHY with 0 stale staging:\n{final_stdout}"
            );
            assert!(
                project.join("node_modules/is-odd/package.json").exists(),
                "{phase} rep {rep}: follow-up materialization must exist"
            );

            // Repair-twice idempotency (Gate 11-B.2): a second --repair after
            // the final doctor must retire ZERO more leases and leave the
            // store HEALTHY — the GC must be a no-op once clean, never
            // re-report a stale it already retired.
            // (Idempotency repair-twice: --repair lần HAI sau doctor cuối phải
            // nghỉ hưu KHÔNG lease nào và store vẫn HEALTHY — GC phải no-op
            // khi đã sạch, không báo lại stale đã nghỉ hưu.)
            let second_repair = doctor_repair_cmd(&mgc, &project).output().unwrap();
            let second_stdout = String::from_utf8_lossy(&second_repair.stdout);
            assert!(
                second_repair.status.success(),
                "{phase} rep {rep}: second doctor --repair must succeed:\n{second_stdout}\nstderr:\n{}",
                String::from_utf8_lossy(&second_repair.stderr)
            );
            assert!(
                second_stdout.contains("retired 0 leaked staging"),
                "{phase} rep {rep}: second --repair must retire 0 leaked staging:\n{second_stdout}"
            );
            assert!(
                !second_stdout.contains("retired 1 leaked staging"),
                "{phase} rep {rep}: second --repair must not re-retire a stale it already retired:\n{second_stdout}"
            );
            let still_healthy = doctor_cmd(&mgc, &project).output().unwrap();
            let still_healthy_stdout = String::from_utf8_lossy(&still_healthy.stdout);
            assert!(
                still_healthy.status.success()
                    && still_healthy_stdout.contains("HEALTHY")
                    && still_healthy_stdout.contains("0 stale staging gen(s)"),
                "{phase} rep {rep}: doctor must stay HEALTHY after repair-twice:\n{still_healthy_stdout}"
            );
        }
    }
}
