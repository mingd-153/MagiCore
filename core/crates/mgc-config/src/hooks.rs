//! mgc hooks — user-defined pre/post scripts (P2, 21 §9)
//! (Config: `mgc.hooks.toml` project-local, fallback `~/.config/magicore/hooks.toml`.
//!  Format: `[hooks.<event>]` = list of shell commands, run in order.
//!  Chính sách: hook fail → command fail; hook không thể bỏ qua security check.)

use anyhow::{Result, bail};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const FORBIDDEN_HOOK_TOOLS: &[&str] = &["npm", "npx", "pnpm", "yarn", "bun", "bunx", "deno"];

/// Toolchains that must never run on dependency-lifecycle hook events:
/// a hook program is user config, but on a dependency event it would
/// bypass the C0 ownership firewall through the hooks lane (T0.3/B4).
/// Non-dependency events keep the rival-only list above.
/// (Toolchain không bao giờ chạy trên hook event dependency: chương trình
/// hook là config của user, nhưng trên event dependency nó sẽ vòng qua
/// tường lửa C0. Event khác giữ danh sách rival-only.)
const DEPENDENCY_EVENT_TOOLS: &[&str] = &[
    "npm",
    "npx",
    "pnpm",
    "yarn",
    "bun",
    "bunx",
    "deno",
    "cargo",
    "uv",
    "pip",
    "pip3",
    "go",
    "flutter",
    "dart",
    "gradle",
    "mvn",
    "dotnet",
    "swift",
    "pod",
    "xcodebuild",
    "terraform",
    "pio",
    "platformio",
    "west",
];

/// True for hook events wired into the dependency lifecycle
/// (pre/post-install/add/remove/update/publish) — hook programs there run
/// with dependency-operation privilege and must pass the toolchain gate.
/// (True cho hook event thuộc lifecycle dependency — chương trình hook ở
/// đó chạy với đặc quyền dependency-op và phải qua cổng toolchain.)
fn is_dependency_event(event: &str) -> bool {
    let verb = event
        .strip_prefix("pre-")
        .or_else(|| event.strip_prefix("post-"))
        .unwrap_or(event);
    matches!(verb, "install" | "add" | "remove" | "update" | "publish")
}
// `deno` rides along: rival JS runtimes must never execute on dependency
// events, matching the Install-scope guard in mgc-exec (allowlist.rs).
// (Kèm `deno`: runtime JS đối thủ không bao giờ chạy trên event
// dependency, khớp cổng Install-scope trong mgc-exec.)

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
    let mut v = vec![project_root.join("mgc.hooks.toml")];
    if let Ok(home) = std::env::var("HOME") {
        v.push(Path::new(&home).join(".config/magicore/hooks.toml"));
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
        reject_forbidden_hook_tool(program, event)?;
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

fn reject_forbidden_hook_tool(program: &str, event: &str) -> Result<()> {
    let name = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program);
    let denied = if is_dependency_event(event) {
        DEPENDENCY_EVENT_TOOLS
    } else {
        FORBIDDEN_HOOK_TOOLS
    };
    if denied.contains(&name) {
        bail!(
            "hook command '{name}' is forbidden on '{event}'; use MagiCore-native commands instead"
        );
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

/// List events đã cấu hình (cho `mgc hooks list`)
pub fn list_hooks(project_root: &Path) -> Result<HashMap<String, Vec<String>>> {
    Ok(load_merged(project_root)?.hooks)
}
