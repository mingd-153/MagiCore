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
/// Retry a missing-root snapshot for 100 ms before treating it as unverifiable.
/// Thử lại snapshot thiếu root trong 100 ms trước khi xem là không thể xác minh.
#[cfg(unix)]
const ROOT_VISIBILITY_RETRIES: usize = 5;
/// Retry for at most 500 ms of sleep while refreshing Windows child metadata.
/// Chờ tối đa 500 ms tổng thời gian ngủ và liên tục làm mới metadata tiến trình con.
#[cfg(any(windows, test))]
const WINDOWS_COMMAND_LINE_RETRIES: usize = 20;
/// Bound Job Object re-enumeration while descendants are changing during inspection.
/// Giới hạn số lần liệt kê lại Job Object khi descendants thay đổi trong lúc kiểm tra.
#[cfg(any(windows, test))]
const WINDOWS_JOB_MEMBER_SNAPSHOT_RETRIES: usize = 20;
#[cfg(any(windows, test))]
const WINDOWS_COMMAND_LINE_RETRY_DELAY_MS: u64 = 25;
#[cfg(any(windows, test))]
/// Check child status during short waits so natural exits stay responsive.
/// Kiểm tra trạng thái child trong lúc chờ ngắn để nhận biết thoát tự nhiên kịp thời.
const WINDOWS_COMMAND_LINE_RETRY_POLL_INTERVAL_MS: u64 = 1;
#[cfg(all(windows, test))]
const WINDOWS_FILETIME_TICKS_PER_SECOND: u64 = 10_000_000;
#[cfg(all(windows, test))]
const WINDOWS_FILETIME_UNIX_EPOCH_OFFSET_SECONDS: u64 = 11_644_473_600;
/// Exact read-only npm shim body; the process guard authenticates these bytes.
/// Nội dung chính xác của shim npm chỉ đọc; process guard xác thực byte này.
#[cfg(unix)]
const NPM_VERSION_PROBE_SHIM_CONTENT: &str = "#!/bin/sh\nif [ \"$#\" -eq 1 ] && [ \"$1\" = \"--version\" ]; then\n  printf '%s\\n' '0.0.0'\n  exit 0\nfi\nprintf '%s\\n' \"MagiCore blocked forbidden package manager: npm\" >&2\nexit 126\n";
/// Grace window between the SIGTERM and SIGKILL sweep over the process
/// group — long enough for a well-behaved tool to flush, short enough to
/// keep `timeout + grace` far below any caller-visible wall-clock budget.
/// (Cửa nghiêng giữa SIGTERM và SIGKILL trên process group — đủ cho tool
/// tử tế kịp flush, đủ ngắn để timeout + grace luôn thấp hơn mọi ngân
/// sách wall-clock của caller.)
#[cfg(unix)]
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
/// Bound retries when a generated shim directory name collides.
/// Giới hạn retry khi tên thư mục shim sinh ra bị trùng.
const MAX_SHADOW_PATH_ATTEMPTS: usize = 16;
static SHADOW_PATH_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(unix)]
const PROCESS_TREE_TRACKING_ENV: &str = "MGC_EXEC_PROCESS_TREE_ID";
#[cfg(unix)]
static PROCESS_TREE_TRACKING_SEQUENCE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[cfg(unix)]
fn process_tree_tracking_token() -> String {
    let sequence =
        PROCESS_TREE_TRACKING_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("{:x}-{sequence:x}-{timestamp:x}", std::process::id())
}

/// Report whether process-tree kill is active on this platform.
/// Unix: the child runs in its own process group and the timeout signals
/// the WHOLE group. Windows: a Job Object owns the suspended-at-start child
/// tree and terminates it when the guard closes. Both platforms can reap
/// grandchildren; the old /proc walk
/// matched nothing on macOS (no /proc) and only ever killed the direct
/// child (P0-3, 2026-09-15).
/// Báo rõ nền tảng hiện tại có guard kill process-tree thật hay không.
/// Unix: child chạy trong process group riêng và timeout signal CẢ group.
/// Windows: Job Object giữ cả cây process và dọn cây khi guard đóng.
/// Cả hai nền tảng đều dọn được grandchild; walk /proc cũ trên macOS không khớp gì
/// (không có /proc) và chỉ giết được child trực tiếp.
pub fn process_tree_guard_available() -> bool {
    cfg!(any(unix, windows))
}

#[cfg(test)]
#[path = "test/run_tests.rs"]
mod tests;

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
    /// Execution scope for tool-specific policy; package managers are forbidden in every scope.
    /// None defaults to Install scope (most restrictive).
    /// Phạm vi áp dụng chính sách theo tool; package manager bị cấm ở mọi scope.
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
    /// Deprecated legacy field; executor policy ignores it and never
    /// authorizes Bun/Deno or an external package manager.
    /// Trường cũ đã deprecated; executor bỏ qua và không cấp quyền spawn.
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

/// Find a bare Windows command in the supplied PATH, preferring native PE
/// executables over command shims and extensionless scripts.
/// (Tìm executable Windows trong PATH được truyền, ưu tiên PE trước shim và script không đuôi.)
#[cfg(any(windows, test))]
fn find_windows_command(
    cmd: &str,
    search_path: Option<&std::ffi::OsStr>,
) -> Option<std::ffi::OsString> {
    let search_path = search_path
        .map(std::ffi::OsStr::to_os_string)
        .or_else(|| std::env::var_os("PATH"))?;
    let suffixes = [".exe", ".com", ".cmd", ".bat", ""];
    let mut matches: [Option<std::ffi::OsString>; 5] = Default::default();

    for directory in std::env::split_paths(&search_path) {
        // An empty PATH entry resolves against the current directory. Do not
        // turn an absent/empty path entry into an implicit cwd search.
        if directory.as_os_str().is_empty() {
            continue;
        }
        for (rank, suffix) in suffixes.iter().enumerate() {
            if matches[rank].is_some() {
                continue;
            }
            let candidate = directory.join(format!("{cmd}{suffix}"));
            if candidate.is_file() {
                matches[rank] = Some(candidate.into_os_string());
            }
        }
    }

    matches.into_iter().flatten().next()
}

/// Resolve a bare command name to a Windows shim (.cmd/.bat) when the bare
/// executable is not directly spawnable. PATH lookup is performed in-process,
/// so resolving a tool does not spawn a second executable.
/// (Resolve tên lệnh sang shim Windows khi cần; tự dò PATH, không spawn helper.)
/// The search PATH is the CALLER-PROVIDED one (opts.env PATH — which the
/// task runner extends with node_modules/.bin), not the parent process env:
/// resolving against the wrong PATH made every project-local shim
/// (tsc.cmd, vite.cmd…) unspawnable (caught by the Windows E2E lane,
/// 2026-09-12).
/// Resolve tên lệnh sang shim Windows (.cmd/.bat) bằng PATH lookup in-process;
/// allowlist kiểm tra tên tool gốc trước bước resolve này.
/// Resolve a command to a Windows shim (.cmd/.bat) with in-process PATH lookup;
/// the allowlist validates the original tool name before this resolution.
/// PATH tìm kiếm là PATH CỦA CALLER (opts.env PATH — task runner mở rộng
/// bằng node_modules/.bin), không phải env của process cha: resolve theo
/// PATH sai làm mọi shim local của project (tsc.cmd, vite.cmd…) không
/// spawn được (bắt được bởi lane E2E Windows).
#[cfg(windows)]
fn resolve_windows_shim(cmd: &str, search_path: Option<&std::ffi::OsStr>) -> std::ffi::OsString {
    use std::ffi::OsString;

    // Names that already carry an extension or path separators spawn as-is.
    if cmd.contains('.') || cmd.contains('\\') || cmd.contains('/') {
        return OsString::from(cmd);
    }
    find_windows_command(cmd, search_path).unwrap_or_else(|| OsString::from(cmd))
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
    reject_external_dependency_resolution(cmd, args, &opts.env)?;
    reject_forbidden_script_file(cmd)?;
    execute_command(cmd, args, opts, OutputMode::Capture)
}

/// Run an allowlisted tool with inherited stdio for streaming commands.
/// On Unix, the private session intentionally does not inherit `/dev/tty` or job control.
/// Chạy tool allowlist với stdio trực tiếp cho lệnh streaming.
/// Trên Unix, session riêng không kế thừa `/dev/tty` hoặc job control.
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
    reject_external_dependency_resolution(cmd, args, &opts.env)?;
    reject_forbidden_script_file(cmd)?;
    execute_command(cmd, args, opts, OutputMode::Inherit)
}

/// Reject package-manager operations and require offline/frozen modes for
/// compiler drivers that also resolve dependencies. This is a guardrail, not
/// a claim that executing arbitrary project code is an OS sandbox.
fn reject_external_dependency_resolution(
    command: &str,
    args: &[String],
    env: &[(String, String)],
) -> Result<()> {
    let mut tool = process_basename(command);
    if tool.starts_with("python") {
        tool = "python".to_string();
    }
    let args: Vec<String> = args
        .iter()
        .take_while(|arg| arg.as_str() != "--")
        .map(|arg| arg.to_ascii_lowercase())
        .collect();
    let has = |flag: &str| args.iter().any(|arg| arg == flag);
    let (first, _) = first_command(&args);
    let goproxy_disabled = env
        .iter()
        .rev()
        .find(|(key, _)| key.eq_ignore_ascii_case("GOPROXY"))
        .is_some_and(|(_, value)| value.eq_ignore_ascii_case("off"));

    let reason = match tool.as_str() {
        "cargo"
            if matches!(
                first,
                "add" | "remove" | "update" | "fetch" | "install" | "vendor" | "generate-lockfile"
            ) =>
        {
            Some("Cargo dependency-management subcommands are not allowed through mgc-exec")
        }
        "cargo"
            if matches!(
                first,
                "package"
                    | "publish"
                    | "search"
                    | "login"
                    | "logout"
                    | "owner"
                    | "yank"
                    | "tree"
                    | "info"
            ) =>
        {
            Some("Cargo registry/package subcommands are not allowed through mgc-exec")
        }
        "cargo"
            if matches!(
                first,
                "build" | "test" | "check" | "run" | "clippy" | "metadata"
            ) && !(has("--locked") && has("--offline")) =>
        {
            Some("Cargo compile/test commands must include --locked --offline")
        }
        "cargo"
            if matches!(
                first,
                "build" | "test" | "check" | "run" | "clippy" | "metadata"
            ) && has("--locked")
                && has("--offline") =>
        {
            None
        }
        "cargo"
            if matches!(
                first,
                "fmt"
                    | "clean"
                    | "version"
                    | "help"
                    | "locate-project"
                    | "read-manifest"
                    | "verify-project"
            ) =>
        {
            None
        }
        // Bare `cargo` prints its help text and does not resolve dependencies.
        "cargo" if first.is_empty() => None,
        "cargo" => Some("unclassified Cargo subcommands are blocked by mgc-exec policy"),
        "rustc" if args.len() == 1 && args[0] == "-vv" => None,
        "rustc" => Some("rustc is restricted to the read-only -vV host target query"),
        "python"
            if args
                .windows(2)
                .any(|pair| pair[0] == "-m" && is_forbidden_python_module(&pair[1])) =>
        {
            Some("Python package-manager modules are not allowed through mgc-exec")
        }
        "python" if has_module(&args, "ensurepip") => {
            Some("Python ensurepip is not allowed through mgc-exec")
        }
        "python"
            if args
                .windows(2)
                .any(|pair| pair[0] == "-m" && pair[1] == "build")
                && !has("--no-isolation") =>
        {
            Some(
                "Python build frontend must include --no-isolation to prevent dependency installation",
            )
        }
        "py" if args
            .windows(2)
            .any(|pair| pair[0] == "-m" && is_forbidden_python_module(&pair[1])) =>
        {
            Some("Python package-manager modules are not allowed through mgc-exec")
        }
        "py" if has_module(&args, "ensurepip") => {
            Some("Python ensurepip is not allowed through mgc-exec")
        }
        "go" if matches!(first, "mod" | "work") => {
            Some("Go module/workspace commands are not allowed through mgc-exec")
        }
        "go" if matches!(first, "get" | "install") => {
            Some("Go dependency-management subcommands are not allowed through mgc-exec")
        }
        "go" if first == "list" => {
            Some("Go list may resolve modules and is not allowed through mgc-exec")
        }
        "go" if matches!(first, "build" | "test" | "run")
            && (!has("-mod=readonly") || !goproxy_disabled) =>
        {
            Some("Go compile/test commands must use -mod=readonly and GOPROXY=off")
        }
        "dotnet" if matches!(first, "restore" | "add" | "remove") => {
            Some(".NET dependency-management subcommands are not allowed through mgc-exec")
        }
        "dotnet" if matches!(first, "tool" | "workload" | "nuget" | "msbuild") => {
            Some(".NET package/toolchain-management commands are not allowed through mgc-exec")
        }
        "dotnet" if requests_restore_target(&args) => {
            Some("explicit MSBuild Restore targets are not allowed through mgc-exec")
        }
        "dotnet"
            if matches!(first, "build" | "test" | "publish" | "run" | "pack")
                && !has("--no-restore") =>
        {
            Some(".NET compile/test commands must include --no-restore")
        }
        "flutter" | "dart" if has("pub") => {
            Some("Dart/Flutter package-manager commands are not allowed through mgc-exec")
        }
        "flutter" if matches!(first, "build" | "test" | "run") && !has("--no-pub") => {
            Some("Flutter build/test/run commands must include --no-pub")
        }
        "swift" if has("package") => {
            Some("SwiftPM dependency-management commands are not allowed through mgc-exec")
        }
        "swift"
            if matches!(first, "build" | "test" | "run")
                && !(has("--skip-update") && has("--disable-automatic-resolution")) =>
        {
            Some(
                "Swift compile/test commands must disable package updates and automatic resolution",
            )
        }
        "gradle" | "gradlew" if !has("--offline") => Some("Gradle commands must include --offline"),
        "mvn" | "mvnw" if !has("-o") && !has("--offline") => {
            Some("Maven commands must include offline mode (-o)")
        }
        _ => None,
    };

    if let Some(reason) = reason {
        anyhow::bail!("external dependency resolution blocked: {reason}");
    }
    Ok(())
}

fn has_module(args: &[String], module: &str) -> bool {
    args.windows(2)
        .any(|pair| pair[0] == "-m" && pair[1] == module)
}

fn is_forbidden_python_module(module: &str) -> bool {
    [
        "pip", "uv", "poetry", "pipenv", "pdm", "conda", "mamba", "hatch", "rye", "pixi",
    ]
    .iter()
    .any(|name| {
        module == *name
            || module
                .strip_prefix(name)
                .is_some_and(|suffix| suffix.starts_with('.'))
    })
}

fn requests_restore_target(args: &[String]) -> bool {
    const TARGET_PREFIXES: &[&str] = &["-t:", "/t:", "-target:", "/target:", "--target="];

    args.iter().enumerate().any(|(index, arg)| {
        let target_value = TARGET_PREFIXES
            .iter()
            .find_map(|prefix| arg.strip_prefix(prefix))
            .or_else(|| {
                matches!(
                    arg.as_str(),
                    "-t" | "/t" | "-target" | "/target" | "--target"
                )
                .then(|| args.get(index + 1).map(String::as_str))
                .flatten()
            });

        target_value.is_some_and(|targets| {
            targets.split([';', ',']).any(|target| {
                target
                    .trim_matches(|ch| matches!(ch, '\'' | '"'))
                    .eq_ignore_ascii_case("restore")
            })
        })
    })
}

fn first_command(args: &[String]) -> (&str, &str) {
    let mut positional = Vec::with_capacity(2);
    let mut skip_value = false;
    for arg in args {
        if skip_value {
            skip_value = false;
            continue;
        }
        if matches!(
            arg.as_str(),
            "--manifest-path"
                | "--config"
                | "--color"
                | "--message-format"
                | "--target-dir"
                | "--target"
                | "--package"
                | "--exclude"
                | "--package-path"
                | "-C"
        ) {
            skip_value = true;
            continue;
        }
        if arg.starts_with('-') || arg.starts_with('+') {
            continue;
        }
        positional.push(arg.as_str());
        if positional.len() == 2 {
            break;
        }
    }
    (
        positional.first().copied().unwrap_or_default(),
        positional.get(1).copied().unwrap_or_default(),
    )
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

    reject_external_dependency_resolution(&canonical.display().to_string(), args, &opts.env)?;

    reject_forbidden_script_file(&canonical.display().to_string())?;
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
    // No package manager is exempt in any scope. A compatibility runtime
    // may be invoked only as a runtime; the PM executable itself stays
    // blocked by the command validator and descendant monitor.
    // No package manager or rival runtime receives a process-tree exemption.
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
    #[cfg(windows)]
    let path_var: Option<std::ffi::OsString> = opts
        .env
        .iter()
        .rev()
        .find(|(key, _)| is_path_env_key(key))
        .map(|(_, value)| std::ffi::OsString::from(value));
    #[cfg(windows)]
    let resolved_cmd = resolve_windows_shim(cmd, path_var.as_deref());
    #[cfg(unix)]
    let resolved_cmd: &str = cmd;
    #[cfg(all(not(unix), not(windows)))]
    let resolved_cmd: &str = cmd;

    // Windows: If resolved_cmd is .cmd/.bat, spawn via cmd.exe to avoid "not a valid Win32 application"
    #[cfg(windows)]
    let mut command = {
        let resolved_str = resolved_cmd.to_string_lossy();
        let is_script = resolved_str.to_ascii_lowercase().ends_with(".cmd")
            || resolved_str.to_ascii_lowercase().ends_with(".bat");

        if is_script {
            windows_batch_command(&resolved_str, args, &cwd)?
        } else {
            let mut cmd = Command::new(&resolved_cmd); // Borrow instead of move
            cmd.args(args).current_dir(&cwd);
            cmd
        }
    };

    #[cfg(all(not(unix), not(windows)))]
    let mut command = {
        let mut cmd = Command::new(resolved_cmd);
        cmd.args(args).current_dir(&cwd);
        cmd
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
    // Isolation applies to EVERY spawn (not only clean_env): timeout must control the WHOLE tree.
    // Cô lập áp dụng cho MỌI lần spawn (không chỉ clean_env): timeout phải kiểm soát CẢ CÂY.
    let process_tree_guard = configure_process_isolation(&mut command)?;

    let shadow_path = ShadowPath::create(scoped_exempt)?;
    let path_env = guarded_path_env(shadow_path.path(), &opts.env)?;
    if opts.clean_env {
        command.env_clear();
        for (key, value) in &opts.env {
            if !is_path_env_key(key) {
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
    } else {
        // Preserve the caller's environment, but prepend deny shims so child
        // processes cannot reach a forbidden package manager through PATH.
        // (Giữ env caller nhưng đặt deny shim đầu PATH để child không gọi PM qua PATH.)
        for (key, value) in &opts.env {
            if !is_path_env_key(key) {
                command.env(key, value);
            }
        }
        command.env("PATH", path_env);
    }

    let monitor_forbidden_children = true;
    ensure_process_inspection_available(monitor_forbidden_children, cfg!(any(unix, windows)))?;
    #[cfg(unix)]
    let process_tree_tracking_token = process_tree_tracking_token();
    #[cfg(unix)]
    command.env(PROCESS_TREE_TRACKING_ENV, &process_tree_tracking_token);
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut child = command
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn '{cmd}': {e}"))?;
    #[cfg(windows)]
    if let Err(error) = process_tree_guard.activate(&child) {
        let _ = process_tree_guard.terminate();
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }

    let timeout = if opts.disable_timeout {
        None
    } else {
        Some(opts.timeout.unwrap_or_else(default_timeout))
    };
    let process_monitor = ProcessMonitor {
        enabled: monitor_forbidden_children,
        exempt: scoped_exempt,
        #[cfg(unix)]
        shadow_dir: shadow_path.path(),
        #[cfg(unix)]
        tracking_token: &process_tree_tracking_token,
    };
    let outcome = wait_with_timeout(child, timeout, process_monitor, process_tree_guard, mode)?;
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
        for _ in 0..MAX_SHADOW_PATH_ATTEMPTS {
            let dir = unique_temp_dir("mgc-exec-shadow-path");
            match Self::create_at(dir, scoped_exempt) {
                Ok(shadow_path) => return Ok(shadow_path),
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|io_error| {
                            io_error.kind() == std::io::ErrorKind::AlreadyExists
                        }) =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            }
        }

        anyhow::bail!("could not allocate a unique package-manager deny directory");
    }

    fn create_at(dir: PathBuf, scoped_exempt: &[&str]) -> Result<Self> {
        create_shadow_directory(&dir)?;
        let shadow_path = Self { dir };
        for tool in FORBIDDEN_TOOLS {
            if !scoped_exempt.contains(tool)
                && let Err(error) = write_blocker(shadow_path.path(), tool)
            {
                drop(shadow_path);
                return Err(error);
            }
        }
        #[cfg(unix)]
        set_shadow_directory_mode(shadow_path.path(), 0o500)?;

        Ok(shadow_path)
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

#[cfg(unix)]
fn create_shadow_directory(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700);
    builder.create(dir)
}

#[cfg(not(unix))]
fn create_shadow_directory(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir(dir)
}

impl Drop for ShadowPath {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            // Restore owner write access so read-only shim files can be removed.
            // Mở lại quyền ghi cho owner để xóa được các shim chỉ đọc.
            let _ = set_shadow_directory_mode(&self.dir, 0o700);
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(unix)]
fn set_shadow_directory_mode(dir: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(dir)?.permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(dir, permissions)?;
    Ok(())
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = SHADOW_PATH_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{nanos}-{sequence}",
        std::process::id()
    ))
}

#[cfg(unix)]
fn write_blocker(dir: &Path, tool: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(tool);
    use std::io::Write;
    // Framework CLIs may ask npm for its version during a build. Answer that
    // read-only probe locally; every package-manager operation still fails.
    // CLI framework đôi khi hỏi phiên bản npm khi build. Trả lời probe đọc;
    // mọi thao tác package manager vẫn bị chặn.
    let content = if tool == "npm" {
        NPM_VERSION_PROBE_SHIM_CONTENT.to_string()
    } else {
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"MagiCore blocked forbidden package manager: {tool}\" >&2\nexit 126\n"
        )
    };
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(content.as_bytes())?;
    drop(file);
    let mut permissions = std::fs::metadata(&path)?.permissions();
    permissions.set_mode(0o500);
    std::fs::set_permissions(&path, permissions)?;
    Ok(())
}

#[cfg(windows)]
fn write_blocker(dir: &Path, tool: &str) -> Result<()> {
    use std::io::Write;

    let path = dir.join(format!("{tool}.cmd"));
    // Keep Windows deny-only: CMD argument expansion is unsafe to interpolate into script syntax.
    // Windows luôn chặn npm: nội suy tham số CMD vào cú pháp script có thể tạo command injection.
    let content = format!(
        "@echo off\r\necho MagiCore blocked forbidden package manager: {tool} 1>&2\r\nexit /b 126\r\n"
    );
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

fn is_path_env_key(key: &str) -> bool {
    #[cfg(windows)]
    {
        key.eq_ignore_ascii_case("PATH")
    }
    #[cfg(not(windows))]
    {
        key == "PATH"
    }
}

fn guarded_path_env(shadow_dir: &Path, explicit_env: &[(String, String)]) -> Result<OsString> {
    let explicit_path = explicit_env
        .iter()
        .rev()
        .find(|(key, _)| is_path_env_key(key))
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

struct ProcessMonitor<'a> {
    enabled: bool,
    exempt: &'a [&'a str],
    #[cfg(unix)]
    shadow_dir: &'a Path,
    #[cfg(unix)]
    tracking_token: &'a str,
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Option<Duration>,
    process_monitor: ProcessMonitor<'_>,
    _process_tree_guard: ProcessTreeGuard,
    mode: OutputMode,
) -> Result<ExecOutcome> {
    let started = Instant::now();
    #[cfg(unix)]
    let mut observed_process_groups = std::collections::HashSet::from([child.id()]);
    #[cfg(unix)]
    let mut root_reaped = false;
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
    let drain =
        |child: &mut std::process::Child, bounded: Option<Duration>| -> (ExecOutcome, bool) {
            let deadline = bounded.map(|duration| Instant::now() + duration);
            let collect = |rx: &std::sync::mpsc::Receiver<Vec<u8>>| match deadline {
                Some(deadline) => {
                    match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                        Ok(bytes) => (bytes, true),
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => (Vec::new(), false),
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => (Vec::new(), true),
                    }
                }
                None => (rx.recv().unwrap_or_default(), true),
            };
            let (stdout, stdout_complete) = collect(&stdout_rx);
            let (stderr, stderr_complete) = collect(&stderr_rx);
            let status = child.wait().unwrap_or_default();
            (
                ExecOutcome {
                    status,
                    stdout,
                    stderr,
                },
                stdout_complete && stderr_complete,
            )
        };
    let _ = mode;
    loop {
        #[cfg(unix)]
        let process_scan = if process_monitor.enabled {
            find_forbidden_descendant_tracking(
                child.id(),
                process_monitor.exempt,
                process_monitor.shadow_dir,
                process_monitor.tracking_token,
                &mut observed_process_groups,
            )
        } else {
            Ok(None)
        };
        #[cfg(unix)]
        if let Ok(Some(found)) = &process_scan {
            terminate_process_tree_tracked(
                child.id(),
                &mut observed_process_groups,
                process_monitor.tracking_token,
                process_monitor.exempt,
                process_monitor.shadow_dir,
                root_reaped,
            );
            let (out, _) = drain(&mut child, Some(Duration::from_millis(POST_KILL_DRAIN_MS)));
            return Err(forbidden_child_error(
                found,
                out.status,
                &out.stderr,
                &out.stdout,
            ));
        }

        if let Some(_status) = child.try_wait()? {
            #[cfg(unix)]
            {
                root_reaped = true;
            }
            // Child exited — pipes may still hold buffered bytes; the
            // reader threads hit EOF (child end closed) and return them.
            // Child đã thoát — pipe có thể còn byte; thread đọc gặp EOF
            // (đầu child đã đóng) và trả về chúng.
            #[cfg(windows)]
            {
                if process_monitor.enabled {
                    return finish_monitored_root_exit(
                        &mut child,
                        _process_tree_guard,
                        process_monitor.exempt,
                        drain,
                    );
                }
                // Close the job so grandchildren cannot outlive a completed root command.
                // Đóng job để tiến trình cháu không thể sống sau khi lệnh gốc đã xong.
                drop(_process_tree_guard);
                return Ok(drain(&mut child, Some(Duration::from_millis(POST_KILL_DRAIN_MS))).0);
            }
            #[cfg(unix)]
            {
                if process_monitor.enabled {
                    match process_scan {
                        Ok(Some(found)) => {
                            terminate_process_tree_tracked(
                                child.id(),
                                &mut observed_process_groups,
                                process_monitor.tracking_token,
                                process_monitor.exempt,
                                process_monitor.shadow_dir,
                                root_reaped,
                            );
                            let (out, _) =
                                drain(&mut child, Some(Duration::from_millis(POST_KILL_DRAIN_MS)));
                            return Err(forbidden_child_error(
                                &found,
                                out.status,
                                &out.stderr,
                                &out.stdout,
                            ));
                        }
                        Err(error) if error.downcast_ref::<MonitoredRootNotVisible>().is_some() => {
                        }
                        Ok(None) => {}
                        Err(error) => {
                            terminate_process_tree_tracked(
                                child.id(),
                                &mut observed_process_groups,
                                process_monitor.tracking_token,
                                process_monitor.exempt,
                                process_monitor.shadow_dir,
                                root_reaped,
                            );
                            let (out, _) =
                                drain(&mut child, Some(Duration::from_millis(POST_KILL_DRAIN_MS)));
                            return Err(anyhow::anyhow!(
                                "cannot verify child process tree after command exit; descendants were terminated (status: {}): {error}",
                                out.status
                            ));
                        }
                    }
                    if process_groups_have_descendants(&observed_process_groups, child.id()) {
                        terminate_process_tree_tracked(
                            child.id(),
                            &mut observed_process_groups,
                            process_monitor.tracking_token,
                            process_monitor.exempt,
                            process_monitor.shadow_dir,
                            root_reaped,
                        );
                        let (out, _) =
                            drain(&mut child, Some(Duration::from_millis(POST_KILL_DRAIN_MS)));
                        return Err(anyhow::anyhow!(
                            "command root exited while child processes remained in its process group; the process tree was terminated (status: {})",
                            out.status
                        ));
                    }
                }
                if let Some(timeout) = timeout {
                    let remaining = timeout.saturating_sub(started.elapsed());
                    let (out, complete) = drain(&mut child, Some(remaining));
                    if complete {
                        return Ok(out);
                    }
                    terminate_process_tree_tracked(
                        child.id(),
                        &mut observed_process_groups,
                        process_monitor.tracking_token,
                        process_monitor.exempt,
                        process_monitor.shadow_dir,
                        root_reaped,
                    );
                    let (out, _) =
                        drain(&mut child, Some(Duration::from_millis(POST_KILL_DRAIN_MS)));
                    return Err(timeout_error(timeout, out.status, &out.stderr, &out.stdout));
                }
                return Ok(drain(&mut child, None).0);
            }
            #[cfg(not(any(unix, windows)))]
            return Ok(drain(&mut child, None).0);
        }
        if process_monitor.enabled {
            #[cfg(windows)]
            let process_scan = find_forbidden_job_descendant_with_timeout(
                &_process_tree_guard,
                child.id(),
                process_monitor.exempt,
                started,
                timeout,
                || Ok(child.try_wait()?.is_some()),
            );
            #[cfg(windows)]
            let process_scan = match process_scan {
                Ok(WindowsProcessScan::Clean) | Ok(WindowsProcessScan::DeadlineReached) => Ok(None),
                Ok(WindowsProcessScan::Forbidden(found)) => Ok(Some(found)),
                Ok(WindowsProcessScan::ChildExited) => {
                    return finish_monitored_root_exit(
                        &mut child,
                        _process_tree_guard,
                        process_monitor.exempt,
                        drain,
                    );
                }
                Ok(WindowsProcessScan::MissingCommandLine { .. }) => {
                    unreachable!("retry controller must resolve missing command-line metadata")
                }
                #[cfg(test)]
                Ok(WindowsProcessScan::MissingProcessIdentity { .. }) => {
                    unreachable!("retry controller must resolve process identity metadata")
                }
                #[cfg(test)]
                Ok(WindowsProcessScan::UnverifiableAncestry { .. }) => {
                    unreachable!("retry controller must reject ambiguous process ancestry")
                }
                Err(error) => Err(error),
            };
            #[cfg(unix)]
            let mut process_scan = process_scan;
            #[cfg(not(any(unix, windows)))]
            let mut process_scan: Result<Option<ForbiddenProcess>> =
                Err(process_inspection_unavailable_error());

            #[cfg(unix)]
            let root_was_missing = process_scan
                .as_ref()
                .err()
                .is_some_and(|error| error.downcast_ref::<MonitoredRootNotVisible>().is_some());
            #[cfg(unix)]
            if root_was_missing {
                for _ in 0..ROOT_VISIBILITY_RETRIES {
                    match child.try_wait() {
                        Ok(Some(_)) => {
                            root_reaped = true;
                            match find_forbidden_descendant_tracking(
                                child.id(),
                                process_monitor.exempt,
                                process_monitor.shadow_dir,
                                process_monitor.tracking_token,
                                &mut observed_process_groups,
                            ) {
                                Ok(scan) => {
                                    process_scan = Ok(scan);
                                    break;
                                }
                                Err(error)
                                    if error
                                        .downcast_ref::<MonitoredRootNotVisible>()
                                        .is_some() =>
                                {
                                    process_scan = Ok(None);
                                    break;
                                }
                                Err(error) => {
                                    process_scan = Err(error);
                                    break;
                                }
                            }
                        }
                        Ok(None) => {
                            std::thread::sleep(Duration::from_millis(WAIT_POLL_INTERVAL_MS));
                            match find_forbidden_descendant_tracking(
                                child.id(),
                                process_monitor.exempt,
                                process_monitor.shadow_dir,
                                process_monitor.tracking_token,
                                &mut observed_process_groups,
                            ) {
                                Ok(scan) => {
                                    process_scan = Ok(scan);
                                    break;
                                }
                                Err(error)
                                    if error
                                        .downcast_ref::<MonitoredRootNotVisible>()
                                        .is_some() =>
                                {
                                    continue;
                                }
                                Err(error) => {
                                    process_scan = Err(error);
                                    break;
                                }
                            }
                        }
                        Err(error) => {
                            process_scan = Err(anyhow::Error::from(error));
                            break;
                        }
                    }
                }
            }

            #[cfg(unix)]
            if process_scan
                .as_ref()
                .err()
                .is_some_and(|error| error.downcast_ref::<MonitoredRootNotVisible>().is_some())
                && matches!(child.try_wait(), Ok(Some(_)))
            {
                root_reaped = true;
                // Re-scan the isolated group after observing exit to close the final status race.
                // Quét lại process group cô lập sau khi thấy child thoát để khép race status cuối.
                process_scan = match find_forbidden_descendant_tracking(
                    child.id(),
                    process_monitor.exempt,
                    process_monitor.shadow_dir,
                    process_monitor.tracking_token,
                    &mut observed_process_groups,
                ) {
                    Ok(scan) => Ok(scan),
                    Err(error) if error.downcast_ref::<MonitoredRootNotVisible>().is_some() => {
                        Ok(None)
                    }
                    Err(error) => Err(error),
                };
            }

            match process_scan {
                Ok(Some(found)) => {
                    #[cfg(unix)]
                    terminate_process_tree_tracked(
                        child.id(),
                        &mut observed_process_groups,
                        process_monitor.tracking_token,
                        process_monitor.exempt,
                        process_monitor.shadow_dir,
                        root_reaped,
                    );
                    #[cfg(windows)]
                    {
                        let _ = _process_tree_guard.terminate();
                        drop(_process_tree_guard);
                    }
                    let _ = child.kill();
                    let (out, _) =
                        drain(&mut child, Some(Duration::from_millis(POST_KILL_DRAIN_MS)));
                    return Err(forbidden_child_error(
                        &found,
                        out.status,
                        &out.stderr,
                        &out.stdout,
                    ));
                }
                Ok(None) => {}
                Err(error) => {
                    #[cfg(unix)]
                    terminate_process_tree_tracked(
                        child.id(),
                        &mut observed_process_groups,
                        process_monitor.tracking_token,
                        process_monitor.exempt,
                        process_monitor.shadow_dir,
                        root_reaped,
                    );
                    #[cfg(windows)]
                    {
                        let _ = _process_tree_guard.terminate();
                        drop(_process_tree_guard);
                    }
                    let _ = child.kill();
                    let (out, _) =
                        drain(&mut child, Some(Duration::from_millis(POST_KILL_DRAIN_MS)));
                    return Err(anyhow::anyhow!(
                        "cannot verify child process tree; command was terminated fail-closed (status after kill: {}): {error}",
                        out.status
                    ));
                }
            }
        }
        if timeout.is_some_and(|timeout| started.elapsed() >= timeout) {
            #[cfg(unix)]
            terminate_process_tree_tracked(
                child.id(),
                &mut observed_process_groups,
                process_monitor.tracking_token,
                process_monitor.exempt,
                process_monitor.shadow_dir,
                root_reaped,
            );
            #[cfg(windows)]
            {
                let _ = _process_tree_guard.terminate();
                drop(_process_tree_guard);
            }
            let _ = child.kill();
            let (out, _) = drain(&mut child, Some(Duration::from_millis(POST_KILL_DRAIN_MS)));
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

#[cfg(windows)]
fn finish_monitored_root_exit(
    child: &mut std::process::Child,
    guard: ProcessTreeGuard,
    exempt: &[&str],
    drain: impl Fn(&mut std::process::Child, Option<Duration>) -> (ExecOutcome, bool),
) -> Result<ExecOutcome> {
    let survivors = windows_job_descendants(&guard, child.id(), exempt)?;
    if survivors.is_empty() {
        let active_process_ids = guard.active_process_ids()?;
        if windows_job_has_active_descendants(child.id(), &active_process_ids) {
            let termination = guard.terminate();
            drop(guard);
            let (out, _) = drain(child, Some(Duration::from_millis(POST_KILL_DRAIN_MS)));
            if let Err(error) = termination {
                return Err(error.context("failed to terminate late Windows job descendants"));
            }
            let child_ids = active_process_ids
                .iter()
                .copied()
                .filter(|process_id| *process_id != child.id())
                .map(|process_id| process_id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "command root exited while uninspected child process(es) {child_ids} appeared in its Windows job; the process tree was terminated (status: {})",
                out.status
            );
        }
        drop(guard);
        return Ok(drain(child, Some(Duration::from_millis(POST_KILL_DRAIN_MS))).0);
    }

    let termination = guard.terminate();
    drop(guard);
    let (out, _) = drain(child, Some(Duration::from_millis(POST_KILL_DRAIN_MS)));
    if let Some((pid, Some(name))) = survivors.iter().find(|(_, forbidden)| forbidden.is_some()) {
        return Err(forbidden_child_error(
            &ForbiddenProcess {
                pid: *pid,
                name: name.clone(),
            },
            out.status,
            &out.stderr,
            &out.stdout,
        ));
    }
    if let Err(error) = termination {
        return Err(error.context("failed to terminate surviving Windows child processes"));
    }
    let child_ids = survivors
        .iter()
        .map(|(pid, _)| pid.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "command root exited while child process(es) {child_ids} remained in its Windows job; the process tree was terminated"
    )
}

struct ExecOutcome {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

// Process guard state; Unix uses a process group, Windows owns a Job Object.
// Trạng thái guard: Unix dùng session riêng; Windows quản lý bằng Job Object.
#[cfg(unix)]
struct ProcessTreeGuard;

#[cfg(windows)]
struct WindowsOwnedHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for WindowsOwnedHandle {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
impl WindowsOwnedHandle {
    #[allow(unsafe_code)]
    fn has_terminated(&self) -> Result<bool> {
        use windows_sys::Win32::Foundation::STILL_ACTIVE;
        use windows_sys::Win32::System::Threading::GetExitCodeProcess;

        let mut exit_code = 0;
        // SAFETY: this handle is open with PROCESS_QUERY_LIMITED_INFORMATION access.
        // AN TOÀN: handle đang mở với quyền PROCESS_QUERY_LIMITED_INFORMATION.
        let queried = unsafe { GetExitCodeProcess(self.0, &mut exit_code) };
        if queried == 0 {
            bail!(
                "failed to inspect Windows child process status (error {})",
                windows_last_error()
            );
        }
        Ok(exit_code != STILL_ACTIVE as u32)
    }
}

#[cfg(windows)]
/// Closing this job handle kills its assigned process tree by OS policy.
/// Đóng handle job sẽ dừng cây tiến trình đã gắn theo chính sách của OS.
/// Source: https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects
/// Nguồn: tài liệu Microsoft xác nhận KILL_ON_JOB_CLOSE kết thúc toàn bộ job.
struct ProcessTreeGuard {
    job: WindowsOwnedHandle,
}

#[cfg(not(any(unix, windows)))]
struct ProcessTreeGuard;

#[cfg(unix)]
fn configure_process_isolation(command: &mut Command) -> Result<ProcessTreeGuard> {
    use std::os::unix::process::CommandExt;

    // A private session prevents descendants from joining caller-owned groups.
    // Session riêng ngăn descendant nhập process group thuộc caller.
    // SAFETY: the pre-exec closure only calls async-signal-safe `setsid` after fork.
    // AN TOÀN: closure pre-exec chỉ gọi `setsid` async-signal-safe sau fork.
    #[allow(unsafe_code)]
    unsafe {
        command.pre_exec(|| {
            // SAFETY: `setsid` changes only the child process's session before exec.
            // AN TOÀN: `setsid` chỉ đổi session của child trước khi exec.
            let session_id = libc::setsid();
            if session_id == -1 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    Ok(ProcessTreeGuard)
}

#[cfg(windows)]
fn configure_process_isolation(command: &mut Command) -> Result<ProcessTreeGuard> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;

    // Keep the primary thread paused until its process is assigned to the job.
    // Tiến trình con chưa chạy cho tới khi đã được gắn vào job quản lý cây.
    // Source: https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags
    // Nguồn: tài liệu Microsoft mô tả CREATE_SUSPENDED giữ thread chính trước khi chạy.
    command.creation_flags(CREATE_SUSPENDED);
    ProcessTreeGuard::new()
}

#[cfg(not(any(unix, windows)))]
fn configure_process_isolation(_command: &mut Command) -> Result<ProcessTreeGuard> {
    Ok(ProcessTreeGuard)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn windows_last_error() -> u32 {
    unsafe { windows_sys::Win32::Foundation::GetLastError() }
}

#[cfg(windows)]
impl ProcessTreeGuard {
    #[allow(unsafe_code)]
    fn new() -> Result<Self> {
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };

        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            bail!(
                "failed to create Windows process job (error {})",
                windows_last_error()
            );
        }
        let guard = Self {
            job: WindowsOwnedHandle(handle),
        };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                guard.job.0,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            bail!(
                "failed to configure Windows process job (error {})",
                windows_last_error()
            );
        }
        Ok(guard)
    }

    #[allow(unsafe_code)]
    fn activate(&self, child: &std::process::Child) -> Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

        let assigned =
            unsafe { AssignProcessToJobObject(self.job.0, child.as_raw_handle().cast()) };
        if assigned == 0 {
            bail!(
                "failed to assign suspended child to Windows process job (error {})",
                windows_last_error()
            );
        }
        self.resume_primary_thread(child.id())
    }

    #[allow(unsafe_code)]
    fn resume_primary_thread(&self, process_id: u32) -> Result<()> {
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        };
        use windows_sys::Win32::System::Threading::{
            OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
        };

        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            bail!(
                "failed to inspect suspended Windows child thread (error {})",
                windows_last_error()
            );
        }
        let snapshot = WindowsOwnedHandle(snapshot);
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut found_thread = None;
        let mut has_entry = unsafe { Thread32First(snapshot.0, &mut entry) } != 0;
        while has_entry {
            if entry.th32OwnerProcessID == process_id {
                found_thread = Some(entry.th32ThreadID);
                break;
            }
            has_entry = unsafe { Thread32Next(snapshot.0, &mut entry) } != 0;
        }
        let thread_id = found_thread.ok_or_else(|| {
            anyhow::anyhow!("suspended Windows child has no discoverable primary thread")
        })?;
        let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, thread_id) };
        if thread.is_null() {
            bail!(
                "failed to open suspended Windows child thread (error {})",
                windows_last_error()
            );
        }
        let thread = WindowsOwnedHandle(thread);
        let previous_suspend_count = unsafe { ResumeThread(thread.0) };
        if previous_suspend_count == u32::MAX {
            bail!(
                "failed to resume Windows child thread (error {})",
                windows_last_error()
            );
        }
        if previous_suspend_count != 1 {
            bail!("Windows child thread had an unexpected suspend count: {previous_suspend_count}");
        }
        Ok(())
    }

    #[allow(unsafe_code)]
    fn terminate(&self) -> Result<()> {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;

        let terminated = unsafe { TerminateJobObject(self.job.0, 1) };
        if terminated == 0 {
            bail!(
                "failed to terminate Windows process job (error {})",
                windows_last_error()
            );
        }
        Ok(())
    }

    #[allow(unsafe_code)]
    fn active_process_ids(&self) -> Result<Vec<u32>> {
        use windows_sys::Win32::System::JobObjects::{
            JobObjectBasicProcessIdList, QueryInformationJobObject,
        };

        #[repr(C)]
        struct ProcessIdListBuffer {
            assigned: u32,
            listed: u32,
            process_ids: [usize; 1],
        }

        let mut capacity = 32usize;
        for _ in 0..4 {
            let word_count = 2usize.saturating_add(capacity);
            let mut buffer = vec![0usize; word_count];
            let list = buffer.as_mut_ptr().cast::<ProcessIdListBuffer>();
            let byte_len = std::mem::size_of::<u32>()
                .saturating_mul(2)
                .saturating_add(capacity.saturating_mul(std::mem::size_of::<usize>()));
            let byte_len = u32::try_from(byte_len)
                .context("Windows job process ID buffer exceeds API size limit")?;
            // SAFETY: `buffer` is aligned for usize, reserves the fixed two-u32 header plus
            // `capacity` process IDs, and stays alive for the synchronous Windows API call.
            // AN TOÀN: `buffer` căn chỉnh theo usize, đủ header hai u32 và `capacity` PID,
            // tồn tại suốt lời gọi Windows đồng bộ.
            let queried = unsafe {
                QueryInformationJobObject(
                    self.job.0,
                    JobObjectBasicProcessIdList,
                    list.cast(),
                    byte_len,
                    std::ptr::null_mut(),
                )
            };
            if queried == 0 {
                let error = windows_last_error();
                if error == 234 {
                    capacity = capacity.saturating_mul(2);
                    continue;
                }
                bail!("failed to inspect Windows job process IDs (error {error})");
            }

            // SAFETY: the successful query wrote the documented header into this aligned buffer.
            // AN TOÀN: truy vấn thành công đã ghi header theo tài liệu vào buffer được căn chỉnh.
            let (assigned, listed) =
                unsafe { ((*list).assigned as usize, (*list).listed as usize) };
            if listed > capacity {
                bail!("Windows job returned an invalid process ID count");
            }
            if assigned > listed {
                capacity = assigned.max(capacity.saturating_mul(2));
                continue;
            }

            // SAFETY: `process_ids` begins immediately after the two-u32 header and `listed`
            // was checked against the allocated capacity above.
            // AN TOÀN: `process_ids` nằm ngay sau header hai u32; `listed` đã được kiểm tra.
            let ids = unsafe {
                std::slice::from_raw_parts(
                    std::ptr::addr_of!((*list).process_ids).cast::<usize>(),
                    listed,
                )
            };
            return ids
                .iter()
                .map(|pid| {
                    u32::try_from(*pid)
                        .map_err(|_| anyhow::anyhow!("Windows job returned an invalid process ID"))
                })
                .collect();
        }
        bail!("Windows job process ID list remained incomplete after bounded retries")
    }

    #[allow(unsafe_code)]
    fn capture_job_member_process(&self, process_id: u32) -> Result<Option<WindowsOwnedHandle>> {
        use windows_sys::Win32::System::JobObjects::IsProcessInJob;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        // SAFETY: this opens only the PID returned by our private Job Object query and requests
        // query-only access; an invalid PID means that process exited during the snapshot.
        // AN TOÀN: chỉ mở PID do Job Object riêng trả về với quyền query; PID sai nghĩa là process
        // đã thoát trong lúc chụp trạng thái.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if process.is_null() {
            let error = windows_last_error();
            if error == 87 {
                if !self.active_process_ids()?.contains(&process_id) {
                    // A fresh Job query confirms this PID is no longer an active member.
                    // Truy vấn Job mới xác nhận PID này không còn là thành viên đang chạy.
                    return Ok(None);
                }
                bail!(
                    "cannot capture Windows job child PID {process_id}: it remains active but unavailable before identity verification"
                );
            }
            bail!("failed to open Windows job child PID {process_id} (error {error})");
        }
        let process = WindowsOwnedHandle(process);
        let mut is_member = 0;
        // SAFETY: both handles are live and `is_member` points to writable BOOL storage.
        // AN TOÀN: cả hai handle còn sống và `is_member` trỏ tới vùng BOOL có thể ghi.
        let queried = unsafe { IsProcessInJob(process.0, self.job.0, &mut is_member) };
        if queried == 0 {
            bail!(
                "failed to verify Windows job child PID {process_id} membership (error {})",
                windows_last_error()
            );
        }
        if is_member == 0 {
            bail!(
                "cannot capture Windows job child PID {process_id}: membership changed before identity verification"
            );
        }
        // Keep this handle alive through metadata capture so Windows cannot reuse its PID.
        // Giữ handle này tới khi chụp metadata để Windows không tái sử dụng PID.
        Ok(Some(process))
    }
}

#[cfg(windows)]
fn windows_job_descendants(
    guard: &ProcessTreeGuard,
    root_pid: u32,
    exempt: &[&str],
) -> Result<Vec<(u32, Option<String>)>> {
    let inspection = inspect_windows_job(guard, root_pid, exempt)?;
    if let Some((pid, image_name)) = inspection.missing_command_line {
        validate_monitored_child_command_line(&image_name, &[], pid)?;
    }
    Ok(inspection.descendants)
}

/// Inspect process IDs owned by this private Job Object.
/// Chỉ kiểm tra các PID thuộc Job Object riêng của lệnh đang chạy.
#[cfg(windows)]
fn inspect_windows_job(
    guard: &ProcessTreeGuard,
    root_pid: u32,
    exempt: &[&str],
) -> Result<WindowsJobInspection> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

    let process_ids = guard.active_process_ids()?;
    if process_ids.is_empty() {
        return Ok(WindowsJobInspection::default());
    }
    let mut system = System::new();
    let captured_snapshots = capture_windows_job_process_snapshots(
        root_pid,
        &process_ids,
        |process_id| {
            Ok(match guard.capture_job_member_process(process_id)? {
                Some(process) => WindowsJobMemberCapture::Captured(process),
                None => WindowsJobMemberCapture::Exited,
            })
        },
        |process_id, identity_guard| {
            let process_id = Pid::from_u32(process_id);
            let process_ids = [process_id];
            for _ in 0..2 {
                // Capture each member's command line immediately after verifying its Job
                // membership, before opening handles for other short-lived descendants.
                // Chụp command line ngay sau khi xác minh membership Job, trước khi mở handle
                // cho các tiến trình con ngắn hạn khác.
                system.refresh_processes_specifics(
                    ProcessesToUpdate::Some(&process_ids),
                    true,
                    ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
                );
                if let Some(process) = system.process(process_id) {
                    return Ok(WindowsJobMemberMetadata::Available(
                        WindowsProcessSnapshot {
                            pid: process_id.as_u32(),
                            #[cfg(test)]
                            parent_pid: None,
                            #[cfg(test)]
                            creation_time: None,
                            image_name: process.name().to_string_lossy().into_owned(),
                            command: process.cmd().to_vec(),
                        },
                    ));
                }
                if identity_guard.has_terminated()? {
                    // Live descendants remain separate members of this Job and are inspected in turn.
                    // Descendant còn sống vẫn là thành viên riêng của Job và sẽ được kiểm tra tiếp.
                    return Ok(WindowsJobMemberMetadata::Exited);
                }
            }
            if identity_guard.has_terminated()? {
                return Ok(WindowsJobMemberMetadata::Exited);
            }
            Ok(WindowsJobMemberMetadata::Unavailable)
        },
        || guard.active_process_ids(),
    )?;
    let captured_job_process_ids = captured_snapshots
        .members
        .iter()
        .map(|(process_id, _)| *process_id)
        .collect::<Vec<_>>();
    inspect_windows_job_processes(
        root_pid,
        &captured_job_process_ids,
        &captured_snapshots.snapshots,
        exempt,
    )
}

/// Capture confirmed Job Object members before metadata refresh and retain their identity evidence.
/// Chụp thành viên Job Object đã xác nhận trước khi refresh metadata và giữ bằng chứng danh tính.
#[cfg(test)]
fn capture_windows_job_members<T>(
    root_pid: u32,
    job_process_ids: &[u32],
    mut capture_member: impl FnMut(u32) -> Result<T>,
) -> Result<Vec<(u32, T)>> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let mut captured = Vec::new();
    for &process_id in job_process_ids {
        if process_id == root_pid || !seen.insert(process_id) {
            continue;
        }
        let identity_guard = capture_member(process_id)?;
        captured.push((process_id, identity_guard));
    }
    Ok(captured)
}

/// Capture each Job member and its command-line snapshot as one ordered operation.
/// Chụp từng thành viên Job và command line thành một thao tác liền nhau.
#[cfg(any(windows, test))]
fn capture_windows_job_process_snapshots<T>(
    root_pid: u32,
    job_process_ids: &[u32],
    mut capture_member: impl FnMut(u32) -> Result<WindowsJobMemberCapture<T>>,
    mut snapshot_member: impl FnMut(u32, &T) -> Result<WindowsJobMemberMetadata>,
    mut refresh_members: impl FnMut() -> Result<Vec<u32>>,
) -> Result<CapturedWindowsJobSnapshots<T>> {
    use std::collections::HashSet;

    let mut inspected = HashSet::from([root_pid]);
    let mut pending = job_process_ids.to_vec();
    let mut captured = Vec::new();
    let mut _retained_exited_members = Vec::new();
    let mut snapshots = Vec::new();
    for attempt in 0..=WINDOWS_JOB_MEMBER_SNAPSHOT_RETRIES {
        let mut attempted_this_pass = HashSet::new();
        for process_id in pending.drain(..) {
            if inspected.contains(&process_id) || !attempted_this_pass.insert(process_id) {
                continue;
            }
            let identity_guard = match capture_member(process_id)? {
                WindowsJobMemberCapture::Captured(identity_guard) => identity_guard,
                WindowsJobMemberCapture::Exited => continue,
            };
            match snapshot_member(process_id, &identity_guard)? {
                WindowsJobMemberMetadata::Available(snapshot) => {
                    inspected.insert(process_id);
                    captured.push((process_id, identity_guard));
                    snapshots.push(snapshot);
                }
                WindowsJobMemberMetadata::Exited => {
                    // Retain the process handle so its PID cannot be recycled during re-enumeration.
                    // Giữ handle để PID không bị tái sử dụng trong lúc liệt kê lại Job.
                    inspected.insert(process_id);
                    _retained_exited_members.push(identity_guard);
                }
                WindowsJobMemberMetadata::Unavailable => {
                    bail!("cannot inspect captured Windows job child PID {process_id}");
                }
            }
        }

        pending = refresh_members()?
            .into_iter()
            .filter(|process_id| !inspected.contains(process_id))
            .collect();
        if pending.is_empty() {
            return Ok(CapturedWindowsJobSnapshots {
                members: captured,
                _retained_exited_members,
                snapshots,
            });
        }
        if attempt == WINDOWS_JOB_MEMBER_SNAPSHOT_RETRIES {
            bail!("Windows Job membership kept changing during process inspection");
        }
    }
    unreachable!("bounded Windows Job snapshot loop must return")
}

/// Inspect snapshots for members confirmed before refresh; do not recheck PID membership afterward.
/// Kiểm snapshot của thành viên đã xác nhận trước refresh; không kiểm tra lại membership sau đó.
#[cfg(any(windows, test))]
fn inspect_windows_job_processes(
    root_pid: u32,
    job_process_ids: &[u32],
    processes: &[WindowsProcessSnapshot],
    exempt: &[&str],
) -> Result<WindowsJobInspection> {
    use std::collections::{HashMap, HashSet};

    let processes_by_pid = processes
        .iter()
        .map(|process| (process.pid, process))
        .collect::<HashMap<_, _>>();
    let mut inspected = WindowsJobInspection::default();
    let mut seen = HashSet::new();
    for &process_id in job_process_ids {
        if process_id == root_pid || !seen.insert(process_id) {
            continue;
        }
        let Some(process) = processes_by_pid.get(&process_id) else {
            bail!("cannot inspect captured Windows job child PID {process_id}");
        };
        let command_line = process
            .command
            .iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        let image_name = process.image_name.clone();
        if command_line.is_empty() && inspected.missing_command_line.is_none() {
            inspected.missing_command_line = Some((process_id, image_name.clone()));
        }
        inspected.descendants.push((
            process_id,
            forbidden_process_name(&image_name, &command_line, exempt),
        ));
    }
    Ok(inspected)
}

#[cfg(any(windows, test))]
enum WindowsJobMemberMetadata {
    Available(WindowsProcessSnapshot),
    Exited,
    Unavailable,
}

#[cfg(any(windows, test))]
enum WindowsJobMemberCapture<T> {
    Captured(T),
    Exited,
}

#[cfg(any(windows, test))]
fn windows_job_has_active_descendants(root_pid: u32, process_ids: &[u32]) -> bool {
    process_ids.iter().any(|process_id| *process_id != root_pid)
}

#[cfg(any(windows, test))]
struct CapturedWindowsJobSnapshots<T> {
    members: Vec<(u32, T)>,
    _retained_exited_members: Vec<T>,
    snapshots: Vec<WindowsProcessSnapshot>,
}

#[cfg(any(windows, test))]
#[derive(Default)]
struct WindowsJobInspection {
    descendants: Vec<(u32, Option<String>)>,
    missing_command_line: Option<(u32, String)>,
}

#[cfg(any(windows, test))]
fn windows_job_scan_from_inspection(inspection: WindowsJobInspection) -> WindowsProcessScan {
    if let Some((pid, Some(name))) = inspection
        .descendants
        .into_iter()
        .find(|(_, forbidden)| forbidden.is_some())
    {
        return WindowsProcessScan::Forbidden(ForbiddenProcess { pid, name });
    }
    if let Some((pid, image_name)) = inspection.missing_command_line {
        return WindowsProcessScan::MissingCommandLine { pid, image_name };
    }
    WindowsProcessScan::Clean
}

#[cfg(all(unix, test))]
fn terminate_process_tree(root_pid: u32, observed_process_groups: &std::collections::HashSet<u32>) {
    let mut groups = observed_process_groups.clone();
    groups.insert(root_pid);
    signal_process_group_ids(&groups, libc::SIGTERM);
    std::thread::sleep(Duration::from_millis(TERM_TO_KILL_GRACE_MS));
    signal_process_group_ids(&groups, libc::SIGKILL);
}

#[cfg(unix)]
struct ProcessSignalPlan {
    process_groups: std::collections::HashSet<u32>,
    signal_root_directly: bool,
}

#[cfg(unix)]
fn process_signal_plan(
    root_pid: u32,
    observed_process_groups: &std::collections::HashSet<u32>,
    live_process_groups: &std::collections::HashSet<u32>,
    root_group_id: Option<u32>,
    root_reaped: bool,
) -> ProcessSignalPlan {
    let mut process_groups = observed_process_groups
        .intersection(live_process_groups)
        .copied()
        .collect::<std::collections::HashSet<_>>();
    if !root_reaped {
        for group in [Some(root_pid), root_group_id].into_iter().flatten() {
            if live_process_groups.contains(&group) {
                process_groups.insert(group);
            }
        }
    }
    ProcessSignalPlan {
        process_groups,
        signal_root_directly: !root_reaped
            && root_group_id.is_none_or(|group| !live_process_groups.contains(&group)),
    }
}

