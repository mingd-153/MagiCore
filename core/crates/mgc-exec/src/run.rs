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
/// Grace window between the SIGTERM and SIGKILL sweep over the process
/// group — long enough for a well-behaved tool to flush, short enough to
/// keep `timeout + grace` far below any caller-visible wall-clock budget.
/// (Cửa nghiêng giữa SIGTERM và SIGKILL trên process group — đủ cho tool
/// tử tế kịp flush, đủ ngắn để timeout + grace luôn thấp hơn mọi ngân
/// sách wall-clock của caller.)
const TERM_TO_KILL_GRACE_MS: u64 = 100;
/// Deadline for draining the output pipes AFTER the tree kill. Once the
/// group is SIGKILLed every write end closes and EOF arrives in
/// milliseconds; the deadline only exists so a tool that ESCAPED the group
/// (setsid) can never hang us past the timeout again (P0-3, 2026-09-15).
/// (Deadline tiêu pipe SAU khi kill cây. Group bị SIGKILL thì mọi đầu ghi
/// đóng, EOF tới trong vài ms; deadline tồn tại để tool THOÁT khỏi group
/// (setsid) không thể treo ta quá hạn timeout lần nữa.)
const POST_KILL_DRAIN_MS: u64 = 200;
/// Max bytes kept per captured stream (stdout/stderr tails) — bounded memory,
/// configurable values live in one place (RULE §12).
/// Giới hạn byte cho mỗi stream captured — bộ nhớ bị chặn, giá trị đổi được
/// tập trung một chỗ (RULE §12).
const MAX_CAPTURE_BYTES: usize = 1024 * 1024;
/// Max lines kept per captured stream — error-log excerpt size.
/// Số dòng giữ tối đa cho mỗi stream — kích thước trích lỗi.
const MAX_CAPTURE_LINES: usize = 40;

/// Report whether process-tree kill is active on this platform.
/// Unix: the child runs in its own process group and the timeout signals
/// the WHOLE group. Windows: the kill uses `taskkill /T` (native tree
/// kill). Both platforms can reap grandchildren; the old /proc walk
/// matched nothing on macOS (no /proc) and only ever killed the direct
/// child (P0-3, 2026-09-15).
/// Báo rõ nền tảng hiện tại có guard kill process-tree thật hay không.
/// Unix: child chạy trong process group riêng và timeout signal CẢ group.
/// Windows: kill qua `taskkill /T` (kill cây native). Cả hai nền tảng
/// đều dọn được grandchild; walk /proc cũ trên macOS không khớp gì
/// (không có /proc) và chỉ giết được child trực tiếp.
pub fn process_tree_guard_available() -> bool {
    cfg!(any(unix, windows))
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
    /// Capture FULL stdout into `stdout_full` (byte-bounded, not
    /// line-bounded) — required by scanner parsers (govulncheck streams
    /// findings throughout the payload). Default false keeps memory flat
    /// for non-scanner callers.
    /// Capture stdout ĐẦY ĐỦ vào `stdout_full` (giới hạn byte, không
    /// giới hạn dòng) — parser scanner cần (govulncheck phát finding rải
    /// khắp payload). Mặc định false giữ bộ nhớ phẳng cho caller thường.
    pub capture_full_stdout: bool,
    /// Rival JS runtime (bun|deno) exempted for THIS call because the CLI
    /// compat gate (cli compat.rs) already validated the explicit
    /// opt-in and printed the loud warning. None = no rival runtime may
    /// spawn (fail-closed). The exempt only ever covers the NAMED
    /// runtime — never other PMs.
    /// Runtime đối thủ (bun|deno) được miễn cho LỜI GỌI này vì cổng
    /// compat ở CLI đã validate opt-in tường minh + in cảnh báo. None =
    /// không runtime đối thủ nào được spawn (fail-closed). Miễn chỉ áp
    /// cho runtime ĐƯỢC NÊU TÊN — không bao giờ PM khác.
    pub compat_runtime: Option<String>,
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
    /// FULL stdout (bounded by MAX_CAPTURE_BYTES, NOT line-count) for
    /// scanner parsers that need the COMPLETE payload — streaming JSON
    /// (govulncheck) and report files (cargo-audit/pip-audit) lose
    /// findings when only the last 40 lines survive. Empty unless the
    /// caller sets `capture_full_stdout` (memory stays bounded).
    /// stdout ĐẦY ĐỦ (giới hạn MAX_CAPTURE_BYTES, KHÔNG giới hạn dòng)
    /// cho parser scanner cần payload trọn vẹn — JSON stream
    /// (govulncheck) và file report (cargo-audit/pip-audit) mất finding
    /// khi chỉ sống sót 40 dòng cuối. Rỗng trừ khi caller bật
    /// `capture_full_stdout` (bộ nhớ vẫn có giới hạn).
    pub stdout_full: String,
}

