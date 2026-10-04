#![allow(clippy::unwrap_used)]
//! Run tests — dry-run không spawn, audit log ghi args đã REDACTED, exit ≠ 0 bail
//! (00-index §5.5 dry-run, §5.4 audit, §5.8 fail → bail)

use mgc_exec::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(unix)]
use std::time::Duration;

// Monotonic per-process counter: two threads calling tmp_dir() in the SAME
// clock tick used to share one dir (pid+nanos collide) while both run
// `remove_dir_all` + fixed subpaths — a rare ENOENT flake under parallel
// load. The counter alone makes collision structurally impossible within
// the process (pid separates processes); no ThreadId — its Debug form
// contains parens that break shell scripts embedding the path.
// (Bộ đếm đơn điệu: đủ duy nhất trong process; không dùng ThreadId vì
// ngoặc đơn phá shell script.)
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn tmp_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let uniq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!(
        "mgc-exec-test-{}-{nanos}-{uniq}",
        std::process::id()
    ));
    let _ = fs::create_dir_all(&d);
    d
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(unix)]
fn fake_npm(dir: &Path) -> PathBuf {
    let path = dir.join("npm");
    fs::write(
        &path,
        "#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$MGC_TEST_PID_FILE\"\n/bin/sleep 0.5\nprintf started > \"$MGC_TEST_MARKER\"\n/bin/sleep 30 &\nwait\n",
    )
    .unwrap();
    make_executable(&path);
    path
}

#[cfg(unix)]
fn test_process_is_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 is a read-only process existence check.
    // (An toàn: signal 0 chỉ kiểm tra process còn tồn tại.)
    #[allow(unsafe_code)]
    unsafe {
        libc::kill(pid, 0) == 0
    }
}

#[cfg(unix)]
fn force_kill_test_process_group(pid_file: &Path) {
    let Some(pid) = fs::read_to_string(pid_file)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
    else {
        return;
    };
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return;
    };
    // SAFETY: the PID and its current group were recorded by this test's own
    // short-lived fixture under a unique temporary directory.
    // (An toàn: PID/group do fixture tạm của test này ghi lại.)
    #[allow(unsafe_code)]
    unsafe {
        let process_group = libc::getpgid(pid);
        if process_group > 0 {
            libc::kill(-process_group, libc::SIGKILL);
        }
        libc::kill(pid, libc::SIGKILL);
    }
}

#[test]
fn dry_run_does_not_spawn_and_prints() {
    // cargo --version sẽ chạy thật nếu không dry-run — dry_run=true không spawn
    let opts = ExecOptions {
        dry_run: true,
        log_path: None,
        cwd: None,
        ..Default::default()
    };
    let report = run("cargo", &["--version".to_string()], &opts).unwrap();
    assert!(report.dry_run);
    assert_eq!(report.exit_code, 0);
}

#[test]
fn inherited_dry_run_uses_same_report_contract() {
    let opts = ExecOptions {
        dry_run: true,
        log_path: None,
        cwd: None,
        ..Default::default()
    };
    let report = run_inherited("git", &["--version".to_string()], &opts).unwrap();
    assert!(report.dry_run);
    assert_eq!(report.cmd, "git");
    assert_eq!(report.exit_code, 0);
}

#[test]
fn inherited_run_rejects_forbidden_pm_before_spawn() {
    let opts = ExecOptions {
        clean_env: true,
        ..Default::default()
    };
    let err = run_inherited("pnpm", &["install".to_string()], &opts).unwrap_err();
    assert!(err.to_string().contains("forbidden"), "{err}");
}

#[test]
fn reports_process_tree_guard_capability_truthfully() {
    // Tree kill is available on BOTH supported platform families now:
    // unix process groups + Windows taskkill /T (P0-3, 2026-09-15).
    // (Kill cây có trên CẢ hai họ nền tảng: unix process group +
    // Windows taskkill /T.)
    assert!(process_tree_guard_available());
}

#[test]
fn forbidden_tool_rejected_before_spawn() {
    let opts = ExecOptions {
        dry_run: false,
        log_path: None,
        cwd: None,
        ..Default::default()
    };
    let err = run("npm", &["install".to_string()], &opts).unwrap_err();
    assert!(err.to_string().contains("forbidden"));
}

#[test]
fn legacy_compat_option_never_authorizes_package_manager_or_rival_runtime() {
    let opts = ExecOptions {
        execution_scope: Some(ExecutionScope::DevServer),
        compat_runtime: Some("bun".to_string()),
        log_path: None,
        ..Default::default()
    };
    for tool in ["bun", "deno", "composer", "pub"] {
        let err = run(tool, &["--version".to_string()], &opts)
            .expect_err("legacy compat option must not authorize an external process");
        assert!(
            err.to_string().contains("forbidden"),
            "{tool} must be rejected by executor policy before spawn: {err}"
        );
    }
}

