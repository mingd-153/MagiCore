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
#[cfg(unix)]
const PROCESS_TABLE_INSPECTOR: &str = "/bin/ps";

static SHADOW_PATH_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
#[cfg(not(unix))]
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
    #[cfg(not(unix))]
    let path_var: Option<std::ffi::OsString> = opts
        .env
        .iter()
        .rev()
        .find(|(key, _)| is_path_env_key(key))
        .map(|(_, value)| std::ffi::OsString::from(value));
    #[cfg(not(unix))]
    let resolved_cmd = resolve_windows_shim(cmd, path_var.as_deref());
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

    ensure_process_inspection_available(opts.clean_env, cfg!(any(unix, windows)))?;
    let child = command
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn '{cmd}': {e}"))?;

    let timeout = if opts.disable_timeout {
        None
    } else {
        Some(opts.timeout.unwrap_or_else(default_timeout))
    };
    let outcome = wait_with_timeout(
        child,
        timeout,
        opts.clean_env,
        scoped_exempt,
        shadow_path.path(),
        mode,
    )?;
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

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Option<Duration>,
    monitor_forbidden_children: bool,
    exempt: &[&str],
    shadow_dir: &Path,
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
        if monitor_forbidden_children {
            match find_forbidden_descendant(child.id(), exempt, shadow_dir) {
                Ok(Some(found)) => {
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
                Ok(None) => {}
                Err(error) => {
                    terminate_process_tree(child.id());
                    let _ = child.kill();
                    let out = drain(&mut child, Some(Duration::from_millis(POST_KILL_DRAIN_MS)));
                    return Err(anyhow::anyhow!(
                        "cannot verify child process tree; command was terminated fail-closed (status after kill: {}): {error}",
                        out.status
                    ));
                }
            }
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
fn find_forbidden_descendant(
    root_pid: u32,
    exempt: &[&str],
    shadow_dir: &Path,
) -> Result<Option<ForbiddenProcess>> {
    find_forbidden_descendant_with_program(
        root_pid,
        exempt,
        shadow_dir,
        Path::new(PROCESS_TABLE_INSPECTOR),
    )
}

#[cfg(unix)]
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
#[cfg(windows)]
fn find_forbidden_descendant(
    root_pid: u32,
    exempt: &[&str],
    _shadow_dir: &Path,
) -> Result<Option<ForbiddenProcess>> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    let root = Pid::from_u32(root_pid);
    if system.process(root).is_none() {
        bail!("process table did not contain monitored root PID {root_pid}");
    }
    let mut frontier = vec![root];
    let mut seen = std::collections::HashSet::new();
    while let Some(parent) = frontier.pop() {
        if !seen.insert(parent) {
            continue;
        }
        for (pid, process) in system
            .processes()
            .iter()
            .filter(|(_, p)| p.parent() == Some(parent))
        {
            // Missing command metadata cannot prove that a script host is clean.
            // Thiếu metadata command không chứng minh được script host an toàn.
            if process.cmd().is_empty() {
                bail!(
                    "cannot read command line for monitored child PID {}",
                    pid.as_u32()
                );
            }
            let command_line = process
                .cmd()
                .iter()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(name) =
                forbidden_process_name(&process.name().to_string_lossy(), &command_line, exempt)
            {
                return Ok(Some(ForbiddenProcess {
                    pid: pid.as_u32(),
                    name,
                }));
            }
            frontier.push(*pid);
        }
    }
    Ok(None)
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
