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
        "[dependencies]\ntorch = \"2.0\"",
    )
    .unwrap();

    let runtimes = detect_runtimes(dir.path(), "ai");
    assert_eq!(runtimes, vec![DetectedRuntime::PythonPyTorch]);
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
