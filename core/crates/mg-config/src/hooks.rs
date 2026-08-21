//! mg hooks — user-defined pre/post scripts (P2, 21 §9)
//! (Config: `mg.hooks.toml` project-local, fallback `~/.config/megagate/hooks.toml`.
//!  Format: `[hooks.<event>]` = list of shell commands, run in order.
//!  Chính sách: hook fail → command fail; hook không thể bỏ qua security check.)

use anyhow::{bail, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const FORBIDDEN_HOOK_TOOLS: &[&str] = &["npm", "npx", "pnpm", "yarn", "bun", "bunx"];

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
        let argv = parse_hook_command(cmd)?;
        let Some((program, args)) = argv.split_first() else {
            continue;
        };
        reject_forbidden_hook_tool(program)?;
        let status = std::process::Command::new(program)
            .args(args)
            .current_dir(project_root)
            .status()?;
        if !status.success() {
            bail!("hook {event} failed (exit {:?}): {cmd}", status.code());
        }
    }
    Ok(())
}

fn reject_forbidden_hook_tool(program: &str) -> Result<()> {
    let name = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program);
    if FORBIDDEN_HOOK_TOOLS.contains(&name) {
        bail!("hook command '{name}' is forbidden; use MegaGate-native commands instead");
    }
    Ok(())
}

fn parse_hook_command(cmd: &str) -> Result<Vec<String>> {
    if cmd.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = cmd.chars().peekable();
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '\'' | '"') => quote = Some(ch),
            (None, ' ' | '\t' | '\n' | '\r') => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            (None, '&') if matches!(chars.peek(), Some('&')) => {
                bail!("hook command contains shell control operator '&&', which is not allowed")
            }
            (None, '|' | ';' | '<' | '>' | '`' | '$' | '(' | ')') => {
                bail!("hook command contains shell metacharacter '{ch}', which is not allowed")
            }
            (None, c) => current.push(c),
        }
    }
    if quote.is_some() {
        bail!("hook command has an unterminated quote");
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

/// List events đã cấu hình (cho `mg hooks list`)
pub fn list_hooks(project_root: &Path) -> Result<HashMap<String, Vec<String>>> {
    Ok(load_merged(project_root)?.hooks)
}
