//! Exec runner — chạy tool qua allowlist, args Vec riêng (00-index §5.5–§5.8)
//! (passthrough run: dry-run, audit log, không shell injection, fail → bail kèm log trích)

use crate::allowlist::FORBIDDEN_TOOLS;
use crate::audit::{AuditEntry, append, now_ts};
use crate::sanitizer::redact_args;
use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

const DEFAULT_EXEC_TIMEOUT_SECS: u64 = 1800;
const EXEC_TIMEOUT_ENV: &str = "MGC_EXEC_TIMEOUT_SECS";
const WAIT_POLL_INTERVAL_MS: u64 = 20;
/// Max bytes kept per captured stream (stdout/stderr tails) — bounded memory,
/// configurable values live in one place (RULE §12).
/// Giới hạn byte cho mỗi stream captured — bộ nhớ bị chặn, giá trị đổi được
/// tập trung một chỗ (RULE §12).
const MAX_CAPTURE_BYTES: usize = 1024 * 1024;
/// Max lines kept per captured stream — error-log excerpt size.
/// Số dòng giữ tối đa cho mỗi stream — kích thước trích lỗi.
const MAX_CAPTURE_LINES: usize = 40;

/// Report whether process-tree monitoring is active on this platform.
/// Báo rõ nền tảng hiện tại có guard process-tree thật hay không.
pub fn process_tree_guard_available() -> bool {
    cfg!(unix)
}

/// Tùy chọn chạy — dry_run in lệnh không chạy (00 §5.5); log_path để ghi audit.
#[derive(Debug, Clone, Default)]
pub struct ExecOptions {
    pub dry_run: bool,
    /// Đường dẫn file audit (vd `<project>/.magicore/exec.log`) — None = không ghi.
    pub log_path: Option<PathBuf>,
    /// Thư mục làm việc của command — None = thừa kế process cwd.
    pub cwd: Option<PathBuf>,
    /// Timeout tối đa cho tool passthrough — None dùng default/env.
    pub timeout: Option<Duration>,
    /// Env explicit truyền cho child — dùng khi core cần cấu hình tối thiểu.
    pub env: Vec<(String, String)>,
    /// Xóa env thừa trước khi chạy — dùng cho script không tin cậy.
    pub clean_env: bool,
    /// Không áp timeout — dùng cho dev server chạy dài, vẫn giữ guard process.
    pub disable_timeout: bool,
    /// Execution scope (TestRunner/BuildRunner/DevServer allow PM tools, Install forbids them).
    /// None defaults to Install scope (most restrictive).
    pub execution_scope: Option<crate::allowlist::ExecutionScope>,
    /// Exit codes treated as success (escape hatch for scanner tools that
    /// exit non-zero when they find findings — e.g. cargo-audit exits 1 on
    /// vulnerabilities). Empty = any non-zero exit fails (default, fail-closed).
    /// Exit code coi là thành công — scanner tool thường thoát khác 0 khi
    /// tìm thấy finding (vd cargo-audit thoát 1). Rỗng = khác 0 là lỗi
    /// (mặc định, fail-closed).
    pub allowed_exit_codes: Vec<i32>,
}

