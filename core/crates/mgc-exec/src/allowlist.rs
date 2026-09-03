//! Allowlist check — kiểm tra tool trước khi exec (00-index §5.1, §5.2)
//!
//! ## Threat Model (2026-09-02)
//!
//! MagiCore orchestrates package managers, NOT sandboxes them. Two security boundaries:
//!
//! 1. **Install scope (HIGH RISK)**: Package installation, registry fetch, transitive deps
//!    - PM tools (npm/pnpm/yarn/bun) FORBIDDEN → use `mgc install` (resolver + audit)
//!    - Rationale: Prevent arbitrary package fetch bypassing mgc resolver
//!
//! 2. **Test/Build/Dev scopes (MEDIUM RISK)**: Project-local scripts execution
//!    - PM tools ALLOWED with constraints: cwd locked to project root, audit log
//!    - Rationale: package.json scripts are user code, run under user's permission
//!    - mgc doesn't sandbox npm scripts (would require OS-level isolation)
//!
//! ## ExecutionScope
//! Test-runner security model: npm/pnpm/yarn/bun FORBIDDEN for Install scope,
//! but ALLOWED for TestRunner/BuildRunner/DevServer scopes (project-local scripts only).
//! See docs/architecture/TEST_RUNNER_SECURITY_MODEL.md for full threat model.

use anyhow::{bail, Result};
use std::path::Path;

/// Execution scope — determines security policy for tool execution.
/// Install scope: HIGH RISK (arbitrary package fetch, transitive deps).
/// TestRunner/BuildRunner/DevServer: MEDIUM RISK (project-local scripts only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionScope {
    /// HIGH RISK: Package installation, fetch from registry, install scripts.
    /// npm/pnpm/yarn/bun FORBIDDEN in this scope.
    Install,

    /// MEDIUM RISK: Test runner execution (project-local test scripts only).
    /// npm/pnpm/yarn/bun ALLOWED with constraints: cwd locked, audit log, no shell injection.
    TestRunner,

    /// MEDIUM RISK: Build runner execution (project-local build scripts).
    /// npm/pnpm/yarn/bun ALLOWED with constraints: cwd locked, audit log.
    BuildRunner,

    /// MEDIUM RISK: Dev server execution (project-local dev scripts).
    /// npm/pnpm/yarn/bun ALLOWED with constraints: cwd locked, audit log.
    DevServer,
}

/// Scope constraints — security policy per execution scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeConstraints {
    /// Must run in project root (not arbitrary cwd).
    pub cwd_locked: bool,

    /// Network access forbidden (not yet enforced, roadmap).
    pub no_network: bool,

    /// Only predefined args (not yet enforced, roadmap).
    pub no_arbitrary_args: bool,

    /// Must log to audit trail.
    pub audit_log_required: bool,

    /// Validate args for shell injection.
    pub shell_injection_check: bool,
}

impl ExecutionScope {
    /// Get scope constraints for this execution scope.
    pub fn constraints(self) -> ScopeConstraints {
        match self {
            ExecutionScope::Install => ScopeConstraints {
                cwd_locked: false, // Install can run anywhere
                no_network: false, // Install needs network
                no_arbitrary_args: false,
                audit_log_required: true,
                shell_injection_check: true,
            },
            ExecutionScope::TestRunner
            | ExecutionScope::BuildRunner
            | ExecutionScope::DevServer => ScopeConstraints {
                cwd_locked: true,        // Must run in project root
                no_network: false,       // Tests may need network (integration tests)
                no_arbitrary_args: true, // Only predefined commands
                audit_log_required: true,
                shell_injection_check: true,
            },
        }
    }

    /// Check if PM tools (npm/pnpm/yarn/bun) are allowed in this scope.
    pub fn allows_pm_tools(self) -> bool {
        matches!(
            self,
            ExecutionScope::TestRunner | ExecutionScope::BuildRunner | ExecutionScope::DevServer
        )
    }
}

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
    "pytest", // AI test runner
    "uv",
    "go",
    "pub",
    "dart",
    "gradle",
    "mvn",
    "composer",
    "node",
    "deno", // Runtime adapter: Deno projects (optimizer support)
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
    "xcodebuild",
    "echo", // Test tool: prove validator runs before allowlist check
];

