//! Allowlist check — kiểm tra tool trước khi exec (00-index §5.1, §5.2)
//! (Exec passthrough allowlist: 00-index §5.1 allowlist bất biến + §5.2 cấm vĩnh viễn)

use anyhow::{bail, Result};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Tools được phép passthrough — allowlist bất biến (00-index §5.1).
/// Mỗi core khai báo subset; thêm tool phải review + ghi lý do.
pub const ALLOWED_TOOLS: &[&str] = &[
    "pip",
    "python3",
    "uv",
    "go",
    "pub",
    "dart",
    "gradle",
    "mvn",
    "composer",
    "node",
    "swift",
    "cargo",
    "espflash",
    "west",
    "pio",
    "platformio",
    "terraform",
    "tofu",
    "cdk",
    "pulumi",
    "aws",
    "wrangler",
    "gcloud",
    "gh",
    "git",
    "docker",
    "godot",
    "flutter",
    "kotlinc",
    "python",
    "unity",
    "upm",
];

/// Tools cấm vĩnh viễn — format có resolver mg (00-index §5.2) nên wrapper bị cấm.
/// (npm/npx/pnpm/yarn/bun cấm mọi core — gọi mg install thay vì npm)
pub const FORBIDDEN_TOOLS: &[&str] = &["npm", "npx", "pnpm", "yarn", "bun", "bunx"];

/// Kiểm tool trước khi exec: cấm vĩnh viễn → lỗi rõ lý do; ngoài allowlist → lỗi.
pub fn check_tool(name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("tool name is empty");
    }
    let normalized = normalize_script_token(name).unwrap_or_else(|| name.to_ascii_lowercase());
    if FORBIDDEN_TOOLS.contains(&normalized.as_str()) {
        bail!(
            "tool '{name}' is permanently forbidden (mg resolver covers its format — use `mg install` instead)"
        );
    }
    if !ALLOWED_TOOLS.contains(&normalized.as_str()) {
        bail!(
            "tool '{name}' is not on the allowlist (00-index §5.1) — add it there only after review"
        );
    }
    Ok(())
}

/// C9: npm/npx chỉ được phép trong react-native subdir của project multi
/// (package.json có dependency react-native + cha có mg.toml `[app] language = "multi"`).
/// Ngoài phạm vi đó, npm vẫn bị cấm như §5.2.
pub fn check_tool_scoped(name: &str, cwd: Option<&Path>) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("tool name is empty");
    }
    let normalized = normalize_script_token(name).unwrap_or_else(|| name.to_ascii_lowercase());
    if FORBIDDEN_TOOLS.contains(&normalized.as_str()) {
        if is_react_native_subdir(cwd) {
            return Ok(());
        }
        bail!(
            "tool '{name}' is forbidden outside react-native subdirs (C9 scoped exception — run inside <project>/react-native)"
        );
    }
    if !ALLOWED_TOOLS.contains(&normalized.as_str()) {
        bail!(
            "tool '{name}' is not on the allowlist (00-index §5.1) — add it there only after review"
        );
    }
    Ok(())
}

/// True khi cwd (hoặc cha gần nhất có mg.toml) là react-native subdir của project app multi.
pub fn is_react_native_subdir(cwd: Option<&Path>) -> bool {
    let Some(cwd) = cwd else { return false };
    let rn_ok = cwd.join("package.json").is_file()
        && std::fs::read_to_string(cwd.join("package.json"))
            .map(|s| s.contains("\"react-native\""))
            .unwrap_or(false);
    if !rn_ok {
        return false;
    }
    let mut current = cwd.parent();
    while let Some(dir) = current {
        let mg = dir.join("mg.toml");
        if mg.is_file() {
            return std::fs::read_to_string(mg)
                .map(|s| s.contains("language = \"multi\""))
                .unwrap_or(false);
        }
        current = dir.parent();
    }
    false
}