#[cfg(unix)]
fn terminate_process_tree_tracked(
    root_pid: u32,
    observed_process_groups: &mut std::collections::HashSet<u32>,
    tracking_token: &str,
    exempt: &[&str],
    shadow_dir: &Path,
    root_reaped: bool,
) {
    signal_process_groups(
        root_pid,
        observed_process_groups,
        libc::SIGTERM,
        root_reaped,
    );
    let mut term_signaled_groups = observed_process_groups.clone();
    let term_deadline = Instant::now() + Duration::from_millis(TERM_TO_KILL_GRACE_MS);
    loop {
        let previous_groups = observed_process_groups.len();
        let _ = find_forbidden_descendant_tracking(
            root_pid,
            exempt,
            shadow_dir,
            tracking_token,
            observed_process_groups,
        );
        let new_groups = observed_process_groups
            .difference(&term_signaled_groups)
            .copied()
            .collect::<std::collections::HashSet<_>>();
        signal_process_group_ids(&new_groups, libc::SIGTERM);
        term_signaled_groups.extend(new_groups);
        if Instant::now() >= term_deadline {
            break;
        }
        let sleep_for = if observed_process_groups.len() != previous_groups {
            Duration::from_millis(1)
        } else {
            Duration::from_millis(10)
        };
        std::thread::sleep(sleep_for.min(term_deadline.saturating_duration_since(Instant::now())));
    }

    signal_process_groups(
        root_pid,
        observed_process_groups,
        libc::SIGKILL,
        root_reaped,
    );
    let kill_deadline = Instant::now() + Duration::from_millis(TERM_TO_KILL_GRACE_MS);
    loop {
        let _ = find_forbidden_descendant_tracking(
            root_pid,
            exempt,
            shadow_dir,
            tracking_token,
            observed_process_groups,
        );
        signal_process_groups(
            root_pid,
            observed_process_groups,
            libc::SIGKILL,
            root_reaped,
        );
        if !process_groups_have_descendants(observed_process_groups, root_pid)
            || Instant::now() >= kill_deadline
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn signal_process_groups(
    root_pid: u32,
    process_groups: &std::collections::HashSet<u32>,
    signal: libc::c_int,
    root_reaped: bool,
) {
    let mut candidate_groups = process_groups.clone();
    let root_group_id = if root_reaped {
        None
    } else {
        process_group_id(root_pid)
    };
    if !root_reaped {
        candidate_groups.insert(root_pid);
        if let Some(root_group_id) = root_group_id {
            candidate_groups.insert(root_group_id);
        }
    }
    let live_process_groups = candidate_groups
        .iter()
        .copied()
        .filter(|group| process_group_is_live(*group))
        .collect::<std::collections::HashSet<_>>();
    let plan = process_signal_plan(
        root_pid,
        process_groups,
        &live_process_groups,
        root_group_id,
        root_reaped,
    );
    signal_process_group_ids(&plan.process_groups, signal);
    if plan.signal_root_directly {
        // The root PID remains reserved until `Child::wait` reaps it.
        // PID root chưa thể được tái sử dụng cho đến khi `Child::wait` reap.
        // SAFETY: this PID belongs to our unreaped direct child.
        // AN TOÀN: PID này thuộc child trực tiếp do runner spawn và chưa reap.
        #[allow(unsafe_code)]
        unsafe {
            if let Ok(root_pid) = libc::pid_t::try_from(root_pid) {
                libc::kill(root_pid, signal);
            }
        }
    }
}

#[cfg(unix)]
fn signal_process_group_ids(groups: &std::collections::HashSet<u32>, signal: libc::c_int) {
    let groups = groups
        .iter()
        .copied()
        .filter(|group| process_group_is_live(*group))
        .filter_map(|group| libc::pid_t::try_from(group).ok())
        .collect::<Vec<_>>();
    // Every target is a process group previously observed under this runner.
    // Mọi đích đều là process group riêng đã quan sát dưới runner này.
    // SAFETY: negative PIDs target only process groups observed under this runner.
    // AN TOÀN: PID âm chỉ nhắm các process group đã được quan sát thuộc runner này.
    #[allow(unsafe_code)]
    unsafe {
        for group in groups {
            libc::kill(-group, signal);
        }
    }
}

#[cfg(unix)]
fn process_group_is_live(group: u32) -> bool {
    let Ok(group) = libc::pid_t::try_from(group) else {
        return false;
    };
    // Recheck that the numeric PGID still names a live group before signaling it.
    // Kiểm tra PGID bằng số vẫn còn là group sống ngay trước khi gửi signal.
    // SAFETY: signal zero only probes existence and permission; it changes no process state.
    // AN TOÀN: signal 0 chỉ dò tồn tại và quyền truy cập, không đổi trạng thái process.
    #[allow(unsafe_code)]
    let result = unsafe { libc::kill(-group, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(any(windows, test))]
/// Keep untrusted batch arguments outside cmd.exe's expansion and operator syntax.
/// Chặn args không tin cậy khỏi cú pháp expansion/toán tử của cmd.exe.
fn validate_windows_batch_invocation(script_path: &str, args: &[String]) -> Result<()> {
    let contains_cmd_metacharacter = |value: &str| {
        value.chars().any(|character| {
            matches!(
                character,
                '"' | '%' | '!' | '&' | '|' | '<' | '>' | '^' | '(' | ')' | '\n' | '\r'
            )
        })
    };
    if contains_cmd_metacharacter(script_path)
        || args.iter().any(|arg| contains_cmd_metacharacter(arg))
    {
        bail!("batch-script command contains unsupported shell metacharacters");
    }
    Ok(())
}

#[cfg(windows)]
fn windows_system_tool_path(name: &str) -> Result<PathBuf> {
    let system_root = std::env::var_os("SystemRoot")
        .ok_or_else(|| anyhow::anyhow!("SystemRoot is unavailable"))?;
    let path = PathBuf::from(system_root).join("System32").join(name);
    if !path.is_absolute() || !path.is_file() {
        bail!("Windows system tool is unavailable");
    }
    Ok(path)
}

#[cfg(windows)]
fn windows_batch_command(script_path: &str, args: &[String], cwd: &Path) -> Result<Command> {
    // Normalize only the interpreter argument; keep canonical paths for validation.
    // Chỉ chuẩn hóa path đưa vào shell; giữ canonical path cho bước kiểm tra.
    // `cmd.exe` does not accept the extended-length prefix produced by canonicalize().
    // `cmd.exe` không nhận prefix extended-length mà canonicalize() có thể trả về.
    let cmd_path = if let Some(unc) = script_path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else if let Some(local) = script_path.strip_prefix(r"\\?\") {
        local.to_string()
    } else {
        script_path.to_string()
    };
    validate_windows_batch_invocation(&cmd_path, args)?;

    // Use the fixed system interpreter path, never a caller-controlled PATH lookup.
    // Dùng đường dẫn interpreter cố định của hệ thống, không dò PATH từ caller.
    let mut command = Command::new(windows_system_tool_path("cmd.exe")?);
    command
        .arg("/D")
        .arg("/S")
        .arg("/C")
        .arg(cmd_path)
        .args(args)
        .current_dir(cwd);
    Ok(command)
}

#[derive(Debug, Clone)]
struct ForbiddenProcess {
    pid: u32,
    name: String,
}

#[cfg(unix)]
#[derive(Debug)]
struct MonitoredRootNotVisible(u32);

#[cfg(unix)]
impl std::fmt::Display for MonitoredRootNotVisible {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "process table did not contain monitored root PID {}",
            self.0
        )
    }
}

#[cfg(unix)]
impl std::error::Error for MonitoredRootNotVisible {}

#[cfg(all(unix, test))]
fn find_forbidden_descendant(
    root_pid: u32,
    exempt: &[&str],
    shadow_dir: &Path,
) -> Result<Option<ForbiddenProcess>> {
    let mut observed_process_groups = std::collections::HashSet::from([root_pid]);
    find_forbidden_descendant_tracking(
        root_pid,
        exempt,
        shadow_dir,
        "",
        &mut observed_process_groups,
    )
}

#[cfg(unix)]
fn find_forbidden_descendant_tracking(
    root_pid: u32,
    exempt: &[&str],
    shadow_dir: &Path,
    tracking_token: &str,
    observed_process_groups: &mut std::collections::HashSet<u32>,
) -> Result<Option<ForbiddenProcess>> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System, UpdateKind};

    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    let root = Pid::from_u32(root_pid);
    let live_process_groups = system
        .processes()
        .keys()
        .filter_map(|pid| process_group_id(pid.as_u32()))
        .collect::<std::collections::HashSet<_>>();
    observed_process_groups.retain(|process_group| live_process_groups.contains(process_group));
    let root_is_visible = system
        .process(root)
        .is_some_and(|process| process.status() != ProcessStatus::Zombie);
    if !root_is_visible {
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cmd(UpdateKind::Always)
                .with_environ(UpdateKind::Always),
        );
        // Reparented children may move into a new session; inspect every group observed
        // while the root was still visible, plus this invocation's inherited marker.
        // Rà group descendant đã ghi nhận và dấu nhận diện kế thừa từ lượt chạy này.
        let mut first_forbidden = None;
        let mut live_descendant_found = false;
        let known_groups = observed_process_groups.clone();
        for (pid, process) in system.processes() {
            if pid.as_u32() == root_pid {
                continue;
            }
            let Some(process_group) = process_group_id(pid.as_u32()) else {
                continue;
            };
            let carries_tracking_token = process.environ().iter().any(|entry| {
                entry == &OsString::from(format!("{PROCESS_TREE_TRACKING_ENV}={tracking_token}"))
            });
            if !known_groups.contains(&process_group) && !carries_tracking_token {
                continue;
            }
            live_descendant_found = true;
            observed_process_groups.insert(process_group);
            if first_forbidden.is_none() {
                first_forbidden = forbidden_process_entry(pid, process, exempt).filter(|found| {
                    !is_shadow_npm_version_probe(
                        process.name().to_string_lossy().as_bytes(),
                        process
                            .cmd()
                            .iter()
                            .map(|arg| arg.to_string_lossy())
                            .collect::<Vec<_>>()
                            .join(" ")
                            .as_bytes(),
                        shadow_dir,
                    ) || found.name != "npm"
                });
            }
        }
        if let Some(found) = first_forbidden {
            return Ok(Some(found));
        }
        if live_descendant_found {
            return Ok(None);
        }
        return Err(anyhow::Error::new(MonitoredRootNotVisible(root_pid)));
    }

    observed_process_groups.insert(root_pid);
    // Traverse native process metadata in-process so inspection cannot spawn a shell or ps.
    // Duyệt metadata process native ngay trong process để không cần spawn shell hay ps.
    let mut frontier = vec![root];
    let mut seen = std::collections::HashSet::new();
    let mut first_forbidden = None;
    while let Some(parent_pid) = frontier.pop() {
        if !seen.insert(parent_pid) {
            continue;
        }
        for (pid, process) in system
            .processes()
            .iter()
            .filter(|(_, process)| process.parent() == Some(parent_pid))
        {
            if let Some(process_group) = process_group_id(pid.as_u32()) {
                observed_process_groups.insert(process_group);
            }
            if first_forbidden.is_none() {
                first_forbidden = forbidden_process_entry(pid, process, exempt).filter(|found| {
                    let command_line = process
                        .cmd()
                        .iter()
                        .map(|arg| arg.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(" ");
                    found.name != "npm"
                        || !is_shadow_npm_version_probe(
                            process.name().to_string_lossy().as_bytes(),
                            command_line.as_bytes(),
                            shadow_dir,
                        )
                });
            }
            frontier.push(*pid);
        }
    }
    Ok(first_forbidden)
}

#[cfg(unix)]
fn process_group_id(pid: u32) -> Option<u32> {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return None;
    };
    // SAFETY: `pid` comes from the OS process table; getpgid only reads its group membership.
    // AN TOÀN: `pid` lấy từ process table của OS; getpgid chỉ đọc process group của PID đó.
    #[allow(unsafe_code)]
    let process_group = unsafe { libc::getpgid(pid) };
    u32::try_from(process_group).ok()
}

#[cfg(all(unix, test))]
fn process_group_matches(pid: &sysinfo::Pid, group_leader_pid: u32) -> bool {
    process_group_id(pid.as_u32()) == Some(group_leader_pid)
}

