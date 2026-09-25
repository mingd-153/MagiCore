//! v4 atomic write + crash recovery tests (design §§4.4–4.6).
//! (Test ghi atomic + phục hồi crash v4.)
//!
//! Crash-table strategy: this test binary re-executes ITSELF as the
//! crashing writer (env `MGC_TEST_V4_MODE` selects the child branch and a
//! `--exact` filter keeps the child to one test). The parent waits for
//! the READY marker, SIGKILLs the child, then asserts the lock on disk is
//! either the old-valid or the new-valid document — never anything else.
//! (Chiến lược bảng crash: binary test tự chạy lại chính nó làm writer
//! đứt (env chọn nhánh con). Parent chờ READY, SIGKILL, rồi assert lock
//! trên đĩa là bản cũ-hợp-lệ hoặc mới-hợp-lệ — không trạng thái nào khác.)

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mgc_lockfile::atomic::{LOCK_FAILPOINTS, atomic_write_locked, cleanup_stale_temps};
use mgc_lockfile::canonical::{LockfileV4, parse_v4_document, payload_digest, write_v4_document};
use mgc_lockfile::project_lock::ProjectWriteLock;

fn test_doc(generator: &str, root_dep: &str) -> LockfileV4 {
    let mut lock = LockfileV4::new(generator);
    lock.metadata.generated_at = "2026-09-17T00:00:00Z".to_string();
    lock.root_dependencies = vec![root_dep.to_string()];
    lock.metadata.lockfile_hash = payload_digest(&lock.payload());
    lock
}

fn write_doc_locked(project: &Path, lock: &LockfileV4) -> PathBuf {
    let guard = ProjectWriteLock::acquire(project, Duration::from_secs(10)).unwrap();
    let dest = project.join("mgc.lock");
    let bytes = write_v4_document(lock).unwrap();
    atomic_write_locked(&guard, &dest, bytes.as_bytes(), Duration::from_secs(60)).unwrap();
    dest
}

fn assert_valid_v4(path: &Path, allowed_generators: &[&str]) {
    let text = std::fs::read_to_string(path).expect("lock file must exist");
    let lock = parse_v4_document(&text).expect("lock file must parse as v4");
    assert_eq!(
        payload_digest(&lock.payload()),
        lock.metadata.lockfile_hash,
        "stored digest must match recomputed payload digest"
    );
    assert!(
        allowed_generators.contains(&lock.metadata.generator.as_str()),
        "unexpected generator {}",
        lock.metadata.generator
    );
}

fn no_temps_left(dir: &Path) {
    let leftovers: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("mgc.lock.tmp."))
        .collect();
    assert!(leftovers.is_empty(), "temp files leaked: {leftovers:?}");
}

#[test]
fn atomic_roundtrip_leaves_no_temps() {
    let dir = tempfile::tempdir().unwrap();
    let dest = write_doc_locked(dir.path(), &test_doc("mgc/1.2.0", "a@1.0.0"));
    assert_valid_v4(&dest, &["mgc/1.2.0"]);
    no_temps_left(dir.path());
}

#[test]
fn stale_temp_with_dead_pid_is_removed_live_pid_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    // A provably-dead pid: spawn + reap `true`, so the pid is ESRCH.
    // (Pid chắc chắn chết: spawn + reap, để kill báo ESRCH.)
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let dead_pid = child.id();
    child.wait().unwrap();
    let stale = dir.path().join(format!("mgc.lock.tmp.{dead_pid}.abc.1"));
    std::fs::write(&stale, b"stale").unwrap();

    let live = dir
        .path()
        .join(format!("mgc.lock.tmp.{}.abc.2", std::process::id()));
    std::fs::write(&live, b"live").unwrap();

    // Zero grace: every mtime qualifies — the decision rests on pid
    // liveness alone.
    let removed = cleanup_stale_temps(dir.path(), Duration::ZERO);
    assert_eq!(removed, 1);
    assert!(!stale.exists());
    assert!(live.exists(), "live-pid temp must never be unlinked");
    std::fs::remove_file(live).unwrap();
}

#[test]
fn lock_contention_fails_closed_then_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let guard = ProjectWriteLock::acquire(dir.path(), Duration::from_secs(10)).unwrap();
    // A live holder is NEVER robbed: short timeout → LockBusy.
    // (Holder sống KHÔNG BAO GIỜ bị cướp: timeout ngắn → LockBusy.)
    let busy = ProjectWriteLock::acquire(dir.path(), Duration::from_millis(50));
    assert!(
        matches!(busy, Err(mgc_lockfile::LockfileError::LockBusy(_))),
        "expected LockBusy, got {busy:?}"
    );
    drop(guard);
    // Release is real: acquire succeeds right after drop.
    ProjectWriteLock::acquire(dir.path(), Duration::from_secs(10)).unwrap();
}

// ---------------------------------------------------------------------------
// Self re-exec crash harness (design §4.5).
// ---------------------------------------------------------------------------