/// Resolve a bare command name to a Windows shim (.cmd/.bat) when the bare
/// executable is not directly spawnable. Uses where.exe (PATH search) so the
/// allowlist still governs WHICH tools can run — this only fixes HOW.
/// The search PATH is the CALLER-PROVIDED one (opts.env PATH — which the
/// task runner extends with node_modules/.bin), not the parent process env:
/// resolving against the wrong PATH made every project-local shim
/// (tsc.cmd, vite.cmd…) unspawnable (caught by the Windows E2E lane,
/// 2026-09-12).
/// Resolve tên lệnh sang shim Windows (.cmd/.bat) khi bare exe không spawn
/// được — dùng where.exe (tìm theo PATH), allowlist vẫn kiểm soát CHỨNG TỪ.
/// PATH tìm kiếm là PATH CỦA CALLER (opts.env PATH — task runner mở rộng
/// bằng node_modules/.bin), không phải env của process cha: resolve theo
/// PATH sai làm mọi shim local của project (tsc.cmd, vite.cmd…) không
/// spawn được (bắt được bởi lane E2E Windows).
#[cfg(not(unix))]
fn resolve_windows_shim(cmd: &str, search_path: Option<&std::ffi::OsStr>) -> std::ffi::OsString {
    use std::ffi::OsString;
    use std::os::windows::process::CommandExt;

    // Names that already carry an extension or path separators spawn as-is.
    if cmd.contains('.') || cmd.contains('\\') || cmd.contains('/') {
        return OsString::from(cmd);
    }

    let mut where_cmd = Command::new("where.exe");
    where_cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    where_cmd.arg(cmd);
    // Search the caller-provided PATH (node_modules/.bin etc.), falling
    // back to the parent env only when no PATH was provided.
    // Tìm theo PATH caller truyền (node_modules/.bin v.v.), chỉ fallback
    // về env cha khi không có PATH.
    if let Some(path) = search_path {
        where_cmd.env("PATH", path);
    }
    let output = where_cmd.output();
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
    crate::allowlist::check_tool_with_scope_compat(
        cmd,
        scope,
        opts.cwd.as_deref(),
        opts.compat_runtime.as_deref(),
    )?;
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
    crate::allowlist::check_tool_with_scope_compat(
        cmd,
        scope,
        opts.cwd.as_deref(),
        opts.compat_runtime.as_deref(),
    )?;
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
    // P0-1/F-A (2026-09-10): the CLI compat gate (already validated +
    // warned by allowlist::check_tool_with_scope_compat above) names ONE
    // rival runtime for THIS invocation. Only that runtime skips the
    // shadow-path blocker shim; scope rules stay untouched — Install
    // scope lifecycle scripts (no compat runtime) still get EVERY
    // blocker shim.
    // Cổng compat ở CLI đã validate + cảnh báo, nêu tên MỘT runtime cho
    // lời gọi này — chỉ runtime đó bỏ qua blocker shim; luật scope giữ
    // nguyên (lifecycle Install không compat vẫn đủ mọi shim).
    let mut scoped_exempt: Vec<&str> = if scope.allows_pm_tools() {
        crate::allowlist::FORBIDDEN_TOOLS.to_vec()
    } else {
        Vec::new()
    };
    if let Some(runtime) = opts.compat_runtime.as_deref()
        && matches!(runtime, "bun" | "deno")
        && !scoped_exempt.contains(&runtime)
    {
        scoped_exempt.push(match runtime {
            "bun" => "bun",
            "deno" => "deno",
            other => other,
        });
    }
    let scoped_exempt: &[&str] = &scoped_exempt;

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
            stdout_full: String::new(),
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
    // Caller-provided PATH (opts.env PATH — extended with node_modules/.bin
    // by the task runner) so project-local shims resolve too.
    // PATH caller truyền (opts.env PATH — task runner mở rộng bằng
    // node_modules/.bin) để shim local của project cũng resolve được.
    #[cfg(not(unix))]
    let path_var: Option<std::ffi::OsString> = opts
        .env
        .iter()
        .find(|(key, _)| key == "PATH")
        .map(|(_, value)| std::ffi::OsString::from(value));
    #[cfg(not(unix))]
    let resolved_cmd = resolve_windows_shim(cmd, path_var.as_ref().map(|p| p.as_os_str()));
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
    // Process-group isolation applies to EVERY spawn (not only clean_env):
    // the timeout must be able to signal the WHOLE tree for any tool
    // (P0-3, 2026-09-15). Windows: no-op here — the tree kill uses
    // `taskkill /T` which walks children natively.
    // Cô lập process-group áp cho MỌI lần spawn (không chỉ clean_env):
    // timeout phải signal được CẢ CÂY cho mọi tool (P0-3, 2026-09-15).
    // Windows: no-op tại đây — kill cây dùng `taskkill /T` tự duyệt con.
    configure_process_isolation(&mut command);

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
        stdout_full: if opts.capture_full_stdout {
            full(&outcome.stdout)
        } else {
            String::new()
        },
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

    // DEADLOCK FIX (2026-09-09): a child writing MORE than the OS pipe
    // buffer (~64 KB) blocks on write until the parent drains the pipe.
    // Polling try_wait WITHOUT draining never sees the child exit — the
    // govulncheck stream (multi-MB JSON) hung here. Drain stdout/stderr
    // on reader threads so the child can always finish, then re-join
    // the bytes when it exits.
    // FIX DEADLOCK: child viết NHIỀU HƠN buffer pipe của OS (~64 KB) sẽ
    // block chờ parent tiêu thụ. Poll try_wait mà không drain thì child
    // không bao giờ thoát — stream govulncheck (JSON MB) từng treo tại
    // đây. Drain stdout/stderr bằng thread đọc để child luôn chạy hết,
    // rồi ghép bytes lại khi nó thoát.
    //
    // The threads PUBLISH their bytes over a channel instead of being
    // joined: a join is unbounded, and a grandchild holding the inherited
    // pipe kept the join blocked forever even after the timeout fired
    // (P0-3, 2026-09-15 — the old code hung for the full 30s sleep).
    // (Thread đọc GỬI bytes qua channel thay vì join: join là vô hạn, và
    // grandchild giữ pipe thừa hưởng làm join treo mãi mãi dù timeout đã
    // nổ — code cũ treo đủ 30s của sleep.)
    let (stdout_tx, stdout_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let (stderr_tx, stderr_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let _stdout_reader = child.stdout.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut s, &mut buf);
            let _ = stdout_tx.send(buf);
        })
    });
    let _stderr_reader = child.stderr.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut s, &mut buf);
            let _ = stderr_tx.send(buf);
        })
    });
    // `bounded`: Some(deadline) is the post-kill contract — the pipes are
    // expected to EOF within milliseconds, so a short deadline is enough
    // and outranks a stuck writer (an escaped process cannot hang us).
    // None keeps the historical behaviour for the natural-exit path (drain
    // until EOF, full scanner payload). In the unbounded path recv() IS the
    // join (send completed ⇒ bytes are ours); in the bounded path a still-
    // parked reader thread is left to die with the process — it owns no
    // state we need and blocking on it would recreate the hang.
    // (`bounded`: Some(deadline) là hợp đồng sau-kill — pipe phải EOF trong
    // vài ms nên deadline ngắn là đủ và thắng writer kẹt (process thoát
    // group không thể treo ta). None giữ hành vi cũ cho đường thoát tự
    // nhiên — drain tới EOF, đủ payload. Ở đường vô hạn recv() CHÍNH LÀ
    // join (send xong ⇒ bytes là của ta); ở đường bounded thread còn kẹt
    // sẽ chết theo process — nó không giữ state ta cần và chặn nó sẽ
    // tái tạo cái treo.)
    let collect = |rx: &std::sync::mpsc::Receiver<Vec<u8>>, bounded: Option<Duration>| -> Vec<u8> {
        match bounded {
            Some(deadline) => rx.recv_timeout(deadline).unwrap_or_default(),
            None => rx.recv().unwrap_or_default(),
        }
    };
    let drain = |child: &mut std::process::Child, bounded: Option<Duration>| -> ExecOutcome {
        let stdout = collect(&stdout_rx, bounded);
        let stderr = collect(&stderr_rx, bounded);
        let status = child.wait().unwrap_or_default();
        ExecOutcome {
            status,
            stdout,
            stderr,
        }
    };
    let _ = mode;

    loop {
        if let Some(_status) = child.try_wait()? {
            // Child exited — pipes may still hold buffered bytes; the
            // reader threads hit EOF (child end closed) and return them.
            // Child đã thoát — pipe có thể còn byte; thread đọc gặp EOF
            // (đầu child đã đóng) và trả về chúng.
            return Ok(drain(&mut child, None));
        }
        // Monitor + forbidden child in one guard — gộp điều kiện theo clippy 1.98.
        if monitor_forbidden_children
            && let Some(found) = find_forbidden_descendant(child.id(), exempt)
        {
            terminate_process_tree(child.id());
            let _ = child.kill();
            let out = drain(&mut child, Some(Duration::from_millis(POST_KILL_DRAIN_MS)));
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
            let out = drain(&mut child, Some(Duration::from_millis(POST_KILL_DRAIN_MS)));
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

// Process-group isolation (unix): the child becomes its own group leader so
// the timeout can signal the WHOLE tree with `kill(-pgid)`. The no-op below
// covers Windows, where the tree kill uses `taskkill /T` instead
// (CI Windows compile fix 2026-09-11 — keep the cfg gates paired).
// Cô lập process-group (unix): child thành group leader riêng để timeout
// signal CẢ CÂY bằng `kill(-pgid)`. No-op dưới đây phủ Windows — kill cây
// dùng `taskkill /T` (fix compile CI Windows 2026-09-11 — giữ cặp cfg gate).
#[cfg(unix)]
fn configure_process_isolation(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_isolation(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_tree(root_pid: u32) {
    // P0-3 (Tech Lead 2026-09-15): kill the WHOLE process GROUP.
    //
    // The child is spawned with `process_group(0)` (see
    // configure_process_isolation), so its PID *is* its PGID and every
    // descendant — grandchildren included — inherits that group. One group
    // signal therefore reaches the entire tree, with no /proc walk: the old
    // walk matched NOTHING on macOS (no /proc) and silently degraded to
    // "kill the direct child only", leaving the grandchild holding the
    // stdout pipe and hanging the drain.
    //
    // The group is derived from the PID we spawned ourselves (never looked
    // up), so it can never resolve to the CI job's own group — the failure
    // mode the old comment warned about.
    //
    // (P0-3: kill CẢ process GROUP. Child spawn với `process_group(0)` nên
    // PID CHÍNH LÀ PGID và mọi con cháu thừa hưởng group đó. Một group
    // signal tới cả cây, không cần walk /proc: walk cũ trên macOS không
    // khớp gì (không có /proc) và lặng lẽ thoái hóa thành "chỉ giết child
    // trực tiếp", để grandchild giữ pipe stdout và treo drain. Group suy
    // từ PID do chính ta spawn (không tra cứu), nên không bao giờ trỏ vào
    // group của job CI — đúng chế độ hỏng mà comment cũ cảnh báo.)
    let group = -(root_pid as i32);
    // SAFETY: the negative pid is the process group of a child this process
    // spawned itself (process_group(0) made it its own group leader), so the
    // signal targets a private group we created — it can never resolve to the
    // caller's own group. Worst case the target already exited: kill()
    // returns ESRCH and no foreign process is ever addressed.
    // (An toàn: pid âm là process group của child do chính tiến trình này
    // spawn (process_group(0) khiến nó tự làm group leader), signal chỉ nhắm
    // group riêng do ta tạo ra — không bao giờ trùng group của caller. Xấu
    // nhất target đã thoát: kill() trả ESRCH, không trúng tiến trình lạ.)
    #[allow(unsafe_code)]
    unsafe {
        libc::kill(group, libc::SIGTERM);
    }
    // Short grace: a well-behaved tool flushes and exits on TERM before the
    // hammer lands. (Cửa nghiêng ngắn: tool tử tế kịp flush rồi thoát.)
    std::thread::sleep(Duration::from_millis(TERM_TO_KILL_GRACE_MS));
    // SAFETY: same provenance as the SIGTERM shot above — the negative pid
    // is the private group of our own spawned child and the direct pid is
    // that same child; neither id is looked up, so a foreign process can
    // never be addressed. If the target already exited, kill() returns ESRCH.
    // (An toàn: cùng nguồn gốc như phát SIGTERM trên — pid âm là group riêng
    // của child do ta spawn, pid dương là chính child đó; không id nào được
    // tra cứu ngoài, nên không bao giờ trúng tiến trình lạ. Target đã thoát
    // thì kill() trả ESRCH.)
    #[allow(unsafe_code)]
    unsafe {
        libc::kill(group, libc::SIGKILL);
        // Belt-and-braces: also signal the root directly in case the tool
        // called setsid() and left our target group. Escaped descendants
        // are out of reach for a group signal — the bounded post-kill drain
        // is what keeps that case from hanging the caller.
        // (Dự phòng: signal luôn root trực tiếp phòng khi tool gọi setsid()
        // và rời group. Con cháu đã thoát group nằm ngoài tầm group signal —
        // drain bounded sau kill chính là thứ giữ ca đó không treo caller.)
        libc::kill(root_pid as i32, libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn terminate_process_tree(root_pid: u32) {
    // Windows: no Job-Object API in std and no new dependency is allowed, so
    // use the OS-native tree kill — taskkill /T walks the child tree and /F
    // forces it. Best-effort: a process that already exited returns non-zero
    // and that is fine.
    // (Windows: std không có Job-Object API và không được thêm dependency,
    // nên dùng kill cây native của OS — taskkill /T duyệt cây con, /F ép
    // buộc. Best-effort: process đã thoát trả non-zero, không sao.)
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &root_pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

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
    // Sub-second timeouts print as milliseconds — "after 0s" hid the
    // actual budget in tests with 250ms timeouts (P0-3, 2026-09-15).
    // (Timeout dưới 1 giây in theo ms — "after 0s" giấu ngân sách thật
    // trong các test dùng timeout 250ms.)
    let budget = if timeout.as_secs() > 0 || timeout.subsec_millis() == 0 {
        format!("{}s", timeout.as_secs())
    } else {
        format!("{}ms", timeout.as_millis())
    };
    anyhow::anyhow!(
        "command timed out after {budget} (status after kill: {})\n--- tail ---\n{}",
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

/// Bounded FULL capture for scanner parsers — byte-capped (memory stays
/// flat) but NOT line-capped, so streaming-JSON findings anywhere in the
/// payload survive. A stream larger than the cap fails closed in the
/// parsers (invalid JSON), never silently truncates findings.
/// Capture ĐẦY ĐỦ có giới hạn byte cho parser scanner — giới hạn byte
/// (bộ nhớ phẳng) nhưng KHÔNG giới hạn dòng, finding JSON stream ở bất
/// kỳ đâu cũng sống sót. Stream lớn hơn giới hạn thì parser fail-closed
/// (JSON lỗi), không cắt cụt finding âm thầm.
fn full(bytes: &[u8]) -> String {
    let slice = if bytes.len() > MAX_CAPTURE_BYTES {
        &bytes[bytes.len() - MAX_CAPTURE_BYTES..]
    } else {
        bytes
    };
    String::from_utf8_lossy(slice).to_string()
}
