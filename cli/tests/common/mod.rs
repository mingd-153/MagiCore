#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const MANIFEST: &str = env!("CARGO_MANIFEST_DIR");

/// Run mg command from workspace root.
pub fn mg(args: &[&str]) -> (bool, String) {
    run_mg(args, Path::new(MANIFEST))
}

/// Run mg command in a specific target directory.
pub fn mg_in(dir: &Path, args: &[&str]) -> (bool, String) {
    run_mg(args, dir)
}

fn run_mg(args: &[&str], cwd: &Path) -> (bool, String) {
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("mg")
        .arg("--manifest-path")
        .arg(format!("{}/../Cargo.toml", MANIFEST))
        .arg("--")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to run mg");
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
    let base = std::env::temp_dir().join(format!("mg-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create work dir");
    base
}

/// Scaffold a project in a temp directory and return its path.
/// Note: --dir flag is not supported; project is always created in CWD.
pub fn scaffold(framework: &str, project: &str) -> PathBuf {
    let base = work_dir();
    let result = mg_in(&base, &["create-web", framework, project, "--ts"]);
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
    let result = mg_in(&base, &["create-web", framework, project, "--ts"]);
    let elapsed = start.elapsed().as_millis();
    assert!(result.0, "bench scaffold {framework} failed: {}", result.1);
    elapsed
}

pub fn assert_help_contains(expected: &str) {
    let (ok, out) = mg(&["--help"]);
    assert!(ok, "mg --help failed");
    assert!(
        out.contains(expected),
        "mg --help should contain '{expected}'\n---\n{out}"
    );
}

pub fn assert_help_excludes(unexpected: &str) {
    let (ok, out) = mg(&["--help"]);
    assert!(ok, "mg --help failed");
    assert!(
        !out.contains(unexpected),
        "mg --help should NOT contain '{unexpected}'\n---\n{out}"
    );
}

/// Default dev port
pub const DEV_PORT: &str = "4315";
