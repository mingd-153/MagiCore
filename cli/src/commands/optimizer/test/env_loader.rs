// Tests for env_loader — extracted from inline tests per RULE §5
// Tests cho env_loader — tách từ inline tests theo RULE §5

use super::super::env_loader::*;
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
    let vars = load_optimizer_env(temp.path()).unwrap();
    assert!(vars.is_empty());
}

#[test]
fn test_load_optimizer_env_with_bun() {
    let temp = TempDir::new().unwrap();
    let optimizer_dir = temp.path().join(".mgc-optimizer");
    fs::create_dir(&optimizer_dir).unwrap();

    fs::write(
        optimizer_dir.join("bun_env.env"),
        "BUN_TRANSPILER_CACHE_PATH=/tmp/bun-cache\n",
    )
    .unwrap();

    let vars = load_optimizer_env(temp.path()).unwrap();
    assert_eq!(
        vars.get("BUN_TRANSPILER_CACHE_PATH"),
        Some(&"/tmp/bun-cache".to_string())
    );
}
