use std::path::PathBuf;

#[derive(clap::Subcommand)]
pub enum DaemonSubcommand {
    /// Start the daemon
    Start,
    /// Stop the daemon
    Stop,
    /// Check daemon status
    Status,
}

fn daemon_pid_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mgpm")
        .join("daemon.pid")
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let output = std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .output();
        matches!(output, Ok(o) if o.status.success())
    }
    #[cfg(not(unix))]
    {
        false
    }
}

pub fn cmd_daemon(command: DaemonSubcommand) -> Result<(), String> {
    match command {
        DaemonSubcommand::Start => cmd_daemon_start(),
        DaemonSubcommand::Stop => cmd_daemon_stop(),
        DaemonSubcommand::Status => cmd_daemon_status(),
    }
}

fn cmd_daemon_start() -> Result<(), String> {
    let pid_path = daemon_pid_path();
    if pid_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if is_process_running(pid) {
                    return Err(format!("daemon already running (PID: {})", pid));
                }
            }
        }
        std::fs::remove_file(&pid_path).ok();
    }

    if let Some(parent) = pid_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create daemon directory: {e}"))?;
    }

    let pid = std::process::id();
    std::fs::write(&pid_path, pid.to_string())
        .map_err(|e| format!("failed to write PID file: {e}"))?;

    println!("Daemon started (PID: {})", pid);

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let store_path = home.join(".mgpm").join("store");
    let _store = mgpm_store::ContentStore::new(store_path)
        .map_err(|e| format!("failed to open store: {e}"))?;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        if !pid_path.exists() {
            break;
        }
    }

    Ok(())
}

fn cmd_daemon_stop() -> Result<(), String> {
    let pid_path = daemon_pid_path();
    if !pid_path.exists() {
        return Err("daemon is not running (no PID file)".to_string());
    }

    let pid_str = std::fs::read_to_string(&pid_path)
        .map_err(|e| format!("failed to read PID file: {e}"))?;
    let pid: u32 = pid_str
        .trim()
        .parse()
        .map_err(|e| format!("invalid PID in file: {e}"))?;

    if !is_process_running(pid) {
        println!("Daemon is not running (stale PID file)");
        std::fs::remove_file(&pid_path).ok();
        return Ok(());
    }

    #[cfg(unix)]
    {
        let result = std::process::Command::new("kill")
            .arg(pid.to_string())
            .output();
        match result {
            Ok(_) => {
                std::fs::remove_file(&pid_path).ok();
                println!("Daemon stopped (PID: {})", pid);
            }
            Err(e) => return Err(format!("failed to stop daemon: {e}")),
        }
    }
    #[cfg(not(unix))]
    {
        return Err("daemon stop is not supported on this platform".to_string());
    }

    Ok(())
}

fn cmd_daemon_status() -> Result<(), String> {
    let pid_path = daemon_pid_path();
    if !pid_path.exists() {
        println!("Daemon is not running");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_path)
        .map_err(|e| format!("failed to read PID file: {e}"))?;
    let pid: u32 = pid_str
        .trim()
        .parse()
        .map_err(|e| format!("invalid PID in file: {e}"))?;

    if is_process_running(pid) {
        println!("Daemon is running (PID: {})", pid);
    } else {
        println!("Daemon is not running (stale PID: {})", pid);
        std::fs::remove_file(&pid_path).ok();
    }

    Ok(())
}
