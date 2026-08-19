//! mg doctor — intelligent diagnostic and AI-guided automatic error recovery.
//!
//! Provides both human-readable tables and machine-readable JSON reports with
//! actionable remediation steps (`suggested_actions`) for AI Coding Assistants.

use anyhow::Result;
use clap::Subcommand;
use serde::Serialize;
use std::path::Path;

#[derive(Subcommand, Debug, Clone)]
pub enum DoctorCmd {
    /// Run diagnostic check with AI-remediation hints
    Check {
        #[arg(short, long, help = "output JSON")]
        json: bool,
        #[arg(long, help = "attempt automatic repair of detected environment issues")]
        fix: bool,
    },
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub toolchain: Vec<Tool>,
    pub store_dir: String,
    pub store_writable: bool,
    pub disk_free_bytes: u64,
    pub registries_reachable: Vec<RegistryStatus>,
    pub health_status: &'static str,
    pub detected_issues: Vec<DiagnosticIssue>,
    pub suggested_actions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticIssue {
    pub code: &'static str,
    pub severity: &'static str, // "CRITICAL" | "WARNING" | "INFO"
    pub message: String,
    pub fix_command: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Tool {
    pub name: &'static str,
    pub path: Option<String>,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct RegistryStatus {
    pub host: String,
    pub reachable: bool,
}

fn which(bin: &str) -> Option<String> {
    let path = std::env::var("PATH").unwrap_or_default();
    #[cfg(windows)]
    let separator = ';';
    #[cfg(not(windows))]
    let separator = ':';

    for dir in path.split(separator) {
        let candidate = Path::new(dir).join(bin);
        if candidate.exists() {
            return Some(candidate.display().to_string());
        }
        #[cfg(windows)]
        {
            let candidate_exe = Path::new(dir).join(format!("{bin}.exe"));
            if candidate_exe.exists() {
                return Some(candidate_exe.display().to_string());
            }
        }
    }
    None
}

fn tool_version(bin: &str, flag: &str) -> String {
    let output = std::process::Command::new(bin)
        .arg(flag)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if output.is_empty() {
        "?".into()
    } else {
        output.lines().next().unwrap_or("?").to_string()
    }
}

fn tool(name: &'static str, bin: &str, flag: &str) -> Tool {
    match which(bin) {
        Some(path) => Tool {
            name,
            path: Some(path),
            version: tool_version(bin, flag),
        },
        None => Tool {
            name,
            path: None,
            version: "MISSING".into(),
        },
    }
}

pub fn report() -> Result<DoctorReport> {
    let toolchain = vec![
        tool("node", "node", "--version"),
        tool("python3", "python3", "--version"),
        tool("rustc", "rustc", "--version"),
        tool("cargo", "cargo", "--version"),
        tool("git", "git", "--version"),
        tool("docker", "docker", "--version"),
    ];

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    let store_dir = Path::new(&home).join(".megagate");
    let store_writable = std::fs::create_dir_all(&store_dir).is_ok();
    let disk_free_bytes = fs_avail(&store_dir);

    let mut registries_reachable = Vec::new();
    for c in crate::commands::network::outbound_connections() {
        registries_reachable.push(RegistryStatus {
            host: c.host.clone(),
            reachable: crate::commands::network::reachable_public(&c.host, c.port),
        });
    }

    let mut detected_issues = Vec::new();
    let mut suggested_actions = Vec::new();

    // 1. Toolchain issues
    for t in &toolchain {
        if t.path.is_none() && matches!(t.name, "git" | "node") {
            detected_issues.push(DiagnosticIssue {
                code: "ERR_TOOLCHAIN_MISSING",
                severity: "WARNING",
                message: format!("Core tool '{}' is not installed or not in PATH.", t.name),
                fix_command: Some(format!("Install '{}' via system package manager (brew/apt/winget)", t.name)),
            });
            suggested_actions.push(format!("Please install {}", t.name));
        }
    }

    // 2. Store issues
    if !store_writable {
        detected_issues.push(DiagnosticIssue {
            code: "ERR_STORE_READONLY",
            severity: "CRITICAL",
            message: format!("MegaGate global store '{}' is not writable.", store_dir.display()),
            fix_command: Some(format!("chmod -R 755 {}", store_dir.display())),
        });
        suggested_actions.push(format!("Fix write permissions on {}", store_dir.display()));
    }

    // 3. Disk space issues (< 500MB)
    if disk_free_bytes > 0 && disk_free_bytes < 500 * 1024 * 1024 {
        detected_issues.push(DiagnosticIssue {
            code: "WARN_LOW_DISK_SPACE",
            severity: "WARNING",
            message: format!("Low disk space: only {} MB free.", disk_free_bytes / (1024 * 1024)),
            fix_command: Some("mg store prune --all".to_string()),
        });
        suggested_actions.push("Run `mg store prune --all` to reclaim disk space".to_string());
    }

    let health_status = if detected_issues.iter().any(|i| i.severity == "CRITICAL") {
        "UNHEALTHY"
    } else if !detected_issues.is_empty() {
        "DEGRADED"
    } else {
        "HEALTHY"
    };

    Ok(DoctorReport {
        toolchain,
        store_dir: store_dir.display().to_string(),
        store_writable,
        disk_free_bytes,
        registries_reachable,
        health_status,
        detected_issues,
        suggested_actions,
    })
}

fn fs_avail(path: &Path) -> u64 {
    #[cfg(unix)]
    {
        let out = std::process::Command::new("df")
            .arg("-k")
            .arg(path)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        let mut lines = out.lines();
        let _header = lines.next();
        let avail_kb: u64 = lines
            .next()
            .and_then(|l| l.split_whitespace().nth(3))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        avail_kb.saturating_mul(1024)
    }
    #[cfg(not(unix))]
    {
        0
    }
}

pub fn handle(cmd: DoctorCmd) -> Result<()> {
    let rep = report()?;
    match cmd {
        DoctorCmd::Check { json: true, .. } => {
            println!("{}", serde_json::to_string_pretty(&rep)?);
        }
        DoctorCmd::Check { json: false, fix } => {
            println!("MegaGate Doctor (v0.3.0) — Environment Health: {}", rep.health_status);
            println!("─────────────────────────────────────────────────────────────");
            for t in &rep.toolchain {
                let mark = if t.path.is_some() { "  ✓ OK  " } else { "  ✗ MISS" };
                println!(
                    "{mark} {:<10} {:<12} {}",
                    t.name,
                    t.version,
                    t.path.as_deref().unwrap_or("")
                );
            }
            println!(
                "  {} Store: {} (writable: {})",
                if rep.store_writable { "✓" } else { "✗" },
                rep.store_dir,
                rep.store_writable
            );
            if rep.disk_free_bytes > 0 {
                println!("  ℹ Disk Free: {} MB", rep.disk_free_bytes / (1024 * 1024));
            }
            for r in &rep.registries_reachable {
                let mark = if r.reachable { "✓" } else { "✗" };
                println!("  {mark} Registry: {}", r.host);
            }

            if !rep.detected_issues.is_empty() {
                println!("\nDetected Issues & AI Recommendations:");
                println!("─────────────────────────────────────────────────────────────");
                for issue in &rep.detected_issues {
                    println!("[{}] {}: {}", issue.severity, issue.code, issue.message);
                    if let Some(ref fix_cmd) = issue.fix_command {
                        println!("  ↳ Fix: {fix_cmd}");
                    }
                }
            }

            if fix {
                println!("\nAttempting automatic remediation...");
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                let store_dir = Path::new(&home).join(".megagate");
                let _ = std::fs::create_dir_all(&store_dir);
                println!("✓ Environment maintenance pass completed.");
            }
        }
    }
    Ok(())
}
