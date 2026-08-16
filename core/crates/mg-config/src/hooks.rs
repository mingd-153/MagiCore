//! mg hooks — user-defined pre/post scripts (P2, 21 §9)
//! (Config: `mg.hooks.toml` project-local, fallback `~/.config/megagate/hooks.toml`.
//!  Format: `[hooks.<event>]` = list of shell commands, run in order.
//!  Chính sách: hook fail → command fail; hook không thể bỏ qua security check.)

use anyhow::{bail, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub hooks: HashMap<String, Vec<String>>,
}

impl HooksConfig {
    fn load_from(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(path)?;
        let cfg: HooksConfig = toml::from_str(&raw)?;
        Ok(Some(cfg))
    }
}

/// Default hooks file paths — project-local trước, user-global sau
pub fn hooks_paths(project_root: &Path) -> Vec<PathBuf> {
    let mut v = vec![project_root.join("mg.hooks.toml")];
    if let Ok(home) = std::env::var("HOME") {
        v.push(Path::new(&home).join(".config/megagate/hooks.toml"));
    }
    v
}

/// Merge các file hooks (project override user cho cùng event)
fn load_merged(project_root: &Path) -> Result<HooksConfig> {
    let mut merged = HooksConfig::default();
    for path in hooks_paths(project_root) {
        if let Some(cfg) = HooksConfig::load_from(&path)? {
            for (event, cmds) in cfg.hooks {
                merged.hooks.entry(event).or_default().extend(cmds);
            }
        }
    }
    Ok(merged)
}

/// Chạy tất cả hooks cho event; fail bất kỳ lệnh nào → trả lỗi (chống bypass)
pub fn run_hooks(project_root: &Path, event: &str) -> Result<()> {
    let cfg = load_merged(project_root)?;
    let Some(cmds) = cfg.hooks.get(event) else {
        return Ok(());
    };
    for cmd in cmds {
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(project_root)
            .status()?;
        if !status.success() {
            bail!("hook {event} failed (exit {:?}): {cmd}", status.code());
        }
    }
    Ok(())
}

/// List events đã cấu hình (cho `mg hooks list`)
pub fn list_hooks(project_root: &Path) -> Result<HashMap<String, Vec<String>>> {
    Ok(load_merged(project_root)?.hooks)
}