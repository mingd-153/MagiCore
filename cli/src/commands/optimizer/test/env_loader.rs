// Tests for env_loader — extracted from inline tests per RULE §5
// Tests cho env_loader — tách từ inline tests theo RULE §5

use super::super::env_loader::*;
use crate::commands::optimizer::runtime_detect::DetectedRuntime;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_parse_env_file() {
    let temp = TempDir::new().unwrap();
    let env_file = temp.path().join("test.env");

    fs::write(
        &env_file,
        r#"
# Comment line
KEY1=value1
KEY2="quoted value"
KEY3='single quoted'
EMPTY_KEY=

# Another comment
KEY4=value with spaces
"#,
    )
    .unwrap();

    let vars = parse_env_file(&env_file).unwrap();

    assert_eq!(vars.get("KEY1"), Some(&"value1".to_string()));
    assert_eq!(vars.get("KEY2"), Some(&"quoted value".to_string()));
    assert_eq!(vars.get("KEY3"), Some(&"single quoted".to_string()));
    assert_eq!(vars.get("EMPTY_KEY"), Some(&"".to_string()));
    assert_eq!(vars.get("KEY4"), Some(&"value with spaces".to_string()));
}

#[test]
fn test_load_optimizer_env_no_dir() {
    let temp = TempDir::new().unwrap();
    let vars = load_optimizer_env(temp.path(), &DetectedRuntime::Bun).unwrap();
    assert!(vars.is_empty());
}

#[test]
fn test_load_optimizer_env_with_bun() {
    let temp = TempDir::new().unwrap();
    let optimizer_dir = temp.path().join(".mgc-optimizer");
    fs::create_dir(&optimizer_dir).unwrap();

    fs::write(
        optimizer_dir.join("bun_env.env"),
        "BUN_RUNTIME_TRANSPILER_CACHE_PATH=/tmp/bun-cache\n",
    )
    .unwrap();

    let vars = load_optimizer_env(temp.path(), &DetectedRuntime::Bun).unwrap();
    assert_eq!(
        vars.get("BUN_RUNTIME_TRANSPILER_CACHE_PATH"),
        Some(&"/tmp/bun-cache".to_string())
    );
}

#[test]
fn test_load_optimizer_env_runtime_mismatch() {
    let temp = TempDir::new().unwrap();
    let optimizer_dir = temp.path().join(".mgc-optimizer");
    fs::create_dir(&optimizer_dir).unwrap();

    // Write Bun config
    fs::write(
        optimizer_dir.join("bun_env.env"),
        "BUN_RUNTIME_TRANSPILER_CACHE_PATH=/tmp/bun-cache\n",
    )
    .unwrap();

    // Write Deno config
    fs::write(
        optimizer_dir.join("deno_env.env"),
        "DENO_V8_FLAGS=--max-old-space-size=4096\n",
    )
    .unwrap();

    // Request Deno runtime - should only load deno_env.env
    let vars = load_optimizer_env(temp.path(), &DetectedRuntime::Deno).unwrap();
    assert!(vars.get("BUN_RUNTIME_TRANSPILER_CACHE_PATH").is_none());
    assert_eq!(
        vars.get("DENO_V8_FLAGS"),
        Some(&"--max-old-space-size=4096".to_string())
    );
}

#[test]
fn test_load_optimizer_env_ai_runtime() {
    let temp = TempDir::new().unwrap();
    let optimizer_dir = temp.path().join(".mgc-optimizer");
    fs::create_dir(&optimizer_dir).unwrap();

    // Write PyTorch config
    fs::write(
        optimizer_dir.join("pytorch_runtime.env"),
        "PYTORCH_CUDA_ALLOC_CONF=max_split_size_mb:512\n",
    )
    .unwrap();

    fs::write(
        optimizer_dir.join("pytorch_docker.env"),
        "PYTORCH_JIT_FALLBACK=1\n",
    )
    .unwrap();

    // Request PythonPyTorch runtime - should load both pytorch files
    let vars = load_optimizer_env(temp.path(), &DetectedRuntime::PythonPyTorch).unwrap();
    assert_eq!(
        vars.get("PYTORCH_CUDA_ALLOC_CONF"),
        Some(&"max_split_size_mb:512".to_string())
    );
    assert_eq!(vars.get("PYTORCH_JIT_FALLBACK"), Some(&"1".to_string()));
}

#[test]
fn test_load_optimizer_env_lib_runtime() {
    let temp = TempDir::new().unwrap();
    let optimizer_dir = temp.path().join(".mgc-optimizer");
    fs::create_dir(&optimizer_dir).unwrap();

    // Write Rust lib config as TOML
    fs::write(
        optimizer_dir.join("rust_cargo_profile.toml"),
        r#"
[build]
rustflags = ["-C", "target-cpu=native", "-C", "opt-level=3"]

[env]
RUST_BACKTRACE = "1"
"#,
    )
    .unwrap();

    // Request RustLib runtime
    let vars = load_optimizer_env(temp.path(), &DetectedRuntime::RustLib).unwrap();

    // Verify RUSTFLAGS extracted from TOML array
    assert_eq!(
        vars.get("RUSTFLAGS"),
        Some(&"-C target-cpu=native -C opt-level=3".to_string())
    );

    // Verify [env] section parsed
    assert_eq!(vars.get("RUST_BACKTRACE"), Some(&"1".to_string()));
}