/// Tools with mgc resolver coverage — PM tools forbidden in Install scope, allowed in Test/Build/Dev scopes.
/// (npm/npx/pnpm/yarn/bun: Install scope FORBIDDEN → use `mgc install`; Test/Build/Dev scope ALLOWED → project-local scripts)
pub const FORBIDDEN_TOOLS: &[&str] = &["npm", "npx", "pnpm", "yarn", "bun", "bunx"];

/// Kiểm tool trước khi exec: cấm vĩnh viễn → lỗi rõ lý do; ngoài allowlist → lỗi.
/// DEPRECATED: Use check_tool_with_scope for new code (supports ExecutionScope).
pub fn check_tool(name: &str) -> Result<()> {
    check_tool_with_scope(name, ExecutionScope::Install, None)
}

/// Check tool with execution scope — new primary API.
/// PM tools (npm/pnpm/yarn/bun) forbidden in Install scope, allowed in TestRunner/BuildRunner/DevServer.
pub fn check_tool_with_scope(
    name: &str,
    scope: ExecutionScope,
    project_root: Option<&Path>,
) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("tool name is empty");
    }

    let normalized = normalize_script_token(name).unwrap_or_else(|| name.to_ascii_lowercase());

    // PM tools: forbidden in Install scope, allowed in others
    let is_pm_tool = FORBIDDEN_TOOLS.contains(&normalized.as_str());
    if is_pm_tool {
        if scope.allows_pm_tools() {
            // ALLOWED: TestRunner/BuildRunner/DevServer scope
            // Verify constraints
            let constraints = scope.constraints();

            if constraints.cwd_locked {
                // Issue #12: Verify project_root is valid (not /tmp, not parent of workspace)
                // For now, just require it's provided
                if project_root.is_none() {
                    bail!(
                        "tool '{name}' requires project_root in {:?} scope (security constraint: cwd_locked)",
                        scope
                    );
                }
            }

            // Audit log handled by caller (run.rs)
            return Ok(());
        } else {
            // FORBIDDEN: Install scope
            bail!(
                "tool '{name}' is permanently forbidden in {:?} scope (mgc resolver covers its format — use `mgc install` instead)",
                scope
            );
        }
    }

    // Non-PM tools: check against general allowlist
    if !ALLOWED_TOOLS.contains(&normalized.as_str()) {
        bail!(
            "tool '{name}' is not on the allowlist (00-index §5.1) — add it there only after review"
        );
    }

    Ok(())
}

/// Scoped check kept for callers that pass cwd, but PM tools stay forbidden everywhere.
/// Kiểm theo cwd vẫn tồn tại để giữ API ổn định; mọi PM ngoài vẫn bị chặn tuyệt đối.
pub fn check_tool_scoped(name: &str, cwd: Option<&Path>) -> Result<()> {
    let _ = cwd;
    let name = name.trim();
    if name.is_empty() {
        bail!("tool name is empty");
    }
    let normalized = normalize_script_token(name).unwrap_or_else(|| name.to_ascii_lowercase());
    if FORBIDDEN_TOOLS.contains(&normalized.as_str()) {
        bail!(
            "tool '{name}' is permanently forbidden (mgc resolver covers its format — use `mgc install` instead)"
        );
    }
    if !ALLOWED_TOOLS.contains(&normalized.as_str()) {
        bail!(
            "tool '{name}' is not on the allowlist (00-index §5.1) — add it there only after review"
        );
    }
    Ok(())
}

/// Historical detector kept for migration diagnostics only; it no longer grants exec bypass.
/// Bộ nhận diện cũ chỉ còn phục vụ chẩn đoán migration, không mở khóa chạy PM.
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
        let mgc = dir.join("mgc.toml");
        if mgc.is_file() {
            return std::fs::read_to_string(mgc)
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
/// Chặn script vòng ngược qua npm/pnpm/yarn/bun để giữ MagiCore độc lập.
pub fn reject_forbidden_pm_script(script: &str) -> Result<()> {
    if let Some(tool) = find_forbidden_tool_in_script(script) {
        bail!(
            "script delegates to forbidden package manager '{tool}'; use MagiCore-native install/run commands instead"
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
                    "script uses unsupported shell control token '{printable}'; MagiCore runs scripts without a shell in beta"
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