/// Find forbidden package-manager tools anywhere in a shell-ish script.
/// Tìm tool PM bị cấm trong toàn bộ script, không chỉ token đầu tiên.
pub fn find_forbidden_tool_in_script(script: &str) -> Option<&'static str> {
    script
        .split(|c: char| c.is_whitespace() || matches!(c, ';' | '&' | '|' | '(' | ')' | '<' | '>'))
        .filter_map(normalize_script_token)
        .find_map(|token| {
            FORBIDDEN_TOOLS
                .iter()
                .copied()
                .find(|forbidden| token == *forbidden)
        })
}

/// Reject scripts that attempt to delegate back into external PMs.
/// Chặn script vòng ngược qua npm/pnpm/yarn/bun để giữ MegaGate độc lập.
pub fn reject_forbidden_pm_script(script: &str) -> Result<()> {
    if let Some(tool) = find_forbidden_tool_in_script(script) {
        bail!(
            "script delegates to forbidden package manager '{tool}'; use MegaGate-native install/run commands instead"
        );
    }
    Ok(())
}

/// Parse a simple single-command script without invoking a shell.
/// Parse script đơn lệnh để tránh shell injection trong lifecycle/task runner.
pub fn parse_simple_script(script: &str) -> Result<(String, Vec<String>)> {
    let invocation = parse_script_invocation(script)?;
    Ok((invocation.program, invocation.args))
}

/// Parse one command with optional leading KEY=value env assignments.
/// Parse lệnh đơn kèm env đầu dòng để hỗ trợ script thật mà vẫn không cần shell.
pub fn parse_script_invocation(script: &str) -> Result<ScriptInvocation> {
    reject_shell_control(script)?;
    let tokens = split_simple_words(script)?;
    let program_index = tokens
        .iter()
        .position(|token| !is_env_assignment(token))
        .ok_or_else(|| anyhow::anyhow!("script is missing a program"))?;

    let mut env = Vec::with_capacity(program_index);
    for token in &tokens[..program_index] {
        let (key, value) = parse_env_assignment(token)?;
        env.push((key.to_string(), value.to_string()));
    }

    let Some((program, args)) = tokens[program_index..].split_first() else {
        bail!("script is empty");
    };
    Ok(ScriptInvocation {
        program: program.clone(),
        args: args.to_vec(),
        env,
    })
}

/// Reject shell control characters that would require `sh -c` semantics.
/// Chặn chaining/redirection/subshell để beta chạy fail-closed.
pub fn reject_shell_control(script: &str) -> Result<()> {
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in script.chars() {
        if escaped {
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            ch if ch.is_control()
                || matches!(ch, ';' | '&' | '|' | '<' | '>' | '(' | ')' | '$' | '`') =>
            {
                let printable = match ch {
                    '\n' => "\\n".to_string(),
                    '\r' => "\\r".to_string(),
                    '\t' => "\\t".to_string(),
                    other => other.to_string(),
                };
                bail!(
                    "script uses unsupported shell control token '{printable}'; MegaGate runs scripts without a shell in beta"
                );
            }
            _ => {}
        }
    }
    Ok(())
}

fn normalize_script_token(token: &str) -> Option<String> {
    let trimmed = token
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '`'))
        .trim();
    if trimmed.is_empty() {
        return None;
    }
    let base = trimmed
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    let base = base
        .strip_suffix(".cmd")
        .or_else(|| base.strip_suffix(".exe"))
        .or_else(|| base.strip_suffix(".ps1"))
        .unwrap_or(&base)
        .to_string();
    Some(base)
}

fn is_env_assignment(token: &str) -> bool {
    token
        .split_once('=')
        .is_some_and(|(key, _)| is_valid_env_key(key))
}

fn parse_env_assignment(token: &str) -> Result<(&str, &str)> {
    let Some((key, value)) = token.split_once('=') else {
        bail!("invalid env assignment");
    };
    if !is_valid_env_key(key) {
        bail!("invalid env assignment key '{key}'");
    }
    Ok((key, value))
}

fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn split_simple_words(script: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in script.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            ch if ch.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            other => current.push(other),
        }
    }

    if escaped {
        bail!("script ends with an unfinished escape");
    }
    if quote.is_some() {
        bail!("script contains an unterminated quote");
    }
    if !current.is_empty() {
        words.push(current);
    }
    Ok(words)
}
