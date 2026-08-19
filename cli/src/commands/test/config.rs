//! Tests cho `mg config` — get/set/delete/unset/list (npmrc + mg.toml layers)

use super::*;
use std::fs;

/// Helper: tạo temp dir có .npmrc
fn temp_dir_with_npmrc(content: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".npmrc"), content).unwrap();
    dir
}

/// Helper: tạo temp dir có mg.toml
fn temp_dir_with_mg_toml(content: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("mg.toml"), content).unwrap();
    dir
}

#[test]
fn file_value_reads_npmrc_key() {
    let dir = temp_dir_with_npmrc("registry=https://example.com\nfoo=bar\n");
    let val = file_value(&dir.path().join(".npmrc"), "registry");
    assert_eq!(val, Some("https://example.com".to_string()));
    let missing = file_value(&dir.path().join(".npmrc"), "missing_key");
    assert!(missing.is_none());
}

#[test]
fn file_value_ignores_comments() {
    let dir = temp_dir_with_npmrc("# this is a comment\n;also ignored\nkey=value\n");
    let val = file_value(&dir.path().join(".npmrc"), "key");
    assert_eq!(val, Some("value".to_string()));
    // comment line must not be a key
    let comment_val = file_value(&dir.path().join(".npmrc"), "# this is a comment");
    assert!(comment_val.is_none());
}

#[test]
fn set_npmrc_creates_and_updates_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".npmrc");
    // Lần đầu: tạo mới
    set_npmrc(&path, "registry", "https://example.com").unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("registry=https://example.com"));
    // Lần hai: ghi đè
    set_npmrc(&path, "registry", "https://other.com").unwrap();
    let content2 = fs::read_to_string(&path).unwrap();
    assert!(content2.contains("registry=https://other.com"));
    assert!(!content2.contains("example.com"));
}

#[test]
fn delete_npmrc_removes_key() {
    let dir = temp_dir_with_npmrc("registry=https://example.com\nfoo=bar\n");
    let path = dir.path().join(".npmrc");
    delete_npmrc(&path, "registry").unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(!content.contains("registry="));
    assert!(content.contains("foo=bar"));
}

#[test]
fn toml_value_reads_top_level_key() {
    let dir = temp_dir_with_mg_toml("name = \"myapp\"\necosystem = \"web\"\nversion = \"0.1.0\"\n");
    let val = toml_value(&dir.path().join("mg.toml"), "ecosystem");
    assert_eq!(val.as_deref(), Some("web"));
    let ver = toml_value(&dir.path().join("mg.toml"), "version");
    assert_eq!(ver.as_deref(), Some("0.1.0"));
}

#[test]
fn toml_value_reads_dot_notation() {
    let dir = temp_dir_with_mg_toml("[game]\nengine = \"bevy\"\n\n[iot]\nframework = \"esp-idf\"\n");
    let val = toml_value(&dir.path().join("mg.toml"), "game.engine");
    assert_eq!(val.as_deref(), Some("bevy"));
    let iot_val = toml_value(&dir.path().join("mg.toml"), "iot.framework");
    assert_eq!(iot_val.as_deref(), Some("esp-idf"));
}

#[test]
fn is_sensitive_catches_token_keys() {
    assert!(is_sensitive("_authToken"));
    assert!(is_sensitive("npm_token"));
    assert!(is_sensitive("my_password"));
    assert!(!is_sensitive("registry"));
    assert!(!is_sensitive("ecosystem"));
}

#[test]
fn is_sensitive_not_false_positive_on_normal_keys() {
    assert!(!is_sensitive("version"));
    assert!(!is_sensitive("name"));
    assert!(!is_sensitive("mode"));
}

#[test]
fn merge_file_combines_keys() {
    let dir = temp_dir_with_npmrc("key1=val1\nkey2=val2\n");
    let mut map = BTreeMap::new();
    merge_file(&mut map, &dir.path().join(".npmrc"));
    assert_eq!(map.get("key1").map(|s| s.as_str()), Some("val1"));
    assert_eq!(map.get("key2").map(|s| s.as_str()), Some("val2"));
}

#[test]
fn merge_file_no_panic_on_missing_file() {
    let mut map = BTreeMap::new();
    // Should not panic on missing file
    merge_file(&mut map, std::path::Path::new("/nonexistent/path/.npmrc"));
    assert!(map.is_empty());
}
