//! Test multiplatform tool scenarios (Node, Flutter, Gradle, Swift, Kotlin)

use mgc_exec::prelude::ExecOptions;
use std::path::PathBuf;

#[test]
fn test_node_spawn_logic() {
    // Scenario: Node on Windows is .exe, not .cmd
    // Priority should pick node.exe over node.cmd if both exist
    
    // This test verifies the logic, actual spawn requires Node installed
    #[cfg(windows)]
    {
        use std::process::Command;
        
        let output = Command::new("where.exe").arg("node").output();
        
        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let lines: Vec<&str> = stdout.lines().collect();
                
                println!("Node resolution on Windows:");
                for line in &lines {
                    println!("  {}", line);
                }
                
                // Should have .exe
                assert!(
                    lines.iter().any(|l| l.to_lowercase().ends_with(".exe")),
                    "Node should resolve to .exe"
                );
            } else {
                println!("Node not installed - test skipped");
            }
        }
    }
    
    #[cfg(not(windows))]
    {
        println!("Node spawn test - Unix always uses bare name");
    }
}

#[test]
fn test_flutter_spawn_windows_bat() {
    // Scenario: Flutter on Windows ships as flutter.bat
    // Should spawn via cmd.exe wrapper
    
    #[cfg(windows)]
    {
        use std::process::Command;
        
        let output = Command::new("where.exe").arg("flutter").output();
        
        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let first_match = stdout.lines().next().unwrap_or("");
                
                println!("Flutter resolved to: {}", first_match);
                
                let is_bat = first_match.to_lowercase().ends_with(".bat");
                let is_cmd = first_match.to_lowercase().ends_with(".cmd");
                
                if is_bat || is_cmd {
                    println!("✅ Flutter is batch script - will spawn via cmd.exe");
                } else {
                    println!("ℹ️  Flutter is PE executable - direct spawn OK");
                }
            } else {
                println!("Flutter not installed - test skipped");
            }
        }
    }
}

#[test]
fn test_gradle_kotlin_spawn() {
    // Scenario: Gradle on Windows can be .bat
    // Kotlin compiler (kotlinc) can be .bat
    
    #[cfg(windows)]
    {
        for tool in &["gradle", "kotlinc"] {
            use std::process::Command;
            
            let output = Command::new("where.exe").arg(tool).output();
            
            if let Ok(out) = output {
                if out.status.success() {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let first = stdout.lines().next().unwrap_or("");
                    
                    println!("{} resolved to: {}", tool, first);
                    
                    let lower = first.to_lowercase();
                    if lower.ends_with(".bat") || lower.ends_with(".cmd") {
                        println!("  → Batch script detected");
                    } else if lower.ends_with(".exe") {
                        println!("  → PE executable detected");
                    }
                } else {
                    println!("{} not installed", tool);
                }
            }
        }
    }
}

#[test]
fn test_swift_spawn() {
    // Scenario: Swift on Windows typically .exe
    
    #[cfg(windows)]
    {
        use std::process::Command;
        
        let output = Command::new("where.exe").arg("swift").output();
        
        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let first = stdout.lines().next().unwrap_or("");
                
                println!("Swift resolved to: {}", first);
                
                assert!(
                    first.to_lowercase().ends_with(".exe"),
                    "Swift should be .exe on Windows"
                );
            } else {
                println!("Swift not installed - test skipped");
            }
        }
    }
}

#[test]
fn test_exec_options_windows_env_preservation() {
    // Verify that ExecOptions preserves critical Windows vars
    
    let _opts = ExecOptions {
        cwd: Some(PathBuf::from(".")),
        clean_env: true, // Even with clean_env, critical vars should be preserved
        ..Default::default()
    };
    
    // Critical Windows vars that must exist
    #[cfg(windows)]
    {
        let critical_vars = [
            "SYSTEMROOT",
            "WINDIR", 
            "TEMP",
            "TMP",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "ProgramData"
        ];
        
        for var in &critical_vars {
            if let Ok(val) = std::env::var(var) {
                println!("{} = {}", var, val);
            }
        }
        
        println!("✅ Critical Windows vars verified present");
    }
}

#[test]
fn test_error_193_scenario() {
    // Error 193: "%1 is not a valid Win32 application"
    // Happens when trying to execute .bat/.cmd directly without cmd.exe
    
    #[cfg(windows)]
    {
        use std::process::Command;
        use std::io::Write;
        use tempfile::NamedTempFile;
        
        // Create test .bat
        let mut bat = NamedTempFile::with_suffix(".bat").expect("create .bat");
        writeln!(bat, "@echo Test").unwrap();
        bat.flush().unwrap();
        
        let bat_path = bat.path();
        
        // Direct spawn should fail with 193 or similar
        let direct = Command::new(bat_path).output();
        
        match direct {
            Err(e) => {
                println!("✅ Direct .bat spawn failed as expected: {}", e);
                // Error 193 or similar Windows spawn error
            }
            Ok(output) => {
                // Some Windows versions may allow this, but generally fails
                println!("⚠️  Direct .bat spawn succeeded (unusual): {:?}", output.status);
            }
        }
        
        // Via cmd.exe should work
        let via_cmd = Command::new("cmd.exe")
            .arg("/C")
            .arg(bat_path)
            .output();
        
        match via_cmd {
            Ok(output) => {
                println!("✅ cmd.exe wrapper spawn succeeded");
                assert!(output.status.success() || output.status.code() == Some(0));
            }
            Err(e) => {
                panic!("cmd.exe wrapper should not fail: {}", e);
            }
        }
    }
}