#[cfg(unix)]
fn process_groups_have_descendants(
    observed_process_groups: &std::collections::HashSet<u32>,
    root_pid: u32,
) -> bool {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    system.processes().iter().any(|(pid, _)| {
        pid.as_u32() != root_pid
            && process_group_id(pid.as_u32())
                .is_some_and(|group| observed_process_groups.contains(&group))
    })
}

#[cfg(unix)]
fn forbidden_process_entry(
    pid: &sysinfo::Pid,
    process: &sysinfo::Process,
    exempt: &[&str],
) -> Option<ForbiddenProcess> {
    let image_name = process.name().to_string_lossy().into_owned();
    let command_line = process
        .cmd()
        .iter()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    forbidden_process_name(&image_name, &command_line, exempt).map(|name| ForbiddenProcess {
        pid: pid.as_u32(),
        name,
    })
}

#[cfg(unix)]
#[cfg(test)]
fn find_forbidden_descendant_with_program(
    root_pid: u32,
    exempt: &[&str],
    shadow_dir: &Path,
    inspector: &Path,
) -> Result<Option<ForbiddenProcess>> {
    let output = Command::new(inspector)
        .args(["-axo", "pid=,ppid=,comm=,command="])
        .output()
        .with_context(|| {
            format!(
                "cannot start process-table inspector '{}'",
                inspector.display()
            )
        })?;
    if !output.status.success() {
        bail!(
            "process-table inspector '{}' exited with {}",
            inspector.display(),
            output.status
        );
    }

    inspect_forbidden_process_table(root_pid, output, exempt, shadow_dir)
}

#[cfg(unix)]
#[cfg(test)]
fn inspect_forbidden_process_table(
    root_pid: u32,
    output: std::process::Output,
    exempt: &[&str],
    shadow_dir: &Path,
) -> Result<Option<ForbiddenProcess>> {
    let mut processes = Vec::new();
    let mut root_seen = false;
    for mut line in output.stdout.split(|byte| *byte == b'\n') {
        if line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let (pid_field, remainder) = next_process_table_field(line)
            .ok_or_else(|| anyhow::anyhow!("process-table row has no PID"))?;
        let pid = std::str::from_utf8(pid_field)
            .context("process-table row has a non-UTF-8 PID")?
            .parse::<u32>()
            .context("process-table row has an invalid PID")?;
        let (ppid_field, remainder) = next_process_table_field(remainder)
            .ok_or_else(|| anyhow::anyhow!("process-table row has no parent PID"))?;
        let ppid = std::str::from_utf8(ppid_field)
            .context("process-table row has a non-UTF-8 parent PID")?
            .parse::<u32>()
            .context("process-table row has an invalid parent PID")?;
        let (command, command_line) = next_process_table_field(remainder)
            .ok_or_else(|| anyhow::anyhow!("process-table row has no executable name"))?;
        root_seen |= pid == root_pid;
        processes.push((pid, ppid, command.to_vec(), command_line.to_vec()));
    }
    if !root_seen {
        bail!("process table did not contain monitored root PID {root_pid}");
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
            let command_text = String::from_utf8_lossy(command);
            let command_line_text = String::from_utf8_lossy(command_line);
            if let Some(name) = forbidden_process_name(&command_text, &command_line_text, exempt) {
                if name == "npm" && is_shadow_npm_version_probe(command, command_line, shadow_dir) {
                    frontier.push(*pid);
                    continue;
                }
                return Ok(Some(ForbiddenProcess { pid: *pid, name }));
            }
            frontier.push(*pid);
        }
    }

    Ok(None)
}

#[cfg(unix)]
#[cfg(test)]
fn next_process_table_field(line: &[u8]) -> Option<(&[u8], &[u8])> {
    let start = line.iter().position(|byte| !byte.is_ascii_whitespace())?;
    let line = &line[start..];
    let end = line
        .iter()
        .position(u8::is_ascii_whitespace)
        .unwrap_or(line.len());
    if end == 0 {
        return None;
    }

    let remainder = &line[end..];
    let remainder_start = remainder
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(remainder.len());
    Some((&line[..end], &remainder[remainder_start..]))
}

/// Inspect native Windows process metadata, without a shell or WMI command.
/// Kiểm metadata process Windows bằng API native, không shell hoặc lệnh WMI.
#[cfg(any(windows, test))]
struct WindowsProcessSnapshot {
    pid: u32,
    #[cfg(test)]
    parent_pid: Option<u32>,
    #[cfg(test)]
    creation_time: Option<u64>,
    image_name: String,
    command: Vec<OsString>,
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct WindowsProcessIdentity {
    creation_time: u64,
    image_name: String,
}

#[cfg(test)]
impl WindowsProcessSnapshot {
    fn identity(&self, creation_time: u64) -> WindowsProcessIdentity {
        WindowsProcessIdentity {
            creation_time,
            image_name: self.image_name.clone(),
        }
    }
}

#[cfg(test)]
#[derive(Default)]
struct WindowsProcessAncestry {
    known_descendant_instances: std::collections::HashMap<u32, WindowsProcessIdentity>,
    retired_pids: std::collections::HashSet<u32>,
}

#[cfg(any(windows, test))]
enum WindowsProcessScan {
    Clean,
    Forbidden(ForbiddenProcess),
    MissingCommandLine {
        pid: u32,
        image_name: String,
    },
    #[cfg(test)]
    MissingProcessIdentity {
        pid: u32,
    },
    #[cfg(test)]
    UnverifiableAncestry {
        parent_pid: u32,
        child_pid: u32,
    },
    ChildExited,
    DeadlineReached,
}

#[cfg(test)]
fn scan_windows_process_snapshot(
    root_pid: u32,
    ancestry: &mut WindowsProcessAncestry,
    processes: &[WindowsProcessSnapshot],
    exempt: &[&str],
    mut query_creation_time: impl FnMut(u32) -> Result<Option<u64>>,
) -> Result<WindowsProcessScan> {
    use std::collections::HashMap;

    let mut children_by_parent: HashMap<u32, Vec<&WindowsProcessSnapshot>> = HashMap::new();
    let mut processes_by_pid = HashMap::new();
    for process in processes {
        processes_by_pid.insert(process.pid, process);
        if let Some(parent_pid) = process.parent_pid {
            children_by_parent
                .entry(parent_pid)
                .or_default()
                .push(process);
        }
    }
    let Some(root_process) = processes_by_pid.get(&root_pid) else {
        bail!("process table did not contain monitored root PID {root_pid}");
    };
    let mut current_identities = HashMap::new();
    let Some(root_creation_time) = current_windows_process_creation_time(
        root_process,
        &mut query_creation_time,
        &mut current_identities,
    )?
    else {
        return Ok(WindowsProcessScan::MissingProcessIdentity { pid: root_pid });
    };
    let root_identity = root_process.identity(root_creation_time);
    if ancestry
        .known_descendant_instances
        .get(&root_pid)
        .is_some_and(|known| known != &root_identity)
    {
        bail!("monitored root PID {root_pid} changed process identity");
    }
    ancestry
        .known_descendant_instances
        .insert(root_pid, root_identity);
    ancestry.retired_pids.remove(&root_pid);

    // Match each remembered PID to its process instance before traversing its current children.
    // So danh tính instance cho PID đã lưu trước khi duyệt các tiến trình con hiện tại.
    let mut frontier = Vec::new();
    let mut verified_current_pids = std::collections::HashSet::new();
    for (&pid, known_identity) in &ancestry.known_descendant_instances {
        if pid == root_pid {
            frontier.push(pid);
            verified_current_pids.insert(pid);
            continue;
        }
        match processes_by_pid.get(&pid) {
            Some(current) if current.image_name != known_identity.image_name => {
                ancestry.retired_pids.insert(pid);
            }
            Some(current) => {
                let Some(current_creation_time) = current_windows_process_creation_time(
                    current,
                    &mut query_creation_time,
                    &mut current_identities,
                )?
                else {
                    return Ok(WindowsProcessScan::MissingProcessIdentity { pid });
                };
                if current.identity(current_creation_time) == *known_identity {
                    ancestry.retired_pids.remove(&pid);
                    frontier.push(pid);
                    verified_current_pids.insert(pid);
                } else {
                    ancestry.retired_pids.insert(pid);
                }
            }
            None if !ancestry.retired_pids.contains(&pid) => frontier.push(pid),
            None => {}
        }
    }

    let mut seen = std::collections::HashSet::new();
    while let Some(parent_pid) = frontier.pop() {
        if !seen.insert(parent_pid) {
            continue;
        }
        for process in children_by_parent.get(&parent_pid).into_iter().flatten() {
            if process.pid != root_pid {
                let Some(creation_time) = current_windows_process_creation_time(
                    process,
                    &mut query_creation_time,
                    &mut current_identities,
                )?
                else {
                    return Ok(WindowsProcessScan::MissingProcessIdentity { pid: process.pid });
                };
                ancestry
                    .known_descendant_instances
                    .insert(process.pid, process.identity(creation_time));
                ancestry.retired_pids.remove(&process.pid);
                verified_current_pids.insert(process.pid);
                frontier.push(process.pid);
            }
        }
    }

    // A child under a PID known to belong to a different instance has ambiguous ancestry; fail closed.
    // Con dưới PID đã xác định bị thay instance có nguồn gốc mơ hồ; từ chối an toàn.
    for &parent_pid in &ancestry.retired_pids {
        if let Some(child) = children_by_parent.get(&parent_pid).and_then(|children| {
            children
                .iter()
                .find(|child| !verified_current_pids.contains(&child.pid))
        }) {
            return Ok(WindowsProcessScan::UnverifiableAncestry {
                parent_pid,
                child_pid: child.pid,
            });
        }
    }

    let mut missing_command_line = None;
    for process in processes
        .iter()
        .filter(|process| process.pid != root_pid && verified_current_pids.contains(&process.pid))
    {
        let command_line = process
            .command
            .iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        if process.command.is_empty() && missing_command_line.is_none() {
            missing_command_line = Some((process.pid, process.image_name.clone()));
        }
        if let Some(name) = forbidden_process_name(&process.image_name, &command_line, exempt) {
            return Ok(WindowsProcessScan::Forbidden(ForbiddenProcess {
                pid: process.pid,
                name,
            }));
        }
    }

    Ok(match missing_command_line {
        Some((pid, image_name)) => WindowsProcessScan::MissingCommandLine { pid, image_name },
        None => WindowsProcessScan::Clean,
    })
}

#[cfg(test)]
fn current_windows_process_creation_time(
    process: &WindowsProcessSnapshot,
    query_creation_time: &mut impl FnMut(u32) -> Result<Option<u64>>,
    cached_identities: &mut std::collections::HashMap<u32, Option<u64>>,
) -> Result<Option<u64>> {
    if let Some(creation_time) = process.creation_time {
        return Ok(Some(creation_time));
    }
    if let Some(creation_time) = cached_identities.get(&process.pid) {
        return Ok(*creation_time);
    }
    let creation_time = query_creation_time(process.pid)?;
    cached_identities.insert(process.pid, creation_time);
    Ok(creation_time)
}

#[cfg(any(windows, test))]
fn retry_windows_process_scan(
    started: Instant,
    timeout: Option<Duration>,
    mut child_has_exited: impl FnMut() -> Result<bool>,
    mut inspect_snapshot: impl FnMut() -> Result<WindowsProcessScan>,
) -> Result<WindowsProcessScan> {
    for attempt in 0..=WINDOWS_COMMAND_LINE_RETRIES {
        if child_has_exited()? {
            return Ok(WindowsProcessScan::ChildExited);
        }
        if timeout.is_some_and(|limit| started.elapsed() >= limit) {
            return Ok(WindowsProcessScan::DeadlineReached);
        }
        let snapshot_scan = match inspect_snapshot() {
            Ok(scan) => scan,
            Err(error) => {
                if child_has_exited()? {
                    return Ok(WindowsProcessScan::ChildExited);
                }
                if timeout.is_some_and(|limit| started.elapsed() >= limit) {
                    return Ok(WindowsProcessScan::DeadlineReached);
                }
                return Err(error);
            }
        };
        if !matches!(&snapshot_scan, WindowsProcessScan::Forbidden(_)) {
            if child_has_exited()? {
                return Ok(WindowsProcessScan::ChildExited);
            }
            if timeout.is_some_and(|limit| started.elapsed() >= limit) {
                return Ok(WindowsProcessScan::DeadlineReached);
            }
        }
        match snapshot_scan {
            WindowsProcessScan::Clean => return Ok(WindowsProcessScan::Clean),
            WindowsProcessScan::Forbidden(process) => {
                return Ok(WindowsProcessScan::Forbidden(process));
            }
            WindowsProcessScan::MissingCommandLine { pid, image_name }
                if attempt == WINDOWS_COMMAND_LINE_RETRIES =>
            {
                if child_has_exited()? {
                    return Ok(WindowsProcessScan::ChildExited);
                }
                if timeout.is_some_and(|limit| started.elapsed() >= limit) {
                    return Ok(WindowsProcessScan::DeadlineReached);
                }
                validate_monitored_child_command_line(&image_name, &[], pid)?;
                bail!("command line unexpectedly passed validation for monitored child PID {pid}");
            }
            #[cfg(test)]
            WindowsProcessScan::MissingProcessIdentity { pid }
                if attempt == WINDOWS_COMMAND_LINE_RETRIES =>
            {
                bail!("cannot read process creation identity for monitored PID {pid}");
            }
            WindowsProcessScan::MissingCommandLine { .. } => {
                // Refresh and rewalk the entire tree on the next inspection, not just this PID.
                // Lần kiểm tra sau làm mới và duyệt lại toàn cây, không chỉ hỏi lại PID này.
                let retry_delay = Duration::from_millis(WINDOWS_COMMAND_LINE_RETRY_DELAY_MS);
                if let Some(stop_reason) = wait_for_windows_process_retry(
                    retry_delay,
                    started,
                    timeout,
                    &mut child_has_exited,
                )? {
                    return Ok(stop_reason);
                }
            }
            #[cfg(test)]
            WindowsProcessScan::MissingProcessIdentity { .. } => {
                let retry_delay = Duration::from_millis(WINDOWS_COMMAND_LINE_RETRY_DELAY_MS);
                if let Some(stop_reason) = wait_for_windows_process_retry(
                    retry_delay,
                    started,
                    timeout,
                    &mut child_has_exited,
                )? {
                    return Ok(stop_reason);
                }
            }
            #[cfg(test)]
            WindowsProcessScan::UnverifiableAncestry {
                parent_pid,
                child_pid,
            } => bail!(
                "process ancestry is ambiguous after PID reuse (parent PID {parent_pid}, child PID {child_pid})"
            ),
            WindowsProcessScan::ChildExited | WindowsProcessScan::DeadlineReached => {
                unreachable!("snapshot inspection cannot produce a retry stop reason")
            }
        }
    }
    unreachable!("bounded process metadata retry loop must return")
}

#[cfg(any(windows, test))]
fn wait_for_windows_process_retry(
    retry_delay: Duration,
    started: Instant,
    timeout: Option<Duration>,
    child_has_exited: &mut impl FnMut() -> Result<bool>,
) -> Result<Option<WindowsProcessScan>> {
    let wait_started = Instant::now();
    loop {
        if child_has_exited()? {
            return Ok(Some(WindowsProcessScan::ChildExited));
        }
        if timeout.is_some_and(|limit| started.elapsed() >= limit) {
            return Ok(Some(WindowsProcessScan::DeadlineReached));
        }

        let wait_elapsed = wait_started.elapsed();
        if wait_elapsed >= retry_delay {
            return Ok(None);
        }
        let poll_interval = Duration::from_millis(WINDOWS_COMMAND_LINE_RETRY_POLL_INTERVAL_MS);
        let remaining_wait = retry_delay.saturating_sub(wait_elapsed);
        let remaining_timeout = timeout
            .map(|limit| limit.saturating_sub(started.elapsed()))
            .unwrap_or(remaining_wait);
        let sleep_for = remaining_wait.min(poll_interval).min(remaining_timeout);
        if sleep_for.is_zero() {
            return Ok(Some(WindowsProcessScan::DeadlineReached));
        }
        std::thread::sleep(sleep_for);
    }
}

#[cfg(windows)]
fn find_forbidden_job_descendant_with_timeout(
    guard: &ProcessTreeGuard,
    root_pid: u32,
    exempt: &[&str],
    started: Instant,
    timeout: Option<Duration>,
    child_has_exited: impl FnMut() -> Result<bool>,
) -> Result<WindowsProcessScan> {
    retry_windows_process_scan(started, timeout, child_has_exited, || {
        Ok(windows_job_scan_from_inspection(inspect_windows_job(
            guard, root_pid, exempt,
        )?))
    })
}

#[cfg(all(windows, test))]
fn find_forbidden_descendant_with_timeout(
    root_pid: u32,
    exempt: &[&str],
    _shadow_dir: &Path,
    started: Instant,
    timeout: Option<Duration>,
    ancestry: &mut WindowsProcessAncestry,
    child_has_exited: impl FnMut() -> Result<bool>,
) -> Result<WindowsProcessScan> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut system = System::new();
    let root = Pid::from_u32(root_pid);
    retry_windows_process_scan(started, timeout, child_has_exited, || {
        // A full refresh discovers descendants created while an earlier command line was blank.
        // Làm mới toàn bộ giúp phát hiện con cháu được tạo khi command line lượt trước còn trống.
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
        );
        if system.process(root).is_none() {
            bail!("process table did not contain monitored root PID {root_pid}");
        }
        let snapshot = system
            .processes()
            .iter()
            .map(|(pid, process)| WindowsProcessSnapshot {
                pid: pid.as_u32(),
                parent_pid: process.parent().map(|parent| parent.as_u32()),
                creation_time: None,
                image_name: process.name().to_string_lossy().into_owned(),
                command: process.cmd().to_vec(),
            })
            .collect::<Vec<_>>();
        let observed_start_times = system
            .processes()
            .iter()
            .map(|(pid, process)| (pid.as_u32(), process.start_time()))
            .collect::<std::collections::HashMap<_, _>>();
        scan_windows_process_snapshot(root_pid, ancestry, &snapshot, exempt, |pid| {
            let Some(observed_start_time) = observed_start_times.get(&pid) else {
                return Ok(None);
            };
            let creation_time = windows_process_creation_time(pid);
            // Match the live handle to the refreshed row before accepting its exact creation time.
            // Đối chiếu handle đang sống với dòng vừa refresh trước khi nhận thời điểm tạo chính xác.
            Ok(creation_time.filter(|creation_time| {
                (*creation_time / WINDOWS_FILETIME_TICKS_PER_SECOND)
                    .checked_sub(WINDOWS_FILETIME_UNIX_EPOCH_OFFSET_SECONDS)
                    == Some(*observed_start_time)
            }))
        })
    })
}