/// Kết quả chạy — args trong report ĐÃ redact (không lộ secret).
#[derive(Debug, Clone)]
pub struct ExecReport {
    pub cmd: String,
    pub args: Vec<String>,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub dry_run: bool,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

/// Resolve a bare command name to a Windows shim (.cmd/.bat) when the bare
/// executable is not directly spawnable. Uses where.exe (PATH search) so the
/// allowlist still governs WHICH tools can run — this only fixes HOW.
/// Resolve tên lệnh sang shim Windows (.cmd/.bat) khi bare exe không spawn
/// được — dùng where.exe (tìm theo PATH), allowlist vẫn kiểm soát CHỨNG TỪ.
#[cfg(not(unix))]
fn resolve_windows_shim(cmd: &str) -> std::ffi::OsString {
    use std::ffi::OsString;
    use std::os::windows::process::CommandExt;

    // Names that already carry an extension or path separators spawn as-is.
    if cmd.contains('.') || cmd.contains('\\') || cmd.contains('/') {
        return OsString::from(cmd);
    }

    let output = Command::new("where.exe")
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .arg(cmd)
        .output();
    if let Ok(out) = output
        && out.status.success()
    {
        // Prefer real PE executable (.exe, .com) to avoid cmd.exe wrapper
        // which can corrupt environment variables (Node CSPRNG crash on Windows runner).
        // Only use .cmd/.bat if no PE executable available.
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();

        // Priority: .exe > .com > .cmd/.bat > extensionless.
        // Rationale: an extensionless PATH entry on Windows is often a
        // Git-bash sh script (flutter's bin ships both `flutter` sh script
        // and `flutter.bat`) — spawning it yields ERROR_BAD_FORMAT (193).
        // .cmd/.bat must go through cmd.exe; extensionless is last resort.
        // Ưu tiên: .exe > .com > .cmd/.bat > không đuôi. Entry không đuôi
        // trên Windows thường là sh script của Git-bash (flutter bin có cả
        // `flutter` lẫn `flutter.bat`) — spawn trực tiếp sẽ lỗi 193.
        let is_pe = |l: &str| {
            let lower = l.to_ascii_lowercase();
            lower.ends_with(".exe") || lower.ends_with(".com")
        };
        let is_shim = |l: &str| {
            let lower = l.to_ascii_lowercase();
            lower.ends_with(".cmd") || lower.ends_with(".bat")
        };
        if let Some(pe) = lines.iter().find(|l| is_pe(l)) {
            return OsString::from(*pe);
        }
        if let Some(shim) = lines.iter().find(|l| is_shim(l)) {
            return OsString::from(*shim);
        }
        if let Some(direct) = lines.first() {
            return OsString::from(*direct);
        }
    }
    OsString::from(cmd)
}

/// Chạy `cmd args` sau khi check allowlist. Không dùng shell — args là Vec riêng (§5.6).
pub fn run(cmd: &str, args: &[String], opts: &ExecOptions) -> Result<ExecReport> {
    let scope = opts
        .execution_scope
        .unwrap_or(crate::allowlist::ExecutionScope::Install);
    crate::allowlist::check_tool_with_scope(cmd, scope, opts.cwd.as_deref())?;
    if opts.clean_env {
        reject_forbidden_script_file(cmd)?;
    }
    execute_command(cmd, args, opts, OutputMode::Capture)
}

/// Run an allowlisted tool while inheriting stdio for interactive/streaming commands.
/// Chạy tool allowlist với stdio trực tiếp cho build/dev mà vẫn giữ guard chung.
pub fn run_inherited(cmd: &str, args: &[String], opts: &ExecOptions) -> Result<ExecReport> {
    let scope = opts
        .execution_scope
        .unwrap_or(crate::allowlist::ExecutionScope::Install);
    crate::allowlist::check_tool_with_scope(cmd, scope, opts.cwd.as_deref())?;
    if opts.clean_env {
        reject_forbidden_script_file(cmd)?;
    }
    execute_command(cmd, args, opts, OutputMode::Inherit)
}

/// Run a concrete project/package binary path with guardrails but without static tool allowlist.
/// Chạy binary cụ thể đã định vị trong project/cache, vẫn có clean env + blocker + audit.
pub fn run_project_binary(path: &Path, args: &[String], opts: &ExecOptions) -> Result<ExecReport> {
    execute_project_binary(path, args, opts, OutputMode::Capture)
}

/// Run a concrete project/package binary path with inherited stdio.
/// Chạy binary project/cache trực tiếp, dành cho launcher chạy dài hoặc output realtime.
pub fn run_project_binary_inherited(
    path: &Path,
    args: &[String],
    opts: &ExecOptions,
) -> Result<ExecReport> {
    execute_project_binary(path, args, opts, OutputMode::Inherit)
}

fn execute_project_binary(
    path: &Path,
    args: &[String],
    opts: &ExecOptions,
    mode: OutputMode,
) -> Result<ExecReport> {
    let canonical = path.canonicalize().map_err(|e| {
        anyhow::anyhow!("project binary '{}' is not executable: {e}", path.display())
    })?;
    if !canonical.is_file() {
        bail!("project binary '{}' is not a file", canonical.display());
    }

    let basename = process_basename(&canonical.display().to_string());
    if FORBIDDEN_TOOLS.contains(&basename.as_str()) {
        bail!(
            "project binary '{}' resolves to forbidden package manager '{}'",
            canonical.display(),
            basename
        );
    }

    if opts.clean_env {
        reject_forbidden_script_file(&canonical.display().to_string())?;
    }
    execute_command(&canonical.display().to_string(), args, opts, mode)
}

#[derive(Debug, Clone, Copy)]
enum OutputMode {
    Capture,
    Inherit,
}

/// Validate args không chứa path traversal patterns
/// Validator path-aware: canonicalize paths relative to project_root, chặn escape
/// Returns Err nếu phát hiện path escape khỏi project boundary
fn validate_args_no_traversal(args: &[String], project_root: Option<&Path>) -> Result<()> {
    // Nếu không có project_root, không thể validate paths → fail-closed
    let root = match project_root {
        Some(r) => r,
        None => {
            // Không có project_root → không thể verify path safety
            // Chặn mọi arg có ".." hoặc absolute path (conservative)
            for arg in args {
                if arg.contains("..") || arg.starts_with('/') {
                    #[cfg(windows)]
                    if arg.len() >= 2 && arg.chars().nth(1) == Some(':') {
                        bail!(
                            "Path argument rejected without project_root context: '{}'\n\
                            Cannot verify path safety without project boundary.",
                            arg
                        );
                    }
                    #[cfg(unix)]
                    bail!(
                        "Path argument rejected without project_root context: '{}'\n\
                        Cannot verify path safety without project boundary.",
                        arg
                    );
                }
            }
            return Ok(());
        }
    };

    // Canonicalize project root once
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize project root: {:?}", root))?;

    for arg in args {
        // Heuristic: arg là path nếu chứa path separator hoặc bắt đầu với . hoặc /
        // Args khác (versions "1.0..2", flags "--option") không validate
        let looks_like_path =
            arg.contains('/') || arg.contains('\\') || arg.starts_with('.') || arg.starts_with('/');

        #[cfg(windows)]
        let looks_like_path =
            looks_like_path || (arg.len() >= 2 && arg.chars().nth(1) == Some(':'));

        if !looks_like_path {
            // Không phải path → skip (cho phép "1.0..2", "config..backup")
            continue;
        }

        // Thử resolve path relative to project root
        let resolved = if arg.starts_with('/') {
            #[cfg(unix)]
            {
                // Absolute path Unix
                PathBuf::from(arg)
            }
            #[cfg(windows)]
            {
                // Không bao giờ đến đây trên Windows (checked bên dưới)
                PathBuf::from(arg)
            }
        } else {
            // Relative path
            canonical_root.join(arg)
        };

        // Canonicalize để resolve .. và symlinks
        match resolved.canonicalize() {
            Ok(canonical_arg) => {
                // Check nếu canonical path nằm trong project root
                if !canonical_arg.starts_with(&canonical_root) {
                    bail!(
                        "Path traversal detected: '{}' resolves to '{}' which is outside project root '{}'",
                        arg,
                        canonical_arg.display(),
                        canonical_root.display()
                    );
                }
            }
            Err(_) => {
                // File không tồn tại → không thể canonicalize
                // Kiểm tra lexical: nếu có .. và có thể escape, reject
                if arg.contains("..") {
                    // Parse path và check số lượng .. có thể escape không
                    let components: Vec<_> = Path::new(arg).components().collect();
                    let mut depth = 0i32;
                    for comp in components {
                        match comp {
                            std::path::Component::ParentDir => depth -= 1,
                            std::path::Component::Normal(_) => depth += 1,
                            _ => {}
                        }
                        if depth < 0 {
                            bail!(
                                "Path traversal pattern detected: '{}' attempts to escape project root.\n\
                                Path contains '..' that would traverse above project boundary.",
                                arg
                            );
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn execute_command(
    cmd: &str,
    args: &[String],
    opts: &ExecOptions,
    mode: OutputMode,
) -> Result<ExecReport> {
    // SECURITY: Validate args không chứa path traversal
    // Pass project_root (from cwd) để validator có context
    let project_root = opts.cwd.as_deref();
    validate_args_no_traversal(args, project_root)?;

    // args REDACTED từ nguồn — console/report/audit không bao giờ chứa secret (§5.4)
    let safe_args = redact_args(args);
    let cwd = opts
        .cwd
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Forbidden PM tools stay blocked in every cwd.
    // PM ngoài bị chặn tuyệt đối, không còn ngoại lệ React Native.
    // Scoped exempt: PM tools allowed in TestRunner/BuildRunner/DevServer scopes (00-index §5.2 + TEST_RUNNER_SECURITY_MODEL.md)
    // PM tools allowed in TestRunner/BuildRunner/DevServer: không tạo blocker shim
    let scope = opts
        .execution_scope
        .unwrap_or(crate::allowlist::ExecutionScope::Install);
    let scoped_exempt: &[&str] = if scope.allows_pm_tools() {
        crate::allowlist::FORBIDDEN_TOOLS
    } else {
        &[]
    };

    if opts.dry_run {
        // dry-run: in lệnh, không chạy, vẫn ghi audit với exit_code 0 + dry_run flag (§5.5)
        println!("[dry-run] {} {}", cmd, safe_args.join(" "));
        let report = ExecReport {
            cmd: cmd.to_string(),
            args: safe_args,
            exit_code: 0,
            duration_ms: 0,
            dry_run: true,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
        };
        if let Some(path) = &opts.log_path {
            append(path, &entry_from(&report, &cwd))?;
        }
        return Ok(report);
    }

    let start = Instant::now();
    // Windows: rust std Command does NOT resolve .cmd/.bat shims through PATH
    // (implicit extension execution was removed for security). npm-style tools
    // (flutter.bat, tsc.cmd...) ship as shims, so resolve them explicitly via
    // where.exe — keep it allowlist-safe: same bare name, resolved by PATH.
    // Windows: std Command không tự chạy .cmd/.bat qua PATH (bảo mật) — tool
    // kiểu npm/flutter là shim .bat, nên resolve tường minh qua where.exe.
    #[cfg(not(unix))]
    let resolved_cmd = resolve_windows_shim(cmd);
    #[cfg(unix)]
    let resolved_cmd: &str = cmd;

    // Windows: If resolved_cmd is .cmd/.bat, spawn via cmd.exe to avoid "not a valid Win32 application"
    #[cfg(not(unix))]
    let mut command = {
        let resolved_str = resolved_cmd.to_string_lossy();
        let is_script = resolved_str.to_ascii_lowercase().ends_with(".cmd")
            || resolved_str.to_ascii_lowercase().ends_with(".bat");

        if is_script {
            // Spawn via cmd.exe /D /S /C "script.bat" args...
            let mut cmd_exe = Command::new("cmd.exe");
            cmd_exe.arg("/D").arg("/S").arg("/C");
            // Pass the raw path as one argument. Command performs the Windows
            // argument quoting; pre-quoting here turns quotes into literal
            // backslash-escaped characters for cmd.exe.
            // Truyền path thô thành một arg. Command tự quote Windows; quote
            // trước ở đây biến quote thành ký tự literal cho cmd.exe.
            // `canonicalize()` may yield an extended-length `\\?\C:\...` path.
            // That path is valid for Win32 APIs but cmd.exe does not accept it.
            // Keep canonical paths for validation, but normalize only the value
            // handed to the command interpreter.
            // `canonicalize()` có thể trả về `\\?\C:\...`; Win32 chấp nhận nhưng
            // cmd.exe không chấp nhận. Chỉ normalize khi giao cho cmd.exe.
            let cmd_path = if let Some(unc) = resolved_str.strip_prefix(r"\\?\UNC\") {
                format!(r"\\{unc}")
            } else if let Some(local) = resolved_str.strip_prefix(r"\\?\") {
                local.to_string()
            } else {
                resolved_str.into_owned()
            };
            cmd_exe.arg(cmd_path);
            cmd_exe.args(args);
            cmd_exe.current_dir(&cwd);
            cmd_exe
        } else {
            let mut cmd = Command::new(&resolved_cmd); // Borrow instead of move
            cmd.args(args).current_dir(&cwd);
            cmd
        }
    };

    #[cfg(unix)]
    let mut command = {
        let mut cmd = Command::new(resolved_cmd);
        cmd.args(args).current_dir(&cwd);
        cmd
    };

    match mode {
        OutputMode::Capture => {
            command
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
        }
        OutputMode::Inherit => {
            command
                .stdin(std::process::Stdio::inherit())
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit());
        }
    }
    if opts.clean_env {
        configure_process_isolation(&mut command);
    }

    let _shadow_path = if opts.clean_env {
        let shadow_path = ShadowPath::create(scoped_exempt)?;
        let path_env = guarded_path_env(shadow_path.path(), &opts.env)?;
        command.env_clear();
        for (key, value) in &opts.env {
            if key != "PATH" {
                command.env(key, value);
            }
        }
        command.env("PATH", path_env);
        // Windows: this must be after env_clear(), otherwise these values are
        // removed and Node's CSPRNG initialization can abort.
        // Windows: phải đặt sau env_clear(), nếu không Node CSPRNG sẽ crash.
        #[cfg(not(unix))]
        for critical_var in [
            "SYSTEMROOT",
            "WINDIR",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "ProgramData",
        ] {
            if let Ok(val) = std::env::var(critical_var) {
                command.env(critical_var, val);
            }
        }
        Some(shadow_path)
    } else {
        command.envs(opts.env.iter().map(|(key, value)| (key, value)));
        None
    };

    let child = command
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn '{cmd}': {e}"))?;

    let timeout = if opts.disable_timeout {
        None
    } else {
        Some(opts.timeout.unwrap_or_else(default_timeout))
    };
    let outcome = wait_with_timeout(child, timeout, opts.clean_env, scoped_exempt, mode)?;
    let duration_ms = start.elapsed().as_millis() as u64;
    let exit_code = outcome.status.code().unwrap_or(-1);

    let report = ExecReport {
        cmd: cmd.to_string(),
        args: safe_args,
        exit_code,
        duration_ms,
        dry_run: false,
        stdout_tail: tail(&outcome.stdout),
        stderr_tail: tail(&outcome.stderr),
    };
    if let Some(path) = &opts.log_path {
        append(path, &entry_from(&report, &cwd))?;
    }

    if exit_code != 0 && !opts.allowed_exit_codes.contains(&exit_code) {
        // 00 §5.8: exit ≠ 0 → bail kèm log trích (trừ exit code được khai
        // báo trước trong allowed_exit_codes — escape hatch tường minh).
        // 00 §5.8: exit ≠ 0 → bail kèm log excerpt (except exit codes
        // declared upfront in allowed_exit_codes — explicit escape hatch).
        let err_tail = if report.stderr_tail.is_empty() {
            report.stdout_tail.clone()
        } else {
            report.stderr_tail.clone()
        };
        bail!(
            "'{cmd}' exited with code {exit_code} ({} ms)\n--- tail ---\n{}",
            duration_ms,
            err_tail
        );
    }
    Ok(report)
}

struct ShadowPath {
    dir: PathBuf,
}

impl ShadowPath {
    fn create(scoped_exempt: &[&str]) -> Result<Self> {
        let dir = unique_temp_dir("mgc-exec-shadow-path");
        std::fs::create_dir_all(&dir)?;

        for tool in FORBIDDEN_TOOLS {
            if !scoped_exempt.contains(tool) {
                write_blocker(&dir, tool)?;
            }
        }

        Ok(Self { dir })
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for ShadowPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}

#[cfg(unix)]
fn write_blocker(dir: &Path, tool: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(tool);
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"MagiCore blocked forbidden package manager: {tool}\" >&2\nexit 126\n"
        ),
    )?;
    let mut permissions = std::fs::metadata(&path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions)?;
    Ok(())
}

#[cfg(windows)]
fn write_blocker(dir: &Path, tool: &str) -> Result<()> {
    let path = dir.join(format!("{tool}.cmd"));
    std::fs::write(
        &path,
        format!(
            "@echo off\r\necho MagiCore blocked forbidden package manager: {tool} 1>&2\r\nexit /b 126\r\n"
        ),
    )?;
    Ok(())
}

fn guarded_path_env(shadow_dir: &Path, explicit_env: &[(String, String)]) -> Result<OsString> {
    let explicit_path = explicit_env
        .iter()
        .rev()
        .find(|(key, _)| key == "PATH")
        .map(|(_, value)| OsString::from(value));
    let base_path = explicit_path.or_else(|| std::env::var_os("PATH"));
    let mut paths = Vec::new();
    paths.push(shadow_dir.to_path_buf());
    if let Some(path) = base_path {
        paths.extend(std::env::split_paths(&path));
    }
    Ok(std::env::join_paths(paths)?)
}

fn reject_forbidden_script_file(cmd: &str) -> Result<()> {
    let path = Path::new(cmd);
    if !path.is_file() {
        return Ok(());
    }

    let Ok(bytes) = std::fs::read(path) else {
        return Ok(());
    };
    if !bytes.starts_with(b"#!") {
        return Ok(());
    }

    let content = String::from_utf8_lossy(&bytes);
    if let Some(tool) = forbidden_process_name("", &content, &[]) {
        bail!(
            "script '{}' references forbidden package manager '{}'",
            path.display(),
            tool
        );
    }
    Ok(())
}

fn default_timeout() -> Duration {
    std::env::var(EXEC_TIMEOUT_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(DEFAULT_EXEC_TIMEOUT_SECS))
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Option<Duration>,
    monitor_forbidden_children: bool,
    exempt: &[&str],
    mode: OutputMode,
) -> Result<ExecOutcome> {
    let started = Instant::now();

    loop {
        if let Some(status) = child.try_wait()? {
            return finish_child(child, status, mode);
        }
        // Monitor + forbidden child in one guard — gộp điều kiện theo clippy 1.98.
        if monitor_forbidden_children
            && let Some(found) = find_forbidden_descendant(child.id(), exempt)
        {
            terminate_process_tree(child.id());
            let _ = child.kill();
            let out = finish_after_forced_exit(child, mode)?;
            return Err(forbidden_child_error(
                &found,
                out.status,
                &out.stderr,
                &out.stdout,
            ));
        }
        if timeout.is_some_and(|timeout| started.elapsed() >= timeout) {
            terminate_process_tree(child.id());
            let _ = child.kill();
            let out = finish_after_forced_exit(child, mode)?;
            return Err(timeout_error(
                timeout.expect("timeout checked above"),
                out.status,
                &out.stderr,
                &out.stdout,
            ));
        }
        std::thread::sleep(Duration::from_millis(WAIT_POLL_INTERVAL_MS));
    }
}

struct ExecOutcome {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn finish_child(
    child: std::process::Child,
    status: ExitStatus,
    mode: OutputMode,
) -> Result<ExecOutcome> {
    match mode {
        OutputMode::Capture => {
            let output = child.wait_with_output()?;
            Ok(ExecOutcome {
                status: output.status,
                stdout: output.stdout,
                stderr: output.stderr,
            })
        }
        OutputMode::Inherit => Ok(ExecOutcome {
            status,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }),
    }
}

fn finish_after_forced_exit(child: std::process::Child, mode: OutputMode) -> Result<ExecOutcome> {
    match mode {
        OutputMode::Capture => {
            let output = child.wait_with_output()?;
            Ok(ExecOutcome {
                status: output.status,
                stdout: output.stdout,
                stderr: output.stderr,
            })
        }
        OutputMode::Inherit => {
            let output = child.wait_with_output()?;
            Ok(ExecOutcome {
                status: output.status,
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }
    }
}

#[cfg(unix)]
fn configure_process_isolation(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_isolation(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_tree(root_pid: u32) {
    // Không spawn `kill -TERM -{pgid}`: trên GH Runner pgid resolver đánh
    // trúng process group của job → SIGTERM toàn job (exit 143/canceled).
    // Walk /proc theo ppid và giết từng pid cụ thể — không đụng ngoài cây.
    let mut frontier = vec![root_pid];
    let mut all = Vec::new();
    while let Some(pid) = frontier.pop() {
        all.push(pid);
        frontier.extend(child_pids(pid));
    }
    all.reverse();
    for &pid in &all {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    std::thread::sleep(Duration::from_millis(WAIT_POLL_INTERVAL_MS));
    for &pid in &all {
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[cfg(unix)]
fn child_pids(ppid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return out;
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.bytes().any(|b| !b.is_ascii_digit()) {
            continue;
        }
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{name}/stat")) else {
            continue;
        };
        let Some((_, rest)) = stat.split_once(')') else {
            continue;
        };
        let mut fields = rest.split_whitespace();
        let _state = fields.next();
        let Some(ppid_field) = fields.next().and_then(|f| f.parse::<i32>().ok()) else {
            continue;
        };
        // Gộp let-chain theo clippy 1.98 — collapsed guard reads cleaner.
        if ppid_field == ppid as i32
            && let Ok(pid) = name.parse::<u32>()
        {
            out.push(pid);
        }
    }
    out
}

#[cfg(not(unix))]
fn terminate_process_tree(_root_pid: u32) {}

#[derive(Debug, Clone)]
struct ForbiddenProcess {
    pid: u32,
    name: String,
}

#[cfg(unix)]
fn find_forbidden_descendant(root_pid: u32, exempt: &[&str]) -> Option<ForbiddenProcess> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,comm=,command="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let mut processes = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut parts = line.split_whitespace();
        let pid = parts.next()?.parse::<u32>().ok()?;
        let ppid = parts.next()?.parse::<u32>().ok()?;
        let command = parts.next().unwrap_or_default().to_string();
        let command_line = parts.collect::<Vec<_>>().join(" ");
        processes.push((pid, ppid, command, command_line));
    }

    let mut frontier = vec![root_pid];
    let mut seen = std::collections::HashSet::new();
    while let Some(parent) = frontier.pop() {
        if !seen.insert(parent) {
            continue;
        }

        for (pid, _ppid, command, command_line) in
            processes.iter().filter(|(_, ppid, _, _)| *ppid == parent)
        {
            if let Some(name) = forbidden_process_name(command, command_line, exempt) {
                return Some(ForbiddenProcess { pid: *pid, name });
            }
            frontier.push(*pid);
        }
    }

    None
}

#[cfg(not(unix))]
fn find_forbidden_descendant(_root_pid: u32, _exempt: &[&str]) -> Option<ForbiddenProcess> {
    None
}

fn process_basename(command: &str) -> String {
    let command = command.trim_matches(|ch| matches!(ch, '"' | '\'' | '`'));
    let base = command
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(command)
        .to_ascii_lowercase();
    base.strip_suffix(".cmd")
        .or_else(|| base.strip_suffix(".exe"))
        .or_else(|| base.strip_suffix(".ps1"))
        .unwrap_or(&base)
        .to_string()
}

fn forbidden_process_name(command: &str, command_line: &str, exempt: &[&str]) -> Option<String> {
    let exempt = |name: &str| exempt.contains(&name);
    let process_name = process_basename(command);
    if FORBIDDEN_TOOLS.contains(&process_name.as_str()) && !exempt(&process_name) {
        return Some(process_name);
    }

    command_line
        .split_whitespace()
        .map(process_basename)
        .find(|name| FORBIDDEN_TOOLS.contains(&name.as_str()) && !exempt(name))
}

fn forbidden_child_error(
    found: &ForbiddenProcess,
    status: ExitStatus,
    stderr: &[u8],
    stdout: &[u8],
) -> anyhow::Error {
    let err_tail = tail(stderr);
    let out_tail = tail(stdout);
    let output_tail = if err_tail.is_empty() {
        out_tail
    } else {
        err_tail
    };
    anyhow::anyhow!(
        "forbidden package manager '{}' spawned in child process {} (status after kill: {})\n--- tail ---\n{}",
        found.name,
        found.pid,
        status,
        output_tail
    )
}

fn timeout_error(
    timeout: Duration,
    status: ExitStatus,
    stderr: &[u8],
    stdout: &[u8],
) -> anyhow::Error {
    let err_tail = tail(stderr);
    let out_tail = tail(stdout);
    let output_tail = if err_tail.is_empty() {
        out_tail
    } else {
        err_tail
    };
    anyhow::anyhow!(
        "command timed out after {}s (status after kill: {})\n--- tail ---\n{}",
        timeout.as_secs(),
        status,
        output_tail
    )
}

/// AuditEntry từ report — args đi qua sanitizer REDACTED trước khi ghi (§5.4).
fn entry_from(report: &ExecReport, cwd: &Path) -> AuditEntry {
    AuditEntry {
        cmd: report.cmd.clone(),
        args: redact_args(&report.args),
        cwd: cwd.display().to_string(),
        exit_code: report.exit_code,
        duration_ms: report.duration_ms,
        dry_run: report.dry_run,
        ts: now_ts(),
    }
}

/// Bounded tail of captured output — last MAX_CAPTURE_BYTES bytes, line order
/// PRESERVED (reversing lines corrupted multi-line payloads like JSON).
/// Tail có giới hạn byte — MAX_CAPTURE_BYTES cuối, giữ nguyên thứ tự dòng
/// (đảo dòng làm hỏng payload nhiều dòng như JSON).
fn tail(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(MAX_CAPTURE_BYTES);
    let slice = &bytes[start..];
    let text = String::from_utf8_lossy(slice);
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.len() > MAX_CAPTURE_LINES {
        lines = lines.split_off(lines.len() - MAX_CAPTURE_LINES);
    }
    lines.join("\n")
}
