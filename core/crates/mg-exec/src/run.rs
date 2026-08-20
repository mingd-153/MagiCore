//! Exec runner — chạy tool qua allowlist, args Vec riêng (00-index §5.5–§5.8)
//! (passthrough run: dry-run, audit log, không shell injection, fail → bail kèm log trích)

use crate::allowlist::{check_tool_scoped, FORBIDDEN_TOOLS};
use crate::audit::{append, now_ts, AuditEntry};
use crate::sanitizer::redact_args;
use anyhow::{bail, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

const DEFAULT_EXEC_TIMEOUT_SECS: u64 = 1800;
const EXEC_TIMEOUT_ENV: &str = "MG_EXEC_TIMEOUT_SECS";
const WAIT_POLL_INTERVAL_MS: u64 = 20;

/// Report whether process-tree monitoring is active on this platform.
/// Báo rõ nền tảng hiện tại có guard process-tree thật hay không.
pub fn process_tree_guard_available() -> bool {
    cfg!(unix)
}

/// Tùy chọn chạy — dry_run in lệnh không chạy (00 §5.5); log_path để ghi audit.
#[derive(Debug, Clone, Default)]
pub struct ExecOptions {
    pub dry_run: bool,
    /// Đường dẫn file audit (vd `<project>/.megagate/exec.log`) — None = không ghi.
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

/// Chạy `cmd args` sau khi check allowlist. Không dùng shell — args là Vec riêng (§5.6).
pub fn run(cmd: &str, args: &[String], opts: &ExecOptions) -> Result<ExecReport> {
    check_tool_scoped(cmd, opts.cwd.as_deref())?;
    if opts.clean_env {
        reject_forbidden_script_file(cmd)?;
    }
    execute_command(cmd, args, opts, OutputMode::Capture)
}

/// Run an allowlisted tool while inheriting stdio for interactive/streaming commands.
/// Chạy tool allowlist với stdio trực tiếp cho build/dev mà vẫn giữ guard chung.
pub fn run_inherited(cmd: &str, args: &[String], opts: &ExecOptions) -> Result<ExecReport> {
    check_tool_scoped(cmd, opts.cwd.as_deref())?;
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

fn execute_command(
    cmd: &str,
    args: &[String],
    opts: &ExecOptions,
    mode: OutputMode,
) -> Result<ExecReport> {
    // args REDACTED từ nguồn — console/report/audit không bao giờ chứa secret (§5.4)
    let safe_args = redact_args(args);
    let cwd = opts
        .cwd
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Forbidden PM tools stay blocked in every cwd.
    // PM ngoài bị chặn tuyệt đối, không còn ngoại lệ React Native.
    let scoped_exempt: &[&str] = &[];

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
    let mut command = Command::new(cmd);
    command.args(args).current_dir(&cwd);
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

    if exit_code != 0 {
        // 00 §5.8: exit ≠ 0 → bail kèm log trích
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
        let dir = unique_temp_dir("mg-exec-shadow-path");
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
            "#!/bin/sh\nprintf '%s\\n' \"MegaGate blocked forbidden package manager: {tool}\" >&2\nexit 126\n"
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
            "@echo off\r\necho MegaGate blocked forbidden package manager: {tool} 1>&2\r\nexit /b 126\r\n"
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
        if monitor_forbidden_children {
            if let Some(found) = find_forbidden_descendant(child.id(), exempt) {
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
    let process_group = format!("-{root_pid}");
    let _ = Command::new("kill")
        .args(["-TERM", &process_group])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    std::thread::sleep(Duration::from_millis(WAIT_POLL_INTERVAL_MS));
    let _ = Command::new("kill")
        .args(["-KILL", &process_group])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
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

/// 40 dòng cuối output (đủ để trích lỗi, không tràn log).
fn tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.lines().rev().take(40).collect::<Vec<_>>().join("\n")
}
