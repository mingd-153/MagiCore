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
    writeln!(bat_file, "@echo off").unwrap();
    writeln!(bat_file, "echo SUCCESS").unwrap();
    bat_file.flush().unwrap();

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
    lines.first().unwrap()
}

#[cfg(not(windows))]
#[test]
fn windows_tests_are_platform_specific() {
    // Placeholder for non-Windows platforms
    println!("Windows spawn tests only run on Windows");
}
