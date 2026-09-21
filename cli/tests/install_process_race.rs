//! Process-level concurrent install E2E (Gate 11-B, vòng-11 verdict): the
//! audit demanded REAL processes, not thread simulations — "16–64
//! concurrent installers same project; kill injection; zero orphan, zero
//! missing active claim". This test spawns the REAL mgc binary as REAL
//! child processes racing on ONE project and asserts the Gate 11-A
//! contract: the project lock serializes them (second install fails FAST
//! with the lock error, never corrupts), and the store's generation
//! invariants hold afterwards (doctor: HEALTHY, zero orphan/staging/
//! missing/corrupt).
//!
//! Kill injection (crash-before-promote): a SIGKILLed install may leave a
//! GC-able staging footprint (the doctor reports it as stale and --repair
//! retires it), and a FOLLOW-UP install must succeed with every claim
//! intact.
//!
//! (E2E install song song MỨC PROCESS (phán quyết vòng-11): audit đòi
//! process THẬT, không phải mô phỏng thread — nhiều installer cùng
//! project, kill injection, zero orphan/missing claim. Test này spawn
//! binary mgc THẬT làm process con đua trên MỘT project và khẳng định
//! hợp đồng Gate 11-A: lock project xếp tuần tự chúng (install thứ hai
//! fail NGAY với lỗi lock, không bao giờ hỏng store), và invariant
//! generation của store giữ vững sau đó (doctor: HEALTHY, 0 orphan/
//! staging/missing/corrupt).
//!
//! Kill injection (đứt trước promote): install bị SIGKILL được phép để
//! lại vết staging dọn được (doctor báo stale và --repair nghỉ hưu), và
//! install TIẾP THEO phải thành công với mọi claim nguyên vẹn.)

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
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