#[test]
fn package_resolution_subcommands_are_rejected_before_spawn() {
    let opts = ExecOptions {
        dry_run: true,
        ..Default::default()
    };
    for (tool, args) in [
        ("cargo", vec!["fetch".to_string()]),
        ("cargo", vec!["+stable".to_string(), "fetch".to_string()]),
        ("cargo", vec!["build".to_string()]),
        (
            "cargo",
            vec![
                "test".to_string(),
                "--".to_string(),
                "--offline".to_string(),
            ],
        ),
        (
            "python3",
            vec!["-m".to_string(), "pip".to_string(), "install".to_string()],
        ),
        (
            "go",
            vec!["get".to_string(), "example.test/pkg".to_string()],
        ),
        (
            "go",
            vec![
                "mod".to_string(),
                "edit".to_string(),
                "-require=example.test/pkg@v1.2.3".to_string(),
            ],
        ),
        (
            "go",
            vec![
                "mod".to_string(),
                "init".to_string(),
                "example.test/project".to_string(),
            ],
        ),
        ("go", vec!["work".to_string(), "sync".to_string()]),
        ("flutter", vec!["pub".to_string(), "get".to_string()]),
        ("dotnet", vec!["restore".to_string()]),
        ("swift", vec!["package".to_string(), "resolve".to_string()]),
    ] {
        let error = run_inherited(tool, &args, &opts).unwrap_err();
        assert!(
            error.to_string().contains("blocked")
                || error.to_string().contains("forbidden")
                || error.to_string().contains("not on the allowlist"),
            "{tool} {args:?} was not rejected clearly: {error}"
        );
    }
}

#[test]
fn unknown_tool_rejected() {
    let opts = ExecOptions::default();
    assert!(run("definitely-not-a-real-tool-xyz", &[], &opts).is_err());
}

