//! Windows spawn integration test - verify .bat/.cmd/.exe resolution
//! Test này chạy chỉ trên Windows để verify logic spawn

#[cfg(windows)]
#[test]
fn test_where_exe_resolves_node_to_exe() {
    use std::process::Command;

    // Verify where.exe finds node.exe (not node.cmd)
    let output = Command::new("where.exe")
        .arg("node")
        .output()
        .expect("where.exe should be available on Windows");

    assert!(output.status.success(), "where.exe node should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    // Node should resolve to .exe
    assert!(
        lines.iter().any(|l| l.to_lowercase().ends_with(".exe")),
        "Node should have .exe in PATH: {:?}",
        lines
    );
}

#[cfg(windows)]
#[test]
fn test_flutter_bat_detection() {
    use mgc_exec::prelude::{ExecOptions, run};
    use std::process::Command;

    // Check if flutter is .bat or .exe
    let output = Command::new("where.exe")
        .arg("flutter")
        .output()
        .expect("where.exe should be available on Windows");
    assert!(
        output.status.success(),
        "Flutter must be installed on the required Windows CI runner: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    println!("Flutter resolution:");
    for line in &lines {
        println!("  {}", line);
    }

    let has_bat = lines.iter().any(|l| {
        let lower = l.to_lowercase();
        lower.ends_with(".bat") || lower.ends_with(".cmd")
    });

    let has_exe = lines.iter().any(|l| l.to_lowercase().ends_with(".exe"));

    println!("Has .bat/.cmd: {}", has_bat);
    println!("Has .exe: {}", has_exe);

    // Assert: if Flutter available, should be either .bat or .exe
    assert!(
        has_bat || has_exe,
        "Flutter should be .bat or .exe, got: {:?}",
        lines
    );

    // This is the production path: bare `flutter` must pass the allowlist,
    // resolve the PATH shim, and execute it. `where.exe` alone is insufficient.
    // Đây là đường production: `flutter` phải qua allowlist, resolver PATH,
    // rồi thực thi được shim; chỉ kiểm tra `where.exe` là chưa đủ.
    let report = run(
        "flutter",
        &["--version".to_string()],
        &ExecOptions::default(),
    )
    .expect("mgc-exec should run Flutter through its Windows resolver");
    assert_eq!(
        report.exit_code, 0,
        "Flutter resolver execution failed: stdout={} stderr={}",
        report.stdout_tail, report.stderr_tail
    );
}

#[cfg(windows)]
#[test]
fn test_project_bat_executes_through_mgc_exec() {
    use mgc_exec::prelude::{ExecOptions, run_project_binary};
    use std::io::Write;

    // Create a test .bat file
    // .bat extension is required: cmd /C dispatches by extension — a
    // random temp name without .bat is treated as an executable and fails.
    // Cần đuôi .bat: cmd /C phân phối theo đuôi file — tên temp không đuôi
    // bị coi là executable và fail.
    let mut bat_file = tempfile::Builder::new()
        .suffix(".bat")
        .rand_bytes(8)
        .tempfile()
        .expect("create temp .bat file");
    writeln!(bat_file, "@echo off").expect("write batch header");
    writeln!(bat_file, "echo SUCCESS").expect("write batch body");
    bat_file.flush().expect("flush batch script");

    // Persist to disk and close handle (Windows requires file handle closed before spawn)
    let (file, bat_path) = bat_file.keep().expect("persist temp file");
    drop(file); // Explicitly close file handle

    // Run through the public production API, not a duplicated cmd.exe command.
    // Chạy qua API production, không tự dựng lại lệnh cmd.exe trong test.
    let report = run_project_binary(&bat_path, &[], &ExecOptions::default())
        .expect("mgc-exec should execute a project .bat file");
    assert_eq!(
        report.exit_code, 0,
        "bat execution failed: stdout={} stderr={}",
        report.stdout_tail, report.stderr_tail
    );
    assert!(
        report.stdout_tail.contains("SUCCESS"),
        "Should execute bat content, got: {:?}",
        report.stdout_tail
    );

    // Clean up
    let _ = std::fs::remove_file(&bat_path);
}

#[cfg(windows)]
#[test]
fn test_project_bat_rejects_shell_operator_arguments_before_execution()
-> Result<(), Box<dyn std::error::Error>> {
    use mgc_exec::prelude::{ExecOptions, run_project_binary};
    use std::io::Write;

    let root = tempfile::tempdir()?;
    let mut bat_file = tempfile::Builder::new()
        .prefix("mgc-safe-")
        .suffix(".bat")
        .tempfile_in(root.path())?;
    writeln!(bat_file, "@echo off")?;
    bat_file.flush()?;
    let (file, bat_path) = bat_file.keep()?;
    drop(file);

    let marker = root.path().join("batch-injected.txt");
    let result = run_project_binary(
        &bat_path,
        &["& echo PWNED > batch-injected.txt".to_string()],
        &ExecOptions {
            cwd: Some(root.path().to_path_buf()),
            ..Default::default()
        },
    );

    assert!(
        result.is_err(),
        "batch execution must reject shell operators before starting cmd.exe"
    );
    assert!(
        !marker.exists(),
        "rejected input must not create a command-injection marker"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn test_job_object_terminates_forbidden_descendant_tree() -> Result<(), Box<dyn std::error::Error>>
{
    use mgc_exec::prelude::{ExecOptions, run_project_binary};
    use std::time::{Duration, Instant};

    let root = tempfile::tempdir()?;
    let comspec = std::env::var_os("COMSPEC").ok_or("Windows must expose COMSPEC")?;
    let forbidden_exe = root.path().join("npm.exe");
    std::fs::copy(comspec, &forbidden_exe)?;
    let batch = root.path().join("job-tree.bat");
    std::fs::write(
        &batch,
        "@echo off\r\nstart \"\" /B \"%MGC_JOB_NPM_EXE%\" /D /S /C \"ping -n 30 127.0.0.1 > NUL\"\r\nping -n 30 127.0.0.1 > NUL\r\n",
    )?;

    let started = Instant::now();
    let error = run_project_binary(
        &batch,
        &[],
        &ExecOptions {
            cwd: Some(root.path().to_path_buf()),
            clean_env: true,
            env: vec![(
                "MGC_JOB_NPM_EXE".to_string(),
                forbidden_exe.display().to_string(),
            )],
            timeout: Some(Duration::from_secs(10)),
            ..Default::default()
        },
    )
    .expect_err("the forbidden descendant must be detected and terminated")
    .to_string();
    assert!(
        error.contains("forbidden package manager 'npm' spawned"),
        "Job Object startup or forbidden-child detection failed: {error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the process tree did not stop promptly"
    );

    let process_id = error
        .split("child process ")
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
        .ok_or("forbidden-child error must contain its PID")?
        .parse::<u32>()?;
    let mut system = sysinfo::System::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut still_running = true;
    while Instant::now() < deadline {
        system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        if system.process(sysinfo::Pid::from_u32(process_id)).is_none() {
            still_running = false;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !still_running,
        "forbidden process {process_id} survived after the Job Object guard closed"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn test_job_object_detects_forbidden_child_after_root_exits()
-> Result<(), Box<dyn std::error::Error>> {
    use mgc_exec::prelude::{ExecOptions, run_project_binary};
    use std::time::{Duration, Instant};

    let root = tempfile::tempdir()?;
    let comspec = std::env::var_os("COMSPEC").ok_or("Windows must expose COMSPEC")?;
    let forbidden_exe = root.path().join("npm.exe");
    std::fs::copy(comspec, &forbidden_exe)?;
    let batch = root.path().join("job-root-exit.bat");
    std::fs::write(
        &batch,
        "@echo off\r\nstart \"\" /B \"%MGC_JOB_NPM_EXE%\" /D /S /C \"ping -n 30 127.0.0.1 > NUL\"\r\nexit /b 0\r\n",
    )?;

    let started = Instant::now();
    let error = run_project_binary(
        &batch,
        &[],
        &ExecOptions {
            cwd: Some(root.path().to_path_buf()),
            clean_env: true,
            env: vec![(
                "MGC_JOB_NPM_EXE".to_string(),
                forbidden_exe.display().to_string(),
            )],
            timeout: Some(Duration::from_secs(10)),
            ..Default::default()
        },
    )
    .expect_err("a forbidden process must not outlive a successful root command")
    .to_string();
    assert!(
        error.contains("forbidden package manager 'npm' spawned"),
        "root-exit job inspection missed the forbidden child: {error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the process tree did not stop promptly after the root exited"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn test_clean_env_preserves_windows_runtime_variables() {
    use mgc_exec::prelude::{ExecOptions, run_project_binary};
    use std::path::PathBuf;

    let comspec = std::env::var_os("COMSPEC").expect("Windows must expose COMSPEC");
    let report = run_project_binary(
        &PathBuf::from(comspec),
        &[
            "/D".to_string(),
            "/S".to_string(),
            "/C".to_string(),
            "echo %SYSTEMROOT%".to_string(),
        ],
        &ExecOptions {
            clean_env: true,
            ..Default::default()
        },
    )
    .expect("clean environment must still start cmd.exe");

    assert_eq!(
        report.exit_code, 0,
        "cmd.exe failed: {}",
        report.stderr_tail
    );
    assert!(
        report
            .stdout_tail
            .trim()
            .to_ascii_lowercase()
            .contains("windows"),
        "SYSTEMROOT was removed after env_clear: {:?}",
        report.stdout_tail
    );
}

#[cfg(windows)]
#[test]
fn test_priority_order_exe_over_bat() {
    // Priority: .exe > .com > .cmd/.bat > extensionless — extensionless PATH
    // entries are usually Git-bash sh scripts (flutter ships both `flutter`
    // sh script and `flutter.bat`); spawning them yields os error 193.
    // Ưu tiên: .exe > .com > .cmd/.bat > không đuôi — file không đuôi trên
    // Windows thường là sh script Git-bash, spawn trực tiếp lỗi 193.
    let test_cases = vec![
        (
            vec!["C:\\path\\node.exe", "C:\\path\\node.cmd"],
            "C:\\path\\node.exe",
            ".exe should win",
        ),
        (
            vec!["C:\\path\\tool.com", "C:\\path\\tool.bat"],
            "C:\\path\\tool.com",
            ".com should win over .bat",
        ),
        (
            vec!["C:\\path\\tool", "C:\\path\\tool.cmd"],
            "C:\\path\\tool.cmd",
            ".cmd should win over extensionless sh script",
        ),
        (
            vec!["C:\\path\\script.cmd", "C:\\path\\script.bat"],
            "C:\\path\\script.cmd",
            ".cmd should win over .bat",
        ),
        (
            vec!["C:\\path\\only.bat"],
            "C:\\path\\only.bat",
            "lone .bat should be picked",
        ),
    ];

    for (lines, expected, reason) in test_cases {
        let result = pick_by_priority(&lines);
        assert_eq!(result, expected, "{}", reason);
    }
}

#[cfg(windows)]
fn pick_by_priority<'a>(lines: &'a [&'a str]) -> &'a str {
    // Replicate resolve_windows_shim logic

    // Priority: .exe > .com > .cmd/.bat > extensionless
    let is_pe = |l: &str| {
        let lower = l.to_ascii_lowercase();
        lower.ends_with(".exe") || lower.ends_with(".com")
    };
    let is_shim = |l: &str| {
        let lower = l.to_ascii_lowercase();
        lower.ends_with(".cmd") || lower.ends_with(".bat")
    };

    if let Some(pe) = lines.iter().find(|l| is_pe(l)) {
        return pe;
    }
    if let Some(shim) = lines.iter().find(|l| is_shim(l)) {
        return shim;
    }
    lines.first().expect("at least one command candidate")
}

#[cfg(not(windows))]
#[test]
fn windows_tests_are_platform_specific() {
    // Placeholder for non-Windows platforms
    println!("Windows spawn tests only run on Windows");
}