/// Spawn an install with stdout/stderr REDIRECTED TO FILES — reliable
/// capture under `cargo test`'s mixed console (inherited fds of sibling
/// tests interleave; per-process files never do).
/// (Spawn install với stdout/stderr CHUYỂN HƯỚNG sang FILE — capture tin
/// cậy dưới console trộn lẫn của cargo test (fd kế thừa của test anh em
/// chen dòng; file riêng từng process không bao giờ trộn).)
fn spawn_install_logged(
    mgc: &str,
    project: &Path,
    registry_url: &str,
    log_dir: &Path,
    tag: &str,
) -> std::process::Child {
    use std::io::Write;
    let stdout_path = log_dir.join(format!("{tag}.out"));
    let stderr_path = log_dir.join(format!("{tag}.err"));
    let stdout = std::fs::File::create(&stdout_path).unwrap();
    let stderr = std::fs::File::create(&stderr_path).unwrap();
    let mut cmd = install_cmd(mgc, project, registry_url);
    cmd.stdout(stdout).stderr(stderr);
    let child = cmd.spawn().unwrap();
    // Write the mapping for post-mortem (files created BEFORE spawn could
    // race the process's own writes; this marker is written after).
    // (Ghi ánh xạ cho post-mortem — file tạo TRƯỚC spawn có thể đua với
    // chính process ghi; marker này ghi sau.)
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

fn create_web_project(temp: &TempDir) -> PathBuf {
    let project = temp.path().join("site");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("package.json"),
        r#"{ "name": "race", "version": "1.0.0", "dependencies": { "is-odd": "3.0.1" } }"#,
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

/// Gate 11-B item: CONCURRENT installers of ONE project, as REAL
/// processes. The project lock must refuse the second one FAST (an
/// honest error), and the store must remain doctor-HEALTHY afterwards.
/// (Mục Gate 11-B: installer song song MỘT project, là process THẬT. Lock
/// project phải từ chối cái thứ hai NGAY (lỗi trung thực), store phải
/// còn doctor-HEALTHY sau đó.)
#[test]
fn concurrent_process_installs_serialize_via_project_lock() {
    let temp = TempDir::new().unwrap();
    let mgc = find_mgc_binary();
    let registry = RegistryFixture::new();
    let project = create_web_project(&temp);

    // 4 processes race on ONE project — each with its OWN log files (the
    // cargo-test console interleaves sibling tests' inherited fds; files
    // never interleave).
    // (4 process đua trên MỘT project — mỗi cái có file log RIÊNG (console
    // cargo test trộn fd kế thừa của test anh em; file không bao giờ trộn).)
    let log_dir = temp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();
    let mut children = Vec::new();
    for idx in 0..4 {
        children.push(spawn_install_logged(
            &mgc,
            &project,
            &registry.url,
            &log_dir,
            &format!("racer{idx}"),
        ));
    }

    let mut successes = 0;
    let mut lock_refusals = 0;
    for (idx, child) in children.into_iter().enumerate() {
        let status = child.wait_with_output().unwrap().status;
        let tag = format!("racer{idx}");
        let stderr = read_log(&log_dir, &tag, "err");
        let stdout = read_log(&log_dir, &tag, "out");
        if status.success() {
            successes += 1;
        } else if stderr.contains("another install holds the project lock")
            || stderr.contains("concurrent installs of one project are serialized")
        {
            lock_refusals += 1;
        } else {
            panic!(
                "racer {idx} failed with an UNEXPECTED error (not the lock \
                 refusal) — exit={:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                status.code()
            );
        }
    }
    assert!(
        successes >= 1,
        "at least one racing install must SUCCEED (the lock winner)"
    );
    assert_eq!(
        successes + lock_refusals,
        4,
        "every racer must either win the lock or be refused by it — no other outcome"
    );

    // The store survives the race untouched: doctor must be HEALTHY with
    // zero generation invariant violations.
    // (Store sống sót qua race: doctor phải HEALTHY, không vi phạm
    // invariant generation nào.)
    let doctor = Command::new(&mgc)
        .arg("store")
        .arg("doctor")
        .arg("--core")
        .arg("web")
        .current_dir(&project)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        doctor.status.success(),
        "doctor must be HEALTHY after the concurrent race, stdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    assert!(stdout.contains("0 orphan claim(s)"), "stdout: {stdout}");
    assert!(
        stdout.contains("0 stale staging gen(s)"),
        "no leaked staging generation may remain: {stdout}"
    );
    assert!(stdout.contains("0 missing blob(s)"), "stdout: {stdout}");
    assert!(stdout.contains("0 corrupt blob(s)"), "stdout: {stdout}");

    // The lock winner's install is REAL: node_modules populated.
    // (Install của người thắng lock là THẬT: node_modules có package.)
    assert!(
        project.join("node_modules/is-odd/package.json").exists(),
        "the winning install must materialize node_modules/is-odd"
    );
}

/// Gate 11-B item: KILL INJECTION — SIGKILL an install mid-flight, then
/// prove (a) the store is still doctor-clean (a killed staging is GC-able
/// garbage, not corruption), (b) a follow-up install SUCCEEDS and leaves
/// every claim live.
/// (Mục Gate 11-B: KILL INJECTION — SIGKILL install giữa chừng, rồi chứng
/// minh (a) store vẫn sạch với doctor (staging bị kill là rác dọn được,
/// không phải hỏng), (b) install kế tiếp THÀNH CÔNG và giữ mọi claim
/// sống.)
#[test]
fn killed_install_leaves_doctor_clean_and_next_install_succeeds() {
    let temp = TempDir::new().unwrap();
    let mgc = find_mgc_binary();
    let registry = RegistryFixture::new();
    let project = create_web_project(&temp);

    // Phase 1: a healthy install to establish a baseline refset.
    // (Giai đoạn 1: install khỏe để dựng baseline refset.)
    let out = install_cmd(&mgc, &project, &registry.url)
        .output()
        .expect("baseline install failed");
    assert!(
        out.status.success(),
        "baseline install failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Phase 2: kill injection — start a RE-install and SIGKILL it as soon
    // as it is in flight. If the machine is fast enough that the install
    // ALREADY finished, the kill becomes a no-op on a dead pid and the
    // test treats that run as "kill landed too late" — the follow-up
    // phases below still prove the store contract, and the assertion is
    // skipped only when the process exit code proves completion (success
    // = install finished BEFORE the kill; failure = the kill landed).
    // (Giai đoạn 2: kill injection — chạy install LẠI và SIGKILL ngay khi
    // nó đang bay. Máy đủ nhanh mà install ĐÃ xong thì kill thành no-op
    // trên pid chết và lần chạy đó coi như "kill trượt" — các giai đoạn
    // kế vẫn chứng minh hợp đồng store, và assertion chỉ bỏ qua khi exit
    // code chứng minh hoàn tất (success = install xong TRƯỚC kill;
    // failure = kill trúng).)
    let log_dir = temp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();
    let mut child = spawn_install_logged(&mgc, &project, &registry.url, &log_dir, "victim");
    std::thread::sleep(Duration::from_millis(150));
    child.kill().expect("SIGKILL must be deliverable");
    let killed_status = child.wait().unwrap();
    let victim_err = read_log(&log_dir, "victim", "err");
    let kill_landed = !killed_status.success();
    if !kill_landed {
        // The install completed inside 150ms — the machine outran the
        // kill. Still assert the winner completed CLEANLY (a corrupt
        // partial install must never look like success).
        // (Install xong trong 150ms — máy nhanh hơn kill. Vẫn khẳng định
        // người thắng kết thúc SẠCH (install dở dang hỏng không bao giờ
        // được trông như success).)
        assert!(
            victim_err.is_empty() || !victim_err.contains("Error:"),
            "fast-completing install must be error-free, stderr: {victim_err}"
        );
    }

    // Phase 3: doctor must still be clean-able — the killed install either
    // (a) died before begin (nothing staged), or (b) left a claim-less
    // staging marker the doctor reports, never corruption. Either way the
    // invariant gates (orphan/missing/corrupt) must be ZERO.
    // (Giai đoạn 3: doctor phải dọn được — install bị kill hoặc (a) chết
    // trước begin (không staging), hoặc (b) để lại marker staging
    // không-claim mà doctor báo, không bao giờ là hỏng. Cả hai cách, các
    // cổng invariant (orphan/missing/corrupt) phải KHÔNG.)
    let doctor = Command::new(&mgc)
        .arg("store")
        .arg("doctor")
        .arg("--core")
        .arg("web")
        .current_dir(&project)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        stdout.contains("0 orphan claim(s)"),
        "kill injection must not orphan claims: {stdout}"
    );
    assert!(
        stdout.contains("0 missing blob(s)"),
        "kill injection must not lose blobs: {stdout}"
    );
    assert!(
        stdout.contains("0 corrupt blob(s)"),
        "kill injection must not corrupt blobs: {stdout}"
    );

    // Phase 4: the FOLLOW-UP install must SUCCEED (lock released by the
    // OS, staging garbage superseded) and leave the full claimset live.
    // (Giai đoạn 4: install TIẾP THEO phải THÀNH CÔNG (lock OS đã nhả,
    // rác staging bị thay thế) và giữ trọn claimset sống.)
    let out = install_cmd(&mgc, &project, &registry.url)
        .output()
        .expect("follow-up install failed");
    assert!(
        out.status.success(),
        "follow-up install after kill must succeed (lock auto-release + staging GC), stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Phase 4 (contract v1.1.0-rc.3): the killed install's staging marker
    // is a designed crash footprint — the follow-up doctor must be
    // CONSISTENT (stale > 0 ⇒ nonzero), --repair retires it, the final
    // doctor is HEALTHY with 0 stale.
    // (Giai đoạn 4 (hợp đồng v1.1.0-rc.3): marker staging của install bị
    // kill là vết crash theo thiết kế — doctor sau follow-up phải NHẤT
    // QUÁN (stale > 0 ⇒ nonzero), --repair nghỉ hưu, doctor cuối HEALTHY
    // với 0 stale.)
    let doctor = Command::new(&mgc)
        .arg("store")
        .arg("doctor")
        .arg("--core")
        .arg("web")
        .current_dir(&project)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    if stdout.contains("0 stale staging gen(s)") {
        assert!(
            doctor.status.success(),
            "kill landed too late (no staging) — doctor must be HEALTHY: {stdout}"
        );
    } else {
        assert!(
            !doctor.status.success(),
            "stale > 0 must exit nonzero (consistency), stdout:\n{stdout}"
        );
    }

    let repair = Command::new(&mgc)
        .arg("store")
        .arg("doctor")
        .arg("--repair")
        .arg("--core")
        .arg("web")
        .current_dir(&project)
        .output()
        .unwrap();
    assert!(
        repair.status.success(),
        "doctor --repair after kill must succeed:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&repair.stdout),
        String::from_utf8_lossy(&repair.stderr)
    );

    let final_doctor = Command::new(&mgc)
        .arg("store")
        .arg("doctor")
        .arg("--core")
        .arg("web")
        .current_dir(&project)
        .output()
        .unwrap();
    let final_stdout = String::from_utf8_lossy(&final_doctor.stdout);
    assert!(
        final_doctor.status.success() && final_stdout.contains("0 stale staging gen(s)"),
        "doctor must be HEALTHY with 0 stale after kill + follow-up + repair:\n{final_stdout}"
    );
    assert!(
        project.join("node_modules/is-odd/package.json").exists(),
        "follow-up install must re-materialize node_modules/is-odd"
    );
}

/// P2-1/P2-2 (fresh-context review 2026-09-15): the doctor's lease GC
/// must NOT abort a staging generation whose project install LOCK is
/// held — the lock is the cross-platform liveness proof (the Unix-only
/// pid probe cannot protect a Windows in-flight install). And --repair
/// must retire a claim-ful crashed staging when (and only when) its
/// claims are covered by a promoted generation — the claim-map GC.
/// This test drives the REAL binary: phase 1 plants a hand-crafted
/// crashed staging (dead pid, claims covered by the promoted baseline)
/// and proves --repair retires it; phase 2 plants a staging while
/// HOLDING the project's install lock and proves --repair leaves it
/// alone.
/// (P2-1/P2-2: GC lease của doctor KHÔNG được hủy staging generation mà
/// project install LOCK của nó đang bị giữ — lock là bằng chứng sống
/// cross-platform (pid probe chỉ Unix không bảo vệ được install đang bay
/// trên Windows). Và --repair phải nghỉ hưu staging đứt CÓ claim khi
/// (và chỉ khi) claim được promoted generation giữ — GC claim-map. Test
/// chạy binary THẬT: giai đoạn 1 cắm staging đứt dựng tay (pid chết,
/// claim được baseline promoted giữ) và chứng minh --repair nghỉ hưu
/// nó; giai đoạn 2 cắm staging trong khi GIỮ install lock của project và
/// chứng minh --repair không đụng nó.)
#[test]
fn doctor_repair_retires_covered_crashed_staging_and_respects_install_lock() {
    let temp = TempDir::new().unwrap();
    let mgc = find_mgc_binary();
    let registry = RegistryFixture::new();
    let project = create_web_project(&temp);

    // Phase 1: a healthy install establishes the promoted baseline.
    // (Giai đoạn 1: install khỏe dựng baseline promoted.)
    let out = install_cmd(&mgc, &project, &registry.url)
        .output()
        .expect("baseline install failed");
    assert!(
        out.status.success(),
        "baseline install failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let db_path = project
        .join(".magicore")
        .join("cache")
        .join("web")
        .join("store.db");

    // Plant a CRASHED staging generation: a marker whose lease pid is
    // provably DEAD (pid 4,000,000 never exists) and whose claims are
    // exactly the promoted baseline's hashes — the claim-map GC's
    // retirement sweet spot. Raw SQL mirrors what a kill -9 between
    // claim and promote would leave behind.
    // (Cắm staging generation ĐỨT: marker có lease pid chứng minh CHẾT
    // (pid 4.000.000 không bao giờ tồn tại) và claim đúng các hash của
    // baseline promoted — điểm nghỉ hưu lý tưởng của GC claim-map. SQL
    // thô mô phỏng những gì kill -9 giữa claim và promote để lại.)
    let project_key = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let (project_root, promoted_gen): (String, i64) = conn
            .query_row(
                "SELECT project_root, generation FROM cas_generations WHERE state = 'promoted'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let crashed_gen = promoted_gen + 1000;
        conn.execute(
            "INSERT INTO cas_generations (project_root, generation, state, lease_pid, lease_started_at)
             VALUES (?1, ?2, 'staging', 4000000, 1)",
            rusqlite::params![project_root, crashed_gen],
        )
        .unwrap();
        // Its claims = the promoted baseline's hashes (all covered).
        // (Claim của nó = hash của baseline promoted (đều được giữ).)
        conn.execute(
            "INSERT INTO cas_blob_refs (project_root, generation, hash)
             SELECT project_root, ?2, hash FROM cas_blob_refs
             WHERE project_root = ?1 AND generation = ?3",
            rusqlite::params![project_root, crashed_gen, promoted_gen],
        )
        .unwrap();
        project_root
    };

    // --repair must retire the covered crashed staging: after it, the
    // doctor reports 0 stale staging and stays HEALTHY with every live
    // ref intact (the claim-map contract: retirement loses nothing).
    // (--repair phải nghỉ hưu staging đứt được phủ: sau đó doctor báo 0
    // staging chết và vẫn HEALTHY với mọi ref sống nguyên (hợp đồng
    // claim-map: nghỉ hưu không mất gì).)
    // --repair must retire the covered crashed staging: the pre-repair
    // invariants line now COUNTS it as stale (P0-1 — claim-ful covered
    // staging is garbage, not silent over-retention), and the GC retires
    // exactly one; the final verdict is HEALTHY.
    // (--repair phải nghỉ hưu staging đứt được phủ: dòng invariants
    // trước-repair giờ ĐẾM nó là stale (P0-1 — staging có claim được phủ
    // là rác, không phải giữ thừa im lặng), và GC nghỉ hưu đúng một;
    // verdict cuối HEALTHY.)
    let doctor = Command::new(&mgc)
        .arg("store")
        .arg("doctor")
        .arg("--repair")
        .arg("--core")
        .arg("web")
        .env("MGC_DOCTOR_STAGING_GRACE_SECS", "0")
        .current_dir(&project)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        doctor.status.success(),
        "doctor --repair must stay HEALTHY after retiring the covered crashed staging:\n{stdout}\n{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    assert!(
        stdout.contains("1 stale staging gen(s)")
            && stdout.contains("retired 1 leaked staging generation(s)"),
        "claim-map GC must detect + retire the covered crashed staging: {stdout}"
    );
    assert!(
        stdout.contains("0 missing blob(s)") && stdout.contains("0 corrupt blob(s)"),
        "retirement must not lose or corrupt any blob: {stdout}"
    );

    // Phase 2 (P2-1 lock gate): plant a claim-less FRESH staging (dead
    // pid) — but HOLD the project's install lock while running
    // --repair. The lock is held by THIS test process, exactly like a
    // concurrent in-flight install would hold it: the doctor must skip
    // it (lock-held), never abort it mid-install.
    // (Giai đoạn 2 (cổng lock P2-1): cắm staging không-claim MỚI (pid
    // chết) — nhưng GIỮ install lock của project trong khi chạy
    // --repair. Lock bị test process này giữ, đúng như install đang bay
    // giữ nó: doctor phải bỏ qua (lock-held), không bao giờ hủy giữa
    // chừng.)
    let fresh_gen = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO cas_generations (project_root, generation, state, lease_pid, lease_started_at)
             VALUES (?1, (SELECT COALESCE(MAX(generation), 0) + 1 FROM cas_generations WHERE project_root = ?1),
                     'staging', 4000001, 1)",
            rusqlite::params![project_key],
        )
        .unwrap();
        let g: i64 = conn
            .query_row(
                "SELECT MAX(generation) FROM cas_generations WHERE project_root = ?1",
                rusqlite::params![project_key],
                |row| row.get(0),
            )
            .unwrap();
        g
    };
    {
        // Hold the install lock for the project (the same lock
        // `mgc --core web install` takes) across the repair run.
        // (Giữ install lock của project (cùng lock install lấy) xuyên
        // suốt lượt repair.)
        let locks_root = project
            .join(".magicore")
            .join("cache")
            .join("web")
            .join("locks");
        let _lock_guard = mgc_store::ProjectInstallLock::acquire_at(&locks_root, &project_key)
            .expect("test must hold the install lock like a live install");

        let doctor = Command::new(&mgc)
            .arg("store")
            .arg("doctor")
            .arg("--repair")
            .arg("--core")
            .arg("web")
            .env("MGC_DOCTOR_STAGING_GRACE_SECS", "0")
            .current_dir(&project)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&doctor.stdout);
        // The lock-held staging survives the repair — and the doctor
        // stays healthy overall (a skipped generation is an honest
        // skip, not corruption).
        // (Staging bị giữ lock sống sót qua repair — và doctor vẫn khỏe
        // (bỏ qua trung thực, không phải hỏng).)
        let still_staging: i64 = {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM cas_generations
                 WHERE project_root = ?1 AND generation = ?2 AND state = 'staging'",
                rusqlite::params![project_key, fresh_gen],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            still_staging, 1,
            "P2-1: the doctor must NOT abort a staging generation whose install lock is held \
             (in-flight install protection): {stdout}"
        );
    }
    // After the lock is RELEASED, the same repair CAN retire it — prove
    // by a follow-up repair run + a plain doctor read (the plain doctor's
    // invariants line is the post-repair truth).
    // (Sau khi lock NHẢ, cùng lượt repair có thể nghỉ hưu nó — chứng minh
    // bằng lượt repair kế + doctor thường (dòng invariants của doctor
    // thường là sự thật sau-repair).)
    let doctor = Command::new(&mgc)
        .arg("store")
        .arg("doctor")
        .arg("--repair")
        .arg("--core")
        .arg("web")
        .env("MGC_DOCTOR_STAGING_GRACE_SECS", "0")
        .current_dir(&project)
        .output()
        .unwrap();
    assert!(
        doctor.status.success(),
        "repair after lock release must succeed: {}",
        String::from_utf8_lossy(&doctor.stdout)
    );
    let doctor = Command::new(&mgc)
        .arg("store")
        .arg("doctor")
        .arg("--core")
        .arg("web")
        .current_dir(&project)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        doctor.status.success() && stdout.contains("0 stale staging gen(s)"),
        "after lock release + repair, a plain doctor read must show 0 stale staging: {stdout}"
    );
}