#[test]
fn audit_log_written_with_redacted_args() {
    let dir = tmp_dir();
    let log = dir.join("exec.log");
    let _ = fs::remove_file(&log);
    let opts = ExecOptions {
        dry_run: true,
        log_path: Some(log.clone()),
        cwd: None,
        ..Default::default()
    };
    run(
        "cargo",
        &["--token=leakme".to_string(), "--version".to_string()],
        &opts,
    )
    .unwrap();
    let content = fs::read_to_string(&log).unwrap();
    assert!(content.contains("cargo"));
    assert!(
        !content.contains("leakme"),
        "secret must never reach the audit log: {content}"
    );
    assert!(content.contains("[REDACTED]"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_tool_fails_with_clear_error() {
    // tool thuộc allowlist nhưng không tồn tại trên máy — spawn fail rõ ràng
    let opts = ExecOptions::default();
    let err = run("pio", &[], &opts).unwrap_err();
    assert!(err.to_string().contains("spawn") || err.to_string().contains("No such"));
}

#[test]
#[cfg(unix)]
fn command_timeout_kills_hung_tool() {
    // P0-3 (Tech Lead 2026-09-15) — the scenario the old test never covered:
    // the tool spawns a GRANDCHILD that inherits stdout/stderr and keeps them
    // open while it sleeps. That is the case that (a) hung the post-kill
    // drain forever (the reader threads waited for an EOF the grandchild
    // never sent) and (b) on macOS survived the timeout entirely, because the
    // /proc-walk tree kill matched nothing without /proc and only the direct
    // child was signalled.
    //
    // Asserts the three things the Tech Lead asked for:
    //   1. run() returns a "timed out" error (not Ok, not a hang)
    //   2. wall-clock stays under 1s for a 250ms timeout
    //   3. the grandchild is GONE after run() returns
    //
    // (Đúng kịch bản Tech Lead nêu: tool sinh GRANDCHILD thừa hưởng pipe và
    // giữ nó trong lúc sleep. Đây là ca (a) treo drain sau kill vô hạn và
    // (b) trên macOS sống sót qua timeout vì walk /proc không khớp gì.
    // Kiểm 3 điều: (1) run() trả lỗi "timed out"; (2) wall-clock < 1s với
    // timeout 250ms; (3) grandchild đã chết sau khi run() trả về.)
    use std::os::unix::fs::PermissionsExt;
    use std::time::Instant;

    let dir = tmp_dir().join("timeout-tree");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let pid_file = dir.join("grandchild.pid");

    // Fake tool: start a 30s sleeper (the grandchild), publish its PID, then
    // keep the shell alive on `wait`. The sleeper inherits the piped stdout
    // and holds it open for the whole 30s.
    // (Tool giả: chạy sleeper 30s (grandchild), ghi PID ra file, rồi giữ
    // shell sống bằng `wait`. Sleeper thừa hưởng stdout dạng pipe và giữ nó
    // suốt 30s.)
    let fake_tool = dir.join("cargo"); // allowlisted name — normalize_script_token takes the basename ("cargo")
    fs::write(
        &fake_tool,
        format!(
            "#!/bin/sh\n/bin/sleep 30 & echo $! > {}\nwait\n",
            pid_file.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_tool).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_tool, permissions).unwrap();

    let opts = ExecOptions {
        clean_env: true,
        cwd: Some(dir.clone()),
        timeout: Some(Duration::from_millis(250)),
        ..Default::default()
    };

    let started = Instant::now();
    let err = run(fake_tool.to_str().unwrap(), &[], &opts).unwrap_err();
    let elapsed = started.elapsed();

    assert!(
        err.to_string().contains("timed out"),
        "expected a timeout error, got: {err}"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "run() took {elapsed:?} for a 250ms timeout — the process tree was \
         not reaped promptly (grandchild still holding the pipe?)"
    );

    // Grandchild must be gone: signal 0 only succeeds while the PID lives.
    // Zombies are reaped by init once the whole group is killed, so a short
    // bounded poll (not an open-ended sleep) settles the answer.
    // (Grandchild phải chết: signal 0 chỉ thành công khi PID còn sống.
    // Zombie được init reap sau khi cả group bị kill nên poll có hạn là đủ.)
    let grandchild: u32 = fs::read_to_string(&pid_file)
        .unwrap_or_else(|e| panic!("grandchild PID file missing ({e}) — tool never ran"))
        .trim()
        .parse()
        .expect("grandchild PID file must hold a PID");
    assert!(grandchild > 0);
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut still_alive = true;
    while Instant::now() < deadline {
        // Safety: kill(pid, 0) performs no signal delivery — it is the
        // standard liveness probe and never affects the target.
        // (An toàn: kill(pid, 0) không gửi signal — đây là phép dò sống-chết
        // chuẩn, không tác động process đích.)
        #[allow(unsafe_code)]
        let alive = unsafe { libc::kill(grandchild as i32, 0) } == 0;
        if !alive {
            still_alive = false;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !still_alive,
        "grandchild {grandchild} is STILL alive after the timeout kill — \
         terminate_process_tree did not reach the whole tree"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[cfg(unix)]
fn clean_env_blocks_forbidden_pm_spawned_by_child_path_lookup() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("child-pm-bin");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let fake_cargo = dir.join("cargo");
    fs::write(&fake_cargo, "#!/bin/sh\nnpm --version\n").unwrap();
    let mut permissions = fs::metadata(&fake_cargo).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_cargo, permissions).unwrap();

    let opts = ExecOptions {
        clean_env: true,
        env: vec![("PATH".to_string(), dir.display().to_string())],
        timeout: Some(Duration::from_secs(2)),
        ..Default::default()
    };

    let err = run(
        fake_cargo.to_str().unwrap(),
        &["--version".to_string()],
        &opts,
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("MagiCore blocked forbidden package manager: npm")
            || err
                .to_string()
                .contains("references forbidden package manager 'npm'"),
        "unexpected error: {err}"
    );
}

#[test]
#[cfg(unix)]
fn inherited_env_still_blocks_forbidden_pm_spawned_by_project_tool() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("inherited-child-pm-bin");
    fs::create_dir_all(&dir).unwrap();
    let fake_cargo = dir.join("cargo");
    let fake_npm = dir.join("npm");
    let marker = dir.join("npm-was-run");
    fs::write(
        &fake_npm,
        format!("#!/bin/sh\nprintf ran > '{}'\n", marker.display()),
    )
    .unwrap();
    fs::write(&fake_cargo, "#!/bin/sh\nnpm --version\n").unwrap();
    for path in [&fake_cargo, &fake_npm] {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    let opts = ExecOptions {
        cwd: Some(dir.clone()),
        clean_env: false,
        env: vec![("PATH".to_string(), dir.display().to_string())],
        timeout: Some(Duration::from_secs(2)),
        ..Default::default()
    };

    let result = run_project_binary_inherited(&fake_cargo, &[], &opts);
    assert!(
        result.is_err(),
        "MagiCore must reject a forbidden child process"
    );
    assert!(
        !marker.exists(),
        "the package-manager child must not execute before detection"
    );
}

#[test]
#[cfg(unix)]
fn inherited_env_monitor_kills_a_session_escaped_forbidden_child()
-> Result<(), Box<dyn std::error::Error>> {
    use std::time::Instant;

    if std::process::Command::new("python3")
        .arg("-c")
        .arg("pass")
        .status()
        .is_err()
    {
        return Ok(());
    }

    let dir = tempfile::tempdir()?;
    let fake_pm = fake_npm(dir.path());
    let fake_cargo = dir.path().join("cargo");
    let pid_file = dir.path().join("escaped-pid");
    let marker = dir.path().join("escaped-ran");
    fs::write(
        &fake_cargo,
        "#!/bin/sh\npython3 -c 'import os,sys; open(sys.argv[1], \"w\").write(str(os.getpid())); os.setsid(); target=os.environ[\"MGC_TEST_PM\"]; os.execv(target, [target, \"install\"])' \"$MGC_TEST_PID_FILE\" &\n/bin/sleep 5\nwait\n",
    )?;
    make_executable(&fake_cargo);

    let started = std::time::Instant::now();
    let result = run_project_binary_inherited(
        &fake_cargo,
        &[],
        &ExecOptions {
            cwd: Some(dir.path().to_path_buf()),
            clean_env: false,
            env: vec![
                ("MGC_TEST_PM".to_string(), fake_pm.display().to_string()),
                (
                    "MGC_TEST_PID_FILE".to_string(),
                    pid_file.display().to_string(),
                ),
                ("MGC_TEST_MARKER".to_string(), marker.display().to_string()),
            ],
            timeout: Some(Duration::from_secs(3)),
            ..Default::default()
        },
    );
    let elapsed = started.elapsed();
    let error = result.expect_err("the forbidden descendant must be rejected");
    let pid = fs::read_to_string(&pid_file)?.trim().parse::<u32>()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while test_process_is_alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let stopped = !test_process_is_alive(pid);
    if !stopped {
        force_kill_test_process_group(&pid_file);
    }

    assert!(
        error
            .to_string()
            .contains("forbidden package manager 'npm' spawned"),
        "unexpected process-guard error: {error}"
    );
    assert!(
        stopped,
        "escaped child PID {pid} survived the process guard"
    );
    assert!(
        !marker.exists(),
        "the escaped package-manager child ran after the guard returned"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "the runner did not stop the escaped process promptly: {elapsed:?}"
    );
    Ok(())
}

#[test]
#[cfg(unix)]
fn captured_runner_scans_and_stops_children_after_root_exits()
-> Result<(), Box<dyn std::error::Error>> {
    use std::time::Instant;

    if std::process::Command::new("python3")
        .arg("-c")
        .arg("pass")
        .status()
        .is_err()
    {
        return Ok(());
    }

    let dir = tempfile::tempdir()?;
    let fake_pm = dir.path().join("npm");
    let fake_cargo = dir.path().join("cargo");
    let pid_file = dir.path().join("delayed-pid");
    let marker = dir.path().join("delayed-ran");
    fs::write(
        &fake_pm,
        "#!/bin/sh\nprintf started > \"$MGC_TEST_MARKER\"\n/bin/sleep 1.2\n",
    )?;
    make_executable(&fake_pm);
    fs::write(
        &fake_cargo,
        "#!/bin/sh\npython3 -c 'import os,sys,time; open(sys.argv[1], \"w\").write(str(os.getpid())); os.setsid(); time.sleep(0.5); target=os.environ[\"MGC_TEST_PM\"]; os.execv(target, [target, \"install\"])' \"$MGC_TEST_PID_FILE\" &\ni=0\nwhile [ ! -f \"$MGC_TEST_PID_FILE\" ] && [ \"$i\" -lt 100 ]; do /bin/sleep 0.01; i=$((i + 1)); done\n/bin/sleep 0.1\nexit 0\n",
    )?;
    make_executable(&fake_cargo);

    let started = Instant::now();
    let result = run_project_binary(
        &fake_cargo,
        &[],
        &ExecOptions {
            cwd: Some(dir.path().to_path_buf()),
            clean_env: false,
            env: vec![
                ("MGC_TEST_PM".to_string(), fake_pm.display().to_string()),
                (
                    "MGC_TEST_PID_FILE".to_string(),
                    pid_file.display().to_string(),
                ),
                ("MGC_TEST_MARKER".to_string(), marker.display().to_string()),
            ],
            timeout: Some(Duration::from_secs(5)),
            ..Default::default()
        },
    );
    let elapsed = started.elapsed();
    let error = result.expect_err("a child must not outlive a successful root command");
    let pid = fs::read_to_string(&pid_file)?.trim().parse::<u32>()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while test_process_is_alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let stopped = !test_process_is_alive(pid);
    if !stopped {
        force_kill_test_process_group(&pid_file);
    }

    assert!(
        error.to_string().contains("child processes remained")
            || error
                .to_string()
                .contains("forbidden package manager 'npm' spawned"),
        "unexpected root-exit guard error: {error}"
    );
    assert!(
        stopped,
        "background child PID {pid} survived root-exit cleanup"
    );
    assert!(
        !marker.exists(),
        "the delayed package-manager child ran after the root exited"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "the runner drained beyond its deadline after root exit: {elapsed:?}"
    );
    Ok(())
}

#[test]
#[cfg(unix)]
fn inherited_env_monitor_kills_all_session_escaped_forbidden_siblings()
-> Result<(), Box<dyn std::error::Error>> {
    use std::time::Instant;

    if std::process::Command::new("python3")
        .arg("-c")
        .arg("pass")
        .status()
        .is_err()
    {
        return Ok(());
    }

    let dir = tempfile::tempdir()?;
    let fake_cargo = dir.path().join("cargo");
    let npm_a_dir = dir.path().join("a");
    let npm_b_dir = dir.path().join("b");
    fs::create_dir_all(&npm_a_dir)?;
    fs::create_dir_all(&npm_b_dir)?;
    let fake_npm_a = npm_a_dir.join("npm");
    let fake_npm_b = npm_b_dir.join("npm");
    let pid_file_a = dir.path().join("escaped-a-pid");
    let pid_file_b = dir.path().join("escaped-b-pid");
    let marker_a = dir.path().join("escaped-a-ran");
    let marker_b = dir.path().join("escaped-b-ran");
    for (path, marker) in [(&fake_npm_a, &marker_a), (&fake_npm_b, &marker_b)] {
        fs::write(
            path,
            format!(
                "#!/bin/sh\n/bin/sleep 0.5\nprintf started > '{}'\n/bin/sleep 30\n",
                marker.display()
            ),
        )?;
        make_executable(path);
    }
    fs::write(
        &fake_cargo,
        "#!/bin/sh\npython3 -c 'import os,sys; open(sys.argv[1], \"w\").write(str(os.getpid())); os.setsid(); target=os.environ[\"MGC_TEST_PM_A\"]; os.execv(target, [target, \"install\"])' \"$MGC_TEST_PID_FILE_A\" &\npython3 -c 'import os,sys; open(sys.argv[1], \"w\").write(str(os.getpid())); os.setsid(); target=os.environ[\"MGC_TEST_PM_B\"]; os.execv(target, [target, \"install\"])' \"$MGC_TEST_PID_FILE_B\" &\n/bin/sleep 5\nwait\n",
    )?;
    make_executable(&fake_cargo);

    let started = Instant::now();
    let result = run_project_binary_inherited(
        &fake_cargo,
        &[],
        &ExecOptions {
            cwd: Some(dir.path().to_path_buf()),
            clean_env: false,
            env: vec![
                (
                    "MGC_TEST_PM_A".to_string(),
                    fake_npm_a.display().to_string(),
                ),
                (
                    "MGC_TEST_PM_B".to_string(),
                    fake_npm_b.display().to_string(),
                ),
                (
                    "MGC_TEST_PID_FILE_A".to_string(),
                    pid_file_a.display().to_string(),
                ),
                (
                    "MGC_TEST_PID_FILE_B".to_string(),
                    pid_file_b.display().to_string(),
                ),
            ],
            timeout: Some(Duration::from_secs(3)),
            ..Default::default()
        },
    );
    let elapsed = started.elapsed();
    let error = result.expect_err("session-escaped package managers must be rejected");
    let pids = [&pid_file_a, &pid_file_b].map(|path| {
        fs::read_to_string(path)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap()
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while pids.iter().any(|pid| test_process_is_alive(*pid)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let stopped = pids.iter().all(|pid| !test_process_is_alive(*pid));
    if !stopped {
        force_kill_test_process_group(&pid_file_a);
        force_kill_test_process_group(&pid_file_b);
    }

    assert!(
        error
            .to_string()
            .contains("forbidden package manager 'npm' spawned"),
        "unexpected process-guard error: {error}"
    );
    assert!(
        stopped,
        "one or more escaped sibling processes survived: {pids:?}"
    );
    assert!(!marker_a.exists(), "the first fake package manager ran");
    assert!(!marker_b.exists(), "the second fake package manager ran");
    assert!(
        elapsed < Duration::from_secs(2),
        "the runner did not stop both escaped sessions promptly: {elapsed:?}"
    );
    Ok(())
}

#[test]
#[cfg(unix)]
fn captured_runner_rejects_a_session_escape_when_root_exits_immediately()
-> Result<(), Box<dyn std::error::Error>> {
    if std::process::Command::new("python3")
        .arg("-c")
        .arg("pass")
        .status()
        .is_err()
    {
        return Ok(());
    }

    let dir = tempfile::tempdir()?;
    let fake_pm = dir.path().join("npm");
    let fake_cargo = dir.path().join("cargo");
    let pid_file = dir.path().join("immediate-exit-pid");
    let session_file = dir.path().join("immediate-exit-session");
    let marker = dir.path().join("immediate-exit-ran");
    fs::write(
        &fake_pm,
        format!(
            "#!/bin/sh\nprintf ran > '{}'\n/bin/sleep 2\n",
            marker.display()
        ),
    )?;
    make_executable(&fake_pm);
    fs::write(
        &fake_cargo,
        "#!/bin/sh\npython3 -c 'import os,sys,time; os.setsid(); open(sys.argv[1], \"w\").write(str(os.getpid())); open(os.environ[\"MGC_TEST_SESSION_FILE\"], \"w\").write(\"ready\"); time.sleep(0.2); target=os.environ[\"MGC_TEST_PM\"]; os.execv(target, [target, \"install\"])' \"$MGC_TEST_PID_FILE\" >/dev/null 2>&1 &\ni=0\nwhile [ ! -f \"$MGC_TEST_SESSION_FILE\" ] && [ \"$i\" -lt 100 ]; do /bin/sleep 0.01; i=$((i + 1)); done\nexit 0\n",
    )?;
    make_executable(&fake_cargo);

    let result = run_project_binary(
        &fake_cargo,
        &[],
        &ExecOptions {
            cwd: Some(dir.path().to_path_buf()),
            clean_env: false,
            env: vec![
                ("MGC_TEST_PM".to_string(), fake_pm.display().to_string()),
                (
                    "MGC_TEST_PID_FILE".to_string(),
                    pid_file.display().to_string(),
                ),
                (
                    "MGC_TEST_SESSION_FILE".to_string(),
                    session_file.display().to_string(),
                ),
            ],
            timeout: Some(Duration::from_secs(2)),
            ..Default::default()
        },
    );
    let pid = fs::read_to_string(&pid_file)?.trim().parse::<u32>()?;
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    while !marker.exists() && test_process_is_alive(pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let escaped_child_survived = test_process_is_alive(pid);
    if escaped_child_survived {
        force_kill_test_process_group(&pid_file);
    }

    assert!(
        result.is_err(),
        "root exit must not hide a detached package-manager child"
    );
    assert!(
        !escaped_child_survived,
        "the detached package-manager child survived root exit"
    );
    assert!(!marker.exists(), "the detached package manager ran");
    Ok(())
}

#[test]
#[cfg(unix)]
fn inherited_monitor_kills_session_started_during_term_grace()
-> Result<(), Box<dyn std::error::Error>> {
    use std::time::Instant;

    if std::process::Command::new("python3")
        .arg("-c")
        .arg("pass")
        .status()
        .is_err()
    {
        return Ok(());
    }

    let dir = tempfile::tempdir()?;
    let fake_pm = dir.path().join("npm");
    let fake_cargo = dir.path().join("cargo");
    let helper_script = dir.path().join("term-helper.py");
    let helper_pid_file = dir.path().join("term-helper-pid");
    let marker = dir.path().join("npm-started");
    fs::write(
        &helper_script,
        "import os, signal, sys, time\nsignal.signal(signal.SIGTERM, signal.SIG_IGN)\nos.setsid()\nopen(sys.argv[1], 'w').write(str(os.getpid()))\ntime.sleep(30)\n",
    )?;
    fs::write(
        &fake_pm,
        "#!/bin/sh\ntrap 'python3 \"$MGC_TEST_HELPER\" \"$MGC_TEST_PID_FILE\" >/dev/null 2>&1 &' TERM\nprintf started > \"$MGC_TEST_MARKER\"\nwhile :; do /bin/sleep 1; done\n",
    )?;
    make_executable(&fake_pm);
    fs::write(
        &fake_cargo,
        "#!/bin/sh\n\"$MGC_TEST_PM\" install &\ni=0\nwhile [ ! -f \"$MGC_TEST_MARKER\" ] && [ \"$i\" -lt 100 ]; do /bin/sleep 0.01; i=$((i + 1)); done\n/bin/sleep 5\nwait\n",
    )?;
    make_executable(&fake_cargo);

    let started = Instant::now();
    let result = run_project_binary_inherited(
        &fake_cargo,
        &[],
        &ExecOptions {
            cwd: Some(dir.path().to_path_buf()),
            clean_env: false,
            env: vec![
                ("MGC_TEST_PM".to_string(), fake_pm.display().to_string()),
                (
                    "MGC_TEST_HELPER".to_string(),
                    helper_script.display().to_string(),
                ),
                (
                    "MGC_TEST_PID_FILE".to_string(),
                    helper_pid_file.display().to_string(),
                ),
                ("MGC_TEST_MARKER".to_string(), marker.display().to_string()),
            ],
            timeout: Some(Duration::from_secs(5)),
            ..Default::default()
        },
    );
    let elapsed = started.elapsed();
    let error = result.expect_err("a forbidden child must remain rejected during TERM cleanup");
    let pid_deadline = Instant::now() + Duration::from_secs(1);
    while !helper_pid_file.exists() && Instant::now() < pid_deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let helper_pid = fs::read_to_string(&helper_pid_file)?
        .trim()
        .parse::<u32>()?;
    let stop_deadline = Instant::now() + Duration::from_secs(1);
    while test_process_is_alive(helper_pid) && Instant::now() < stop_deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let stopped = !test_process_is_alive(helper_pid);
    if !stopped {
        force_kill_test_process_group(&helper_pid_file);
    }

    assert!(
        error
            .to_string()
            .contains("forbidden package manager 'npm' spawned"),
        "unexpected process-guard error: {error}"
    );
    assert!(
        stopped,
        "session escaped helper PID {helper_pid} survived TERM/KILL cleanup"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "the guard exceeded its bounded cleanup window: {elapsed:?}"
    );
    Ok(())
}

#[test]
#[cfg(unix)]
fn clean_env_allows_only_the_executor_npm_version_probe_shim() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("npm-version-probe");
    fs::create_dir_all(&dir).unwrap();
    let fake_cargo = dir.join("cargo");
    let fake_npm = dir.join("npm");
    let marker = dir.join("external-npm-was-run");
    // Build the command indirectly so static script inspection cannot see it;
    // this exercises the runtime process guard and its read-only probe shim.
    // Dựng tên lệnh gián tiếp để đi qua guard runtime; kiểm tra shim chỉ đọc.
    fs::write(
        &fake_cargo,
        "#!/bin/sh\npm=n\npm=\"${pm}pm\"\n\"$pm\" --version\n",
    )
    .unwrap();
    fs::write(
        &fake_npm,
        format!("#!/bin/sh\nprintf ran > '{}'\nexit 99\n", marker.display()),
    )
    .unwrap();
    for path in [&fake_cargo, &fake_npm] {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    let opts = ExecOptions {
        clean_env: true,
        env: vec![("PATH".to_string(), dir.display().to_string())],
        timeout: Some(Duration::from_secs(2)),
        capture_full_stdout: true,
        ..Default::default()
    };
    let report = run(fake_cargo.to_str().unwrap(), &[], &opts)
        .expect("npm --version probe should use the executor's inert shim");

    assert_eq!(report.exit_code, 0);
    assert_eq!(report.stdout_full.trim(), "0.0.0");
    assert!(!marker.exists(), "the real npm executable must never run");
}

#[test]
#[cfg(unix)]
fn clean_env_still_blocks_npm_install_from_an_indirect_child() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("npm-install-blocked");
    fs::create_dir_all(&dir).unwrap();
    let fake_cargo = dir.join("cargo");
    let fake_npm = dir.join("npm");
    let marker = dir.join("external-npm-was-run");
    fs::write(
        &fake_cargo,
        "#!/bin/sh\npm=n\npm=\"${pm}pm\"\n\"$pm\" install\n",
    )
    .unwrap();
    fs::write(
        &fake_npm,
        format!("#!/bin/sh\nprintf ran > '{}'\n", marker.display()),
    )
    .unwrap();
    for path in [&fake_cargo, &fake_npm] {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    let opts = ExecOptions {
        clean_env: true,
        env: vec![("PATH".to_string(), dir.display().to_string())],
        timeout: Some(Duration::from_secs(2)),
        ..Default::default()
    };
    let error = run(fake_cargo.to_str().unwrap(), &[], &opts).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("MagiCore blocked forbidden package manager: npm")
            || error
                .to_string()
                .contains("forbidden package manager 'npm' spawned"),
        "unexpected error: {error}"
    );
    assert!(!marker.exists(), "npm install must not execute");
}

#[test]
#[cfg(unix)]
fn clean_env_kills_forbidden_pm_spawned_by_absolute_child_path() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("absolute-child-pm-bin");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let fake_cargo = dir.join("cargo");
    let fake_npm = dir.join("npm");
    // Margin matters: under parallel test load a 2s sleep with a 2s timeout
    // races; 5s sleep with a 10s timeout keeps the kill-detection semantics
    // stable regardless of machine load.
    // Biên thời gian quan trọng: dưới tải test song song, sleep 2s với
    // timeout 2s sẽ đua nhau; sleep 5s + timeout 10s giữ nguyên semantics
    // phát hiện kill, ổn định dù máy có tải.
    fs::write(&fake_npm, "#!/bin/sh\n/bin/sleep 5\n").unwrap();
    fs::write(
        &fake_cargo,
        format!("#!/bin/sh\n\"{}\"\n", fake_npm.display()),
    )
    .unwrap();
    for path in [&fake_cargo, &fake_npm] {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    let opts = ExecOptions {
        clean_env: true,
        timeout: Some(Duration::from_secs(10)),
        ..Default::default()
    };

    let err = run(
        fake_cargo.to_str().unwrap(),
        &["--version".to_string()],
        &opts,
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("references forbidden package manager 'npm'")
            || err
                .to_string()
                .contains("forbidden package manager 'npm' spawned"),
        "unexpected error: {err}"
    );
}

#[test]
#[cfg(unix)]
fn project_binary_rejects_forbidden_binary_name() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("project-bin-forbidden-name");
    fs::create_dir_all(&dir).unwrap();
    let fake_npm = dir.join("npm");
    fs::write(&fake_npm, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = fs::metadata(&fake_npm).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_npm, permissions).unwrap();

    let opts = ExecOptions {
        clean_env: true,
        ..Default::default()
    };
    let err = run_project_binary(&fake_npm, &[], &opts).unwrap_err();
    assert!(err.to_string().contains("forbidden package manager"));
}

#[test]
#[cfg(unix)]
fn project_binary_rejects_script_that_references_forbidden_pm() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("project-bin-script-pm");
    fs::create_dir_all(&dir).unwrap();
    let fake_bin = dir.join("tool");
    fs::write(&fake_bin, "#!/bin/sh\n/usr/bin/npm --version\n").unwrap();
    let mut permissions = fs::metadata(&fake_bin).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_bin, permissions).unwrap();

    let opts = ExecOptions {
        clean_env: true,
        ..Default::default()
    };
    let err = run_project_binary(&fake_bin, &[], &opts).unwrap_err();
    assert!(
        err.to_string()
            .contains("references forbidden package manager")
    );
}

#[test]
#[cfg(unix)]
fn inherited_project_binary_rejects_script_that_references_forbidden_pm() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir().join("project-bin-inherited-script-pm");
    fs::create_dir_all(&dir).unwrap();
    let fake_bin = dir.join("tool");
    fs::write(&fake_bin, "#!/bin/sh\n/usr/bin/yarn --version\n").unwrap();
    let mut permissions = fs::metadata(&fake_bin).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_bin, permissions).unwrap();

    let opts = ExecOptions {
        clean_env: true,
        ..Default::default()
    };
    let err = run_project_binary_inherited(&fake_bin, &[], &opts).unwrap_err();
    assert!(
        err.to_string()
            .contains("references forbidden package manager")
    );
}

#[test]
fn npm_is_blocked_even_inside_react_native_subdir() {
    let base = std::env::temp_dir().join(format!("mgexec-rn-{}", std::process::id()));
    let rn = base.join("react-native");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&rn).unwrap();
    fs::write(base.join("mgc.toml"), "[app]\nlanguage = \"multi\"\n").unwrap();
    fs::write(
        rn.join("package.json"),
        "{\"dependencies\": {\"react-native\": \"0.7x\"}}",
    )
    .unwrap();

    let opts = ExecOptions {
        cwd: Some(rn.clone()),
        clean_env: true,
        ..Default::default()
    };
    let err = run_inherited("npm", &["install".to_string()], &opts).unwrap_err();
    assert!(
        err.to_string().contains("is forbidden"),
        "npm inside react-native subdir must stay blocked, got: {err}"
    );

    let outside = ExecOptions {
        clean_env: true,
        ..Default::default()
    };
    let err = run_inherited("npm", &["install".to_string()], &outside).unwrap_err();
    assert!(
        err.to_string().contains("is forbidden"),
        "npm outside react-native subdir must be rejected, got: {err}"
    );

    let _ = fs::remove_dir_all(&base);
}
