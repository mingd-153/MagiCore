//! Unit tests for runtime detection layer

use crate::commands::optimizer::runtime_detect::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_detect_web_deno() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("deno.json"), "{}").unwrap();

    let runtimes = detect_runtimes(dir.path(), "web");
    assert_eq!(runtimes, vec![DetectedRuntime::Deno]);
}

#[test]
fn test_detect_web_bun() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("bun.lockb"), "").unwrap();

    let runtimes = detect_runtimes(dir.path(), "web");
    assert_eq!(runtimes, vec![DetectedRuntime::Bun]);
}

#[test]
fn test_detect_web_nodejs_pnpm() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("package.json"), "{}").unwrap();
    fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();

    let runtimes = detect_runtimes(dir.path(), "web");
    assert_eq!(
        runtimes,
        vec![DetectedRuntime::NodeJs {
            package_manager: PackageManager::Pnpm
        }]
    );
}

#[test]
fn test_detect_ai_python_pytorch() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\ndependencies = [\"torch>=2.8\"]\n",
    )
    .unwrap();

    let runtimes = detect_runtimes(dir.path(), "ai");
    assert_eq!(runtimes, vec![DetectedRuntime::PythonPyTorch]);
}

#[test]
fn test_generic_python_ai_is_not_misclassified_as_pytorch() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\ndependencies = [\"tensorflow>=2\"]\n",
    )
    .unwrap();

    let runtimes = detect_runtimes(dir.path(), "ai");
    assert_eq!(runtimes, vec![DetectedRuntime::Unknown]);
}

#[test]
fn test_detect_ai_python_pytorch_from_requirements() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"demo\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("requirements.txt"),
        "# runtime\ntorch[distributed]==2.8.0 ; python_version >= '3.11'\n",
    )
    .unwrap();

    let runtimes = detect_runtimes(dir.path(), "ai");
    assert_eq!(runtimes, vec![DetectedRuntime::PythonPyTorch]);
}

#[test]
fn test_detect_ai_python_pytorch_from_poetry_dependencies() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("pyproject.toml"),
        "[tool.poetry.dependencies]\npython = \"^3.12\"\ntorch = \"^2.8\"\n",
    )
    .unwrap();

    let runtimes = detect_runtimes(dir.path(), "ai");
    assert_eq!(runtimes, vec![DetectedRuntime::PythonPyTorch]);
}

#[test]
fn test_unrelated_torch_metadata_does_not_enable_pytorch_optimizer() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"torch-documentation-example\"\ndependencies = [\"numpy\"]\n",
    )
    .unwrap();

    let runtimes = detect_runtimes(dir.path(), "ai");
    assert_eq!(runtimes, vec![DetectedRuntime::Unknown]);
}

#[test]
fn test_detect_ai_rust_candle() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[dependencies]\ncandle-core = \"0.1\"",
    )
    .unwrap();

    let runtimes = detect_runtimes(dir.path(), "ai");
    assert_eq!(runtimes, vec![DetectedRuntime::RustCandle]);
}

#[test]
fn test_detect_lib_go() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("go.mod"), "module example.com/lib").unwrap();

    let runtimes = detect_runtimes(dir.path(), "lib");
    assert_eq!(runtimes, vec![DetectedRuntime::GoLib]);
}

#[test]
fn test_detect_implicit_rust_lib_target() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(dir.path().join("src/lib.rs"), "pub fn demo() {}\n").unwrap();

    let runtimes = detect_runtimes(dir.path(), "lib");
    assert_eq!(runtimes, vec![DetectedRuntime::RustLib]);
}

#[test]
fn test_binary_only_rust_project_is_not_detected_as_lib() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();

    let runtimes = detect_runtimes(dir.path(), "lib");
    assert_eq!(runtimes, vec![DetectedRuntime::Unknown]);
}

#[test]
fn test_detect_app_flutter() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("pubspec.yaml"), "name: myapp").unwrap();

    let runtimes = detect_runtimes(dir.path(), "app");
    assert_eq!(runtimes, vec![DetectedRuntime::Flutter]);
}

#[test]
fn test_detect_app_react_native() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"dependencies": {"react-native": "0.70"}}"#,
    )
    .unwrap();

    let runtimes = detect_runtimes(dir.path(), "app");
    assert_eq!(runtimes, vec![DetectedRuntime::ReactNative]);
}

#[test]
fn test_unknown_fallback() {
    let dir = TempDir::new().unwrap();
    let runtimes = detect_runtimes(dir.path(), "web");
    assert_eq!(runtimes, vec![DetectedRuntime::Unknown]);
}
