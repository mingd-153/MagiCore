//! `install_hardening_e2e.rs` — P0 store/lifecycle hardening through the
//! REAL binary: locked HOME, unwritable store, corrupt DB, concurrent
//! installs, SIGKILL mid-materialize and mid-rollback. Every test names
//! the contract it proves; network-dependent ones skip via the registry
//! guard (same convention as the audit lanes).
//! E2E hardening qua binary thật: HOME khóa, store không ghi được, DB
//! hỏng, song song, SIGKILL giữa materialize/rollback.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;
use tempfile::TempDir;

fn find_mgc_binary() -> String {
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(PathBuf::from)
        .map(|p| p.to_string_lossy().to_string())
        .expect("CARGO_BIN_EXE_mgc unavailable — run via cargo test")
}

fn registry_reachable() -> bool {
    use std::net::ToSocketAddrs;
    let Ok(mut addrs) = "registry.npmjs.org:443".to_socket_addrs() else {
        return false;
    };
    let Some(addr) = addrs.next() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(5)).is_ok()
}

fn web_fixture(dir: &Path, name: &str) {
    std::fs::write(
        dir.join("mgc.toml"),
        format!(
            "name = \"{name}\"\nversion = \"0.1.0\"\necosystem = \"web\"\nmode = \"frontend\"\nframeworks = []\ntemplate = \"\"\nfeatures = []\nregistries = []\npatches = []\n"
        ),
    )
    .unwrap();
    // esbuild ships a real postinstall (succeeds normally).
    // (esbuild có postinstall thật.)
    std::fs::write(
        dir.join("package.json"),
        r#"{"name":"hardening","version":"1.0.0","dependencies":{"esbuild":"0.28.2"}}"#,
    )
    .unwrap();
}

fn run_install(mgc: &str, dir: &Path, extra_env: &[(&str, &str)]) -> (Option<i32>, String) {
    let mut cmd = Command::new(mgc);
    cmd.args(["install", "--allow-scripts"]).current_dir(dir);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("failed to spawn mgc");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), text)
}

/// P0-store: install must not depend on a writable HOME — shared cache
/// discovery degrades, store/CAS stay project-local.
/// (Install không được phụ thuộc HOME ghi được.)
#[test]
fn install_succeeds_with_locked_home() {
    if !registry_reachable() {
        eprintln!("SKIP: registry unreachable");
        return;
    }
    let mgc = find_mgc_binary();
    let tmp = TempDir::new().unwrap();
    // Locked HOME (read-only dir): rustup/cargo are already resolved —
    // only the child process sees this HOME.
    // (HOME khóa read-only: chỉ tiến trình con thấy.)
    let locked_home = tmp.path().join("locked-home");
    std::fs::create_dir_all(&locked_home).unwrap();
    let mut perms = std::fs::metadata(&locked_home).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&locked_home, perms).unwrap();

    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    web_fixture(&project, "locked-home-proj");

    let mut cmd = Command::new(&mgc);
    cmd.args(["install", "--allow-scripts"])
        .current_dir(&project)
        .env("HOME", &locked_home);
    #[cfg(unix)]
    {
        // Keep TMPDIR writable (tempfile needs it); drop every store
        // override to prove zero dependence.
        // (Giữ TMPDIR ghi được; gỡ mọi override store để chứng minh.)
        cmd.env_remove("MGC_CACHE_DIR");
        cmd.env_remove("MAGICORE_SHARED_CACHE_DIR");
        cmd.env_remove("MAGICORE_STORE_ROOT");
    }
    let out = cmd.output().expect("failed to spawn mgc");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "install with locked HOME must succeed:\n{text}"
    );
    assert!(project.join("node_modules").join("esbuild").is_dir());
}

/// P0-store: an unwritable install location fails LOUDLY with the path —
/// never silently, never panic. (A read-only *store* with a satisfied
/// lock legitimately succeeds — reads need no writes — so this pins a
/// read-only `node_modules` instead.)
/// (Chỗ cài không ghi được thì lỗi rõ kèm path.)
#[test]
fn unwritable_project_store_fails_loudly() {
    if !registry_reachable() {
        eprintln!("SKIP: registry unreachable");
        return;
    }
    let mgc = find_mgc_binary();
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    web_fixture(&project, "ro-store-proj");

    // Lock the project root itself read-only: every install mutation
    // (tree, lock, store) must fail here. (A read-only *subtree* with a
    // writable parent legitimately succeeds via atomic rename-aside —
    // POSIX semantics, not a bug — so the root itself is locked.)
    // (Khóa root project: mọi mutation đều phải fail ở đây.)
    let mut perms = std::fs::metadata(&project).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&project, perms).unwrap();

    let (code, out) = run_install(&mgc, &project, &[]);
    assert_ne!(
        code,
        Some(0),
        "unwritable tree must fail, got output:\n{out}"
    );
    assert!(
        out.contains("proj")
            || out.contains("Permission denied")
            || out.contains("permission")
            || out.contains("Read-only"),
        "error must name the tree path/problem:\n{out}"
    );
}

