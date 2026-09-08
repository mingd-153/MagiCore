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
    use std::process::Command;
    
    // Check if flutter is .bat or .exe
    let output = Command::new("where.exe")
        .arg("flutter")
        .output();
    
    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
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
        } else {
            println!("Flutter not installed on this system - test skipped");
        }
    }
}

#[cfg(windows)]
#[test]
fn test_cmd_exe_spawn_bat_file() {
    use std::process::Command;
    use std::io::Write;
    use tempfile::NamedTempFile;
    
    // Create a test .bat file
    let mut bat_file = NamedTempFile::new().expect("create temp file");
    writeln!(bat_file, "@echo off").unwrap();
    writeln!(bat_file, "echo SUCCESS").unwrap();
    bat_file.flush().unwrap();
    
    let bat_path = bat_file.path();
    
    // Try spawning directly (should fail with error 193)
    let direct = Command::new(bat_path).output();
    
    println!("Direct spawn result: {:?}", direct);
    
    // Spawn via cmd.exe (should work)
    let via_cmd = Command::new("cmd.exe")
        .arg("/D")
        .arg("/S")
        .arg("/C")
        .arg(format!("\"{}\"", bat_path.display()))
        .output()
        .expect("cmd.exe spawn should work");
    
    assert!(via_cmd.status.success(), "cmd.exe spawn should succeed");
    
    let stdout = String::from_utf8_lossy(&via_cmd.stdout);
    assert!(stdout.contains("SUCCESS"), "Should execute bat content");
}

#[cfg(windows)]
#[test]
fn test_priority_order_exe_over_bat() {
    // Test priority logic: .exe > .com > extensionless > .cmd > .bat
    let test_cases = vec![
        (
            vec!["C:\\path\\node.exe", "C:\\path\\node.cmd"],
            "C:\\path\\node.exe",
            ".exe should win"
        ),
        (
            vec!["C:\\path\\tool.com", "C:\\path\\tool.bat"],
            "C:\\path\\tool.com",
            ".com should win over .bat"
        ),
        (
            vec!["C:\\path\\tool", "C:\\path\\tool.cmd"],
            "C:\\path\\tool",
            "extensionless should win over .cmd"
        ),
        (
            vec!["C:\\path\\script.cmd", "C:\\path\\script.bat"],
            "C:\\path\\script.cmd",
            ".cmd should win over .bat"
        ),
        (
            vec!["C:\\path\\only.bat"],
            "C:\\path\\only.bat",
            "lone .bat should be picked"
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
    
    // Priority: .exe > .com > extensionless > .cmd > .bat
    if let Some(exe) = lines.iter().find(|l| l.to_ascii_lowercase().ends_with(".exe")) {
        return exe;
    }
    if let Some(com) = lines.iter().find(|l| l.to_ascii_lowercase().ends_with(".com")) {
        return com;
    }
    
    let is_script = |l: &str| {
        let lower = l.to_ascii_lowercase();
        lower.ends_with(".cmd") || lower.ends_with(".bat")
    };
    
    if let Some(direct) = lines.iter().find(|l| !is_script(l)) {
        return direct;
    }
    
    // Last resort: .cmd/.bat
    lines.first().unwrap()
}

#[cfg(not(windows))]
#[test]
fn windows_tests_are_platform_specific() {
    // Placeholder for non-Windows platforms
    println!("Windows spawn tests only run on Windows");
}
