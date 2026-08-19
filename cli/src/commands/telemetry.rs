//! mg telemetry — opt-in status/log (P2, 18 §4)
//! (Zero-telemetry mặc định; user bật bằng env MEGAGATE_TELEMETRY=1)

use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand, Debug, Clone)]
pub enum TelemetryCmd {
    /// Show telemetry status (enabled/disabled)
    Status,
    /// Show locally queued events (minh bạch — không gửi đi đâu)
    Log,
    /// Enable telemetry (env MEGAGATE_TELEMETRY=1, persist tới ~/.config/megagate/env)
    On,
    /// Disable telemetry (mặc định sẵn)
    Off,
}

pub fn handle(cmd: TelemetryCmd) -> Result<()> {
    let enabled = mg_http::telemetry::enabled();
    match cmd {
        TelemetryCmd::Status => {
            println!(
                "telemetry: {} (MEGAGATE_TELEMETRY={})",
                if enabled { "ON" } else { "OFF (default)" },
                std::env::var("MEGAGATE_TELEMETRY").unwrap_or_default()
            );
            if enabled {
                println!("  queue sends nowhere — writes locally on flush only");
            }
        }
        TelemetryCmd::Log => {
            let dir = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            let path = std::path::Path::new(&dir).join(".megagate/telemetry/events.jsonl");
            if path.exists() {
                println!("{}", path.display());
                let raw = std::fs::read_to_string(&path)?;
                for line in raw.lines() {
                    println!("{line}");
                }
            } else {
                println!("no telemetry events (not enabled or nothing recorded)");
            }
        }
        TelemetryCmd::On | TelemetryCmd::Off => {
            let on = matches!(cmd, TelemetryCmd::On);
            // env persist pick-up: ghi vào profile config để session sau tự nạp
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            let cfg_dir = std::path::Path::new(&home).join(".config/megagate");
            std::fs::create_dir_all(&cfg_dir)?;
            let env_file = cfg_dir.join("env");
            let mut lines: Vec<String> = std::fs::read_to_string(&env_file)
                .map(|s| s.lines().map(str::to_string).collect::<Vec<_>>())
                .unwrap_or_default();
            lines.retain(|l| !l.starts_with("MEGAGATE_TELEMETRY="));
            lines.push(format!("MEGAGATE_TELEMETRY={}", if on { "1" } else { "0" }));
            std::fs::write(&env_file, lines.join("\n") + "\n")?;
            println!(
                "telemetry: {} (add a line to ~/.config/megagate/env — the shell prompt must source it; default OFF if absent)"
                ,
                if on { "ON" } else { "OFF" }
            );
        }
    }
    Ok(())
}