fn child_env(mode: &str, dir: &Path, ready: &Path) -> std::process::Command {
    let exe = std::env::current_exe().unwrap();
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--exact")
        .arg("v4_crash_probe")
        .arg("--nocapture")
        .env("MGC_TEST_V4_MODE", mode)
        .env("MGC_TEST_V4_DIR", dir)
        .env("MGC_TEST_V4_READY", ready);
    // Failpoint phases ride the shared env name so one harness drives
    // both store and lock crash tables.
    if let Some(phase) = mode.strip_prefix("failpoint:") {
        cmd.env("MGC_LOCK_FAILPOINT", phase);
    }
    cmd
}

fn wait_ready(ready: &Path) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        if ready.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("child never signaled READY: {}", ready.display());
}

#[cfg(unix)]
fn kill_child(child: &mut std::process::Child) {
    // SAFETY: pid comes from our own spawned child, waited below; SIGKILL
    // cannot address anything else.
    // (AN TOÀN: pid từ child do chính ta spawn, wait bên dưới.)
    #[allow(unsafe_code)]
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGKILL);
    }
    let _ = child.wait();
}

#[cfg(windows)]
fn kill_child(child: &mut std::process::Child) {
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/PID", &child.id().to_string()])
        .output();
    let _ = child.wait();
}

#[cfg(not(any(unix, windows)))]
fn kill_child(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn touch_ready() {
    let ready = std::env::var("MGC_TEST_V4_READY").unwrap();
    std::fs::write(&ready, "READY\n").unwrap();
}

fn park_forever() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

/// Probe test with a dual life: as a CHILD (env present) it performs one
/// crash-table role and parks; as a PARENT (no env) it drives every role
/// and asserts the on-disk contract after SIGKILL.
#[test]
fn v4_crash_probe() {
    if let Ok(mode) = std::env::var("MGC_TEST_V4_MODE") {
        // ---- child life ----
        let dir = PathBuf::from(std::env::var("MGC_TEST_V4_DIR").unwrap());
        if mode == "lock-holder" {
            let _guard = ProjectWriteLock::acquire(&dir, Duration::from_secs(10)).unwrap();
            touch_ready();
            park_forever();
        }
        if let Some(_phase) = mode.strip_prefix("failpoint:") {
            // Attempt a full v4 write: the failpoint env parks the child
            // at the requested phase (its own READY proves the park).
            let guard = ProjectWriteLock::acquire(&dir, Duration::from_secs(10)).unwrap();
            let dest = dir.join("mgc.lock");
            let mut lock = test_doc("mgc/1.2.0-new", "b@2.0.0");
            lock.metadata.generated_at = "2026-09-17T00:00:01Z".to_string();
            lock.metadata.lockfile_hash = payload_digest(&lock.payload());
            let bytes = write_v4_document(&lock).unwrap();
            mgc_lockfile::atomic::atomic_write_locked(
                &guard,
                &dest,
                bytes.as_bytes(),
                Duration::from_secs(60),
            )
            .unwrap();
            // Reaching here means no failpoint fired (must not happen in
            // this harness — every listed phase sits on this path).
            touch_ready();
            park_forever();
        }
        panic!("unknown child mode: {mode}");
    }

    // ---- parent life: SIGKILL releases the project lock ----
    {
        let dir = tempfile::tempdir().unwrap();
        let ready = dir.path().join("holder.ready");
        let mut child = child_env("lock-holder", dir.path(), &ready)
            .spawn()
            .expect("spawn holder child");
        wait_ready(&ready);
        // Holder is alive and parked on the lock: a second acquire must
        // refuse (no TTL robbery across processes either).
        assert!(matches!(
            ProjectWriteLock::acquire(dir.path(), Duration::from_millis(200)),
            Err(mgc_lockfile::LockfileError::LockBusy(_))
        ));
        kill_child(&mut child);
        // The OS released the lock on SIGKILL: acquire works immediately.
        ProjectWriteLock::acquire(dir.path(), Duration::from_secs(10))
            .expect("lock must release on SIGKILL");
    }

    // ---- parent life: crash table (§4.5), one kill per failpoint ----
    for phase in LOCK_FAILPOINTS {
        let dir = tempfile::tempdir().unwrap();
        // Seed the old-valid document first (parent-held lock, released).
        write_doc_locked(dir.path(), &test_doc("mgc/1.2.0-old", "a@1.0.0"));
        let ready = dir.path().join(format!("ready-{phase}"));
        let mut child = child_env(&format!("failpoint:{phase}"), dir.path(), &ready)
            .env("MGC_LOCK_FAILPOINT_READY_FILE", &ready)
            .spawn()
            .expect("spawn crash child");
        // The failpoint READY lands at `ready` too (same path): whichever
        // arrives first proves the child is parked on the write path.
        wait_ready(&ready);
        kill_child(&mut child);
        // Either outcome is valid (old intact OR new complete) — anything
        // else (missing, half-written, digest mismatch) fails.
        assert_valid_v4(
            &dir.path().join("mgc.lock"),
            &["mgc/1.2.0-old", "mgc/1.2.0-new"],
        );
        // The killed writer may leave a temp behind by design; a later
        // locked cleanup must remove it (pid is dead — child was reaped).
        mgc_lockfile::atomic::cleanup_stale_temps(dir.path(), Duration::ZERO);
        no_temps_left(dir.path());
    }
}