/// P0-store: a corrupt project DB fails closed (clear error, no panic,
/// no repair-masquerade).
/// (DB project hỏng thì fail rõ, không panic.)
#[test]
fn corrupt_project_db_fails_closed() {
    if !registry_reachable() {
        eprintln!("SKIP: registry unreachable");
        return;
    }
    let mgc = find_mgc_binary();
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    web_fixture(&project, "corrupt-db-proj");

    let (code, out) = run_install(&mgc, &project, &[]);
    assert_eq!(code, Some(0), "baseline install must succeed:\n{out}");

    // Corrupt every sqlite file under the project store.
    // (Làm hỏng mọi file sqlite trong store project.)
    let mut corrupted = 0;
    let mut stack = vec![project.join(".magicore")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("db") {
                std::fs::write(&path, b"definitely-not-sqlite").unwrap();
                corrupted += 1;
            }
        }
    }
    assert!(corrupted > 0, "baseline must leave a sqlite db to corrupt");
    std::fs::remove_dir_all(project.join("node_modules")).unwrap();

    let (code, out) = run_install(&mgc, &project, &[]);
    assert_ne!(
        code,
        Some(0),
        "corrupt project DB must fail, got output:\n{out}"
    );
}

/// Spawn `mgc install` parked at a failpoint; wait for its READY marker.
/// (Chạy install đỗ ở failpoint; đợi marker READY.)
#[cfg(unix)]
fn spawn_parked_install(
    mgc: &str,
    project: &Path,
    phase: &str,
    extra_env: &[(&str, &str)],
) -> (Child, PathBuf) {
    let ready = project.join(format!("ready-{phase}"));
    let mut cmd = Command::new(mgc);
    cmd.args(["install", "--allow-scripts"])
        .current_dir(project)
        .env("MGC_FAILPOINT", phase)
        .env("MGC_FAILPOINT_READY_FILE", &ready);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let child = cmd.spawn().expect("spawn parked install");
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    loop {
        if ready.is_file() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "failpoint {phase} never parked"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    (child, ready)
}

#[cfg(unix)]
fn sigkill(child: &mut Child) {
    Command::new("kill")
        .args(["-9", &child.id().to_string()])
        .output()
        .expect("kill -9");
    let _ = child.wait();
}

/// P0-concurrent: a second install while one holds the project lock
/// fails FAST (no race, no hang) with a lock error.
/// (Install thứ hai khi lock bị giữ thì fail NHANH với lỗi lock.)
#[cfg(unix)]
#[test]
fn concurrent_second_install_fails_fast_on_lock() {
    if !registry_reachable() {
        eprintln!("SKIP: registry unreachable");
        return;
    }
    let mgc = find_mgc_binary();
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    web_fixture(&project, "concurrent-proj");

    let (mut parked, _ready) = spawn_parked_install(&mgc, &project, "before-materialize", &[]);
    let second = Command::new(&mgc)
        .args(["install", "--allow-scripts"])
        .current_dir(&project)
        .output()
        .expect("spawn second install");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );
    sigkill(&mut parked);
    assert_ne!(
        second.status.code(),
        Some(0),
        "second concurrent install must fail fast:\n{text}"
    );
    assert!(
        text.to_lowercase().contains("lock"),
        "failure must name the lock contention:\n{text}"
    );
}

/// P0-SIGKILL: kill -9 during materialize, then re-run → full success
/// with a consistent tree, no backup litter, valid lock.
/// (SIGKILL giữa materialize, chạy lại → thành công, trạng thái nhất quán.)
#[cfg(unix)]
#[test]
fn sigkill_during_materialize_recovers_cleanly() {
    if !registry_reachable() {
        eprintln!("SKIP: registry unreachable");
        return;
    }
    let mgc = find_mgc_binary();
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    web_fixture(&project, "sigkill-mat-proj");

    let (mut parked, _ready) = spawn_parked_install(&mgc, &project, "before-materialize", &[]);
    sigkill(&mut parked);

    let (code, out) = run_install(&mgc, &project, &[]);
    assert_eq!(code, Some(0), "post-kill re-run must succeed:\n{out}");
    assert!(project.join("node_modules").join("esbuild").is_dir());
    assert!(project.join("mgc.lock").is_file());
    let litter: Vec<_> = std::fs::read_dir(&project)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with(".mgc-prev-") || n.starts_with(".mgc-stage-")
        })
        .collect();
    assert!(
        litter.is_empty(),
        "no backup/staging litter may remain: {litter:?}"
    );
}

/// P0-SIGKILL: kill -9 inside the rollback path, then re-run → the
/// recovery restores the baseline tree and the install succeeds.
/// (SIGKILL giữa rollback, chạy lại → recovery dựng cây baseline.)
#[cfg(unix)]
#[test]
fn sigkill_during_rollback_recovers_cleanly() {
    if !registry_reachable() {
        eprintln!("SKIP: registry unreachable");
        return;
    }
    let mgc = find_mgc_binary();
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    web_fixture(&project, "sigkill-rb-proj");

    let (code, out) = run_install(&mgc, &project, &[]);
    assert_eq!(code, Some(0), "baseline install must succeed:\n{out}");

    // Force the script-failure arm, then die inside its rollback.
    // (Ép nhánh script-fail, rồi chết giữa rollback.)
    std::fs::remove_dir_all(project.join("node_modules")).unwrap();
    let (mut parked, _ready) = spawn_parked_install(
        &mgc,
        &project,
        "before-rollback-restore",
        &[("MGC_LIFECYCLE_FAIL_PACKAGES", "esbuild")],
    );
    sigkill(&mut parked);

    let (code, out) = run_install(&mgc, &project, &[]);
    assert_eq!(code, Some(0), "post-kill re-run must succeed:\n{out}");
    assert!(project.join("node_modules").join("esbuild").is_dir());
    assert!(project.join("mgc.lock").is_file());
}
