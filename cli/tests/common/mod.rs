#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const MANIFEST: &str = env!("CARGO_MANIFEST_DIR");

/// Run mgc command from workspace root.
pub fn mgc(args: &[&str]) -> (bool, String) {
    run_mg(args, Path::new(MANIFEST))
}

/// Run mgc command in a specific target directory.
pub fn mgc_in(dir: &Path, args: &[&str]) -> (bool, String) {
    run_mg(args, dir)
}

fn run_mg(args: &[&str], cwd: &Path) -> (bool, String) {
    let workspace_manifest = Path::new(MANIFEST).join("../Cargo.toml");
    let workspace_root = workspace_manifest
        .parent()
        .expect("workspace manifest should have a parent");
    let debug_bin = workspace_root.join("target").join("debug").join("mgc");

    let runtime_bin = std::env::var("CARGO_BIN_EXE_mg")
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.exists());
    let compile_bin = option_env!("CARGO_BIN_EXE_mg")
        .map(PathBuf::from)
        .filter(|path| path.exists());

    let mut command = if let Some(bin) = runtime_bin.or(compile_bin) {
        Command::new(bin)
    } else if debug_bin.exists() {
        Command::new(debug_bin)
    } else {
        let mut fallback = Command::new("cargo");
        fallback
            .arg("run")
            .arg("--bin")
            .arg("mgc")
            .arg("--manifest-path")
            .arg(&workspace_manifest)
            .arg("--");
        fallback
    };

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
        command.env("MAGICORE_TEMPLATE_DIR", template_disk);
    }

    let output = command
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to run mgc");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = if stderr.is_empty() {
        stdout
    } else {
        format!("{stdout}\n{stderr}")
    };
    (output.status.success(), combined)
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
