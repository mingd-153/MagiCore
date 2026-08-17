//! mg doctor — environment diagnostic (18 §18)
//! (Check toolchain + PATH + store quyền + disk + network registry reachable.
//!  `--json` cho agent parse. Không sửa gì — chỉ báo cáo.)

use anyhow::Result;
use clap::Subcommand;
use serde::Serialize;

#[derive(Subcommand, Debug, Clone)]
pub enum DoctorCmd {
    /// Environment diagnostic (default: human table)
    Check {
        #[arg(short, long, help = "output JSON")]
        json: bool,
    },
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub toolchain: Vec<Tool>,
    pub store_dir: String,
    pub store_writable: bool,
    pub disk_free_bytes: u64,
    pub registries_reachable: Vec<RegistryStatus>,
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
    for dir in path.split(':') {
        let candidate = std::path::Path::new(dir).join(bin);
        if candidate.exists() {
            return Some(candidate.display().to_string());
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

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let store_dir = std::path::Path::new(&home).join(".megagate");
    let store_writable = std::fs::create_dir_all(&store_dir).is_ok();
    let disk_free_bytes = fs2_avail(&store_dir);

    let mut registries_reachable = Vec::new();
    for c in crate::commands::network::outbound_connections() {
        registries_reachable.push(RegistryStatus {
            host: c.host.clone(),
            reachable: crate::commands::network::reachable_public(&c.host, c.port),
        });
    }

    Ok(DoctorReport {
        toolchain,
        store_dir: store_dir.display().to_string(),
        store_writable,
        disk_free_bytes,
        registries_reachable,
    })
}

/// Free disk trên mount chứa path — parse `df -k` (ponytail: không dep libc/fs2 mới)
fn fs2_avail(path: &std::path::Path) -> u64 {
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

pub fn handle(cmd: DoctorCmd) -> Result<()> {
    let rep = report()?;
    match cmd {
        DoctorCmd::Check { json: true } => {
            println!("{}", serde_json::to_string_pretty(&rep)?);
        }
        DoctorCmd::Check { json: false } => {
            println!("MegaGate doctor — environment check");
            for t in &rep.toolchain {
                let mark = if t.path.is_some() { "OK" } else { "MISSING" };
                println!(
                    "{mark:8} {} {:<10} {}",
                    t.name,
                    t.version,
                    t.path.as_deref().unwrap_or("")
                );
            }
            println!(
                "{:8} store {} (writable: {})",
                if rep.store_writable { "OK" } else { "ERROR" },
                rep.store_dir,
                rep.store_writable
            );
            println!("{:8} disk free: {} bytes", "INFO", rep.disk_free_bytes);
            for r in &rep.registries_reachable {
                let mark = if r.reachable { "OK" } else { "UNREACHABLE" };
                println!("{mark:8} registry {}", r.host);
            }
        }
    }
    Ok(())
}
