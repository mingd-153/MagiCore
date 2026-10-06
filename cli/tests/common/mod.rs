#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::Instant;

const MANIFEST: &str = env!("CARGO_MANIFEST_DIR");
const COMMAND_TIMEOUT_SECS: u64 = 300;

thread_local! {
    /// Each integration test gets isolated caches shared by its CLI child processes.
    /// Mỗi integration test có cache riêng, dùng chung cho các tiến trình CLI con.
    static ISOLATED_TEST_CACHE: std::cell::OnceCell<tempfile::TempDir> = const {
        std::cell::OnceCell::new()
    };
}

fn isolated_test_cache_root() -> PathBuf {
    ISOLATED_TEST_CACHE.with(|cache| {
        cache
            .get_or_init(|| tempfile::tempdir().expect("create isolated MGC test cache"))
            .path()
            .to_path_buf()
    })
}

/// Run mgc command from workspace root.
pub fn mgc(args: &[&str]) -> (bool, String) {
    run_mg(args, Path::new(MANIFEST))
}

/// Run mgc command in a specific target directory.
pub fn mgc_in(dir: &Path, args: &[&str]) -> (bool, String) {
    run_mg(args, dir)
}

fn run_mg(args: &[&str], cwd: &Path) -> (bool, String) {
    let runtime_bin = std::env::var("CARGO_BIN_EXE_mgc")
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.exists());
    let compile_bin = option_env!("CARGO_BIN_EXE_mgc")
        .map(PathBuf::from)
        .filter(|path| path.exists());
    let bin = runtime_bin
        .or(compile_bin)
        .expect("Cargo must provide CARGO_BIN_EXE_mgc for CLI integration tests");
    let mut options = mgc_exec::run::ExecOptions {
        cwd: Some(cwd.to_path_buf()),
        timeout: Some(std::time::Duration::from_secs(COMMAND_TIMEOUT_SECS)),
        capture_full_stdout: true,
        // A CLI error is evidence returned to the test, never a successful command.
        // Trả mã lỗi cho assertion của test, không coi CLI lỗi là thành công.
        allowed_exit_codes: (1..=255).collect(),
        ..Default::default()
    };
    let cache_root = isolated_test_cache_root();
    options.env.push((
        "MGC_TEMPLATES_DIR".into(),
        cache_root.join("templates").to_string_lossy().into_owned(),
    ));
    options.env.push((
        "MGC_SCAFFOLDS_DIR".into(),
        cache_root.join("scaffolds").to_string_lossy().into_owned(),
    ));
    options.env.push((
        "XDG_CACHE_HOME".into(),
        cache_root.join("xdg-cache").to_string_lossy().into_owned(),
    ));

    let workspace_root = Path::new(MANIFEST)
        .parent()
        .expect("CLI manifest should have a parent");

    // Only pin workspace templates when the tree actually holds template
    // content — the repo may keep just placeholder READMEs and rely on the
    // registry cache (~/.mgc/templates).
    let template_disk = workspace_root.join("templates");
    let template_contract = template_disk
        .join("web")
        .join("frontend")
        .join("react-vite")
        .join("template.toml")
        .is_file();

    if template_contract {
        options.env.push((
            "MAGICORE_TEMPLATE_DIR".into(),
            template_disk.to_string_lossy().into_owned(),
        ));
    }

    run_binary_with_options(&bin, args, &options)
}

pub fn run_binary_with_options(
    bin: &Path,
    args: &[&str],
    options: &mgc_exec::run::ExecOptions,
) -> (bool, String) {
    // Reuse the platform's bounded process-tree executor instead of unbounded output().
    // Dùng executor có timeout và kill cây tiến trình thay output() không giới hạn.
    let arguments = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let output = match mgc_exec::run::run_project_binary(bin, &arguments, options) {
        Ok(report) => report,
        Err(error) => return (false, format!("MGC test command failed: {error:#}")),
    };
    let stdout = output.stdout_full;
    let stderr = output.stderr_tail;
    let combined = if stderr.is_empty() {
        stdout
    } else {
        format!("{stdout}\n{stderr}")
    };
    (output.exit_code == 0, combined)
}

/// Create a temp directory for scaffold testing.
pub fn work_dir() -> PathBuf {
    let unique = format!(
        "mgc-test-{}-{:?}-{}",
        std::process::id(),
        std::thread::current().id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let base = std::env::temp_dir().join(unique);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create work dir");
    base
}

/// Scaffold a project in a temp directory and return its path.
/// Note: --dir flag is not supported; project is always created in CWD.
pub fn scaffold(framework: &str, project: &str) -> PathBuf {
    let base = work_dir();
    let result = mgc_in(&base, &["create-web", framework, project, "--ts"]);
    assert!(result.0, "scaffold {framework} failed:\n{}", result.1);
    base.join(project)
}

/// Assert file exists in project directory.
pub fn assert_file_exists(project: &Path, rel_path: &str) {
    let full = project.join(rel_path);
    assert!(
        full.exists(),
        "expected file '{}' not found in '{}'",
        rel_path,
        project.display()
    );
}

/// Assert file contains expected content.
pub fn assert_file_contains(project: &Path, rel_path: &str, expected: &str) {
    let full = project.join(rel_path);
    assert!(full.exists(), "file '{rel_path}' does not exist");
    let content = std::fs::read_to_string(&full).unwrap_or_default();
    assert!(
        content.contains(expected),
        "file '{rel_path}' does not contain:\n  expected: {expected}\n  actual:\n{content}"
    );
}

/// Time scaffold execution, return duration in ms.
pub fn bench_scaffold(framework: &str, project: &str) -> u128 {
    let base = work_dir();
    let start = Instant::now();
    let result = mgc_in(&base, &["create-web", framework, project, "--ts"]);
    let elapsed = start.elapsed().as_millis();
    assert!(result.0, "bench scaffold {framework} failed: {}", result.1);
    elapsed
}

pub fn assert_help_contains(expected: &str) {
    let (ok, out) = mgc(&["--help"]);
    assert!(ok, "mgc --help failed");
    assert!(
        out.contains(expected),
        "mgc --help should contain '{expected}'\n---\n{out}"
    );
}

pub fn assert_help_excludes(unexpected: &str) {
    let (ok, out) = mgc(&["--help"]);
    assert!(ok, "mgc --help failed");
    assert!(
        !out.contains(unexpected),
        "mgc --help should NOT contain '{unexpected}'\n---\n{out}"
    );
}

/// Default dev port
pub const DEV_PORT: &str = "4315";