#[cfg(all(windows, test))]
#[allow(unsafe_code)]
fn windows_process_creation_time(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: this PID comes from the refreshed process table; the requested access is query-only.
    // (An toàn: PID lấy từ bảng process vừa refresh; quyền mở chỉ dùng để truy vấn thông tin.)
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }

    let mut creation_time = FILETIME::default();
    let mut exit_time = FILETIME::default();
    let mut kernel_time = FILETIME::default();
    let mut user_time = FILETIME::default();
    // SAFETY: handle is valid from OpenProcess and all output pointers reference live FILETIME values.
    // (An toàn: handle còn hiệu lực từ OpenProcess; các con trỏ output trỏ tới FILETIME còn sống.)
    let query_succeeded = unsafe {
        GetProcessTimes(
            handle,
            &mut creation_time,
            &mut exit_time,
            &mut kernel_time,
            &mut user_time,
        ) != 0
    };
    // SAFETY: handle is the non-null handle returned by OpenProcess above and is closed exactly once.
    // (An toàn: đóng đúng một lần handle khác null do OpenProcess trả về ở trên.)
    let close_succeeded = unsafe { CloseHandle(handle) != 0 };
    if !query_succeeded || !close_succeeded {
        return None;
    }
    Some(((creation_time.dwHighDateTime as u64) << 32) | creation_time.dwLowDateTime as u64)
}

#[cfg(not(any(unix, windows)))]
fn find_forbidden_descendant(
    _root_pid: u32,
    _exempt: &[&str],
    _shadow_dir: &Path,
) -> Result<Option<ForbiddenProcess>> {
    ensure_process_inspection_available(true, false)?;
    Ok(None)
}

#[cfg(unix)]
fn is_shadow_npm_version_probe(command: &[u8], command_line: &[u8], shadow_dir: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;

    let Ok(directory_metadata) = std::fs::metadata(shadow_dir) else {
        return false;
    };
    let shim_path = shadow_dir.join("npm");
    let Ok(shim_metadata) = std::fs::metadata(&shim_path) else {
        return false;
    };
    let Ok(shim_content) = std::fs::read(&shim_path) else {
        return false;
    };
    if directory_metadata.permissions().mode() & 0o222 != 0
        || shim_metadata.permissions().mode() & 0o222 != 0
        || shim_content != NPM_VERSION_PROBE_SHIM_CONTENT.as_bytes()
    {
        return false;
    }

    let process = process_basename(&String::from_utf8_lossy(command));
    npm_probe_command_line_matches_shim_path(
        &process,
        command_line,
        shim_path.as_os_str().as_bytes(),
    )
}

#[cfg(unix)]
fn npm_probe_command_line_matches_shim_path(
    process: &str,
    command_line: &[u8],
    shim: &[u8],
) -> bool {
    let prefixes: &[&[u8]] = match process {
        "sh" => &[b"sh", b"/bin/sh"],
        "dash" => &[b"dash", b"/bin/dash"],
        "bash" => &[b"bash", b"/bin/bash"],
        "busybox" => &[b"busybox sh", b"/bin/busybox sh"],
        "npm" => &[
            b"",
            b"sh",
            b"/bin/sh",
            b"dash",
            b"/bin/dash",
            b"bash",
            b"/bin/bash",
            b"busybox sh",
            b"/bin/busybox sh",
        ],
        _ => return false,
    };

    prefixes.iter().any(|prefix| {
        let mut direct = prefix.to_vec();
        if !prefix.is_empty() {
            direct.push(b' ');
        }
        direct.extend_from_slice(shim);
        direct.extend_from_slice(b" --version");
        if command_line == direct {
            return true;
        }

        let mut quoted = prefix.to_vec();
        if !prefix.is_empty() {
            quoted.push(b' ');
        }
        quoted.push(b'\"');
        quoted.extend_from_slice(shim);
        quoted.extend_from_slice(b"\" --version");
        command_line == quoted
    })
}

fn ensure_process_inspection_available(
    monitor_enabled: bool,
    inspector_available: bool,
) -> Result<()> {
    if monitor_enabled && !inspector_available {
        return Err(process_inspection_unavailable_error());
    }
    Ok(())
}

fn process_inspection_unavailable_error() -> anyhow::Error {
    anyhow::anyhow!(
        "native process-tree inspection is unavailable on this platform; refusing to treat the process tree as clean"
    )
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
        .or_else(|| base.strip_suffix(".bat"))
        .or_else(|| base.strip_suffix(".ps1"))
        .unwrap_or(&base)
        .to_string()
}

#[cfg(any(windows, test))]
fn validate_monitored_child_command_line(
    image_name: &str,
    command: &[OsString],
    pid: u32,
) -> Result<()> {
    if command.is_empty() {
        bail!(
            "cannot read command line for monitored child '{}' PID {pid}",
            image_name
        );
    }
    Ok(())
}

fn forbidden_process_name(command: &str, command_line: &str, exempt: &[&str]) -> Option<String> {
    let exempt = |name: &str| exempt.contains(&name);
    let process_name = process_basename(command);
    if FORBIDDEN_TOOLS.contains(&process_name.as_str()) && !exempt(&process_name) {
        return Some(process_name);
    }

    command_line
        .split_whitespace()
        .filter_map(crate::allowlist::normalize_script_token)
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
