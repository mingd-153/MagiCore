//! Tests cho `mgc config` — get/set/delete/unset/list (npmrc + mgc.toml layers)

use super::*;
use std::fs;

/// Helper: tạo temp dir có .npmrc
fn temp_dir_with_npmrc(content: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".npmrc"), content).unwrap();
    dir
}

/// Helper: tạo temp dir có mgc.toml
fn temp_dir_with_mgc_toml(content: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("mgc.toml"), content).unwrap();
    dir
}

#[test]
fn file_value_reads_npmrc_key() {
    let dir = temp_dir_with_npmrc("registry=https://example.com\nfoo=bar\n");
    let val = file_value(&dir.path().join(".npmrc"), "registry").unwrap();
    assert_eq!(val, Some("https://example.com".to_string()));
    let missing = file_value(&dir.path().join(".npmrc"), "missing_key").unwrap();
    assert!(missing.is_none());
}

#[test]
fn file_value_ignores_comments() {
    let dir = temp_dir_with_npmrc("# this is a comment\n;also ignored\nkey=value\n");
    let val = file_value(&dir.path().join(".npmrc"), "key").unwrap();
    assert_eq!(val, Some("value".to_string()));
    // comment line must not be a key
    let comment_val = file_value(&dir.path().join(".npmrc"), "# this is a comment").unwrap();
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

#[cfg(unix)]
#[test]
fn new_npmrc_is_private_and_existing_mode_is_preserved() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".npmrc");
    set_npmrc(&path, "registry", "https://example.test").unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    set_npmrc(&path, "registry", "https://next.example.test").unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[cfg(unix)]
#[test]
fn writing_npmrc_credentials_tightens_permissive_existing_mode() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".npmrc");
    fs::write(&path, "registry=https://registry.example\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

    set_npmrc(&path, "//registry.example/:_authToken", "sensitive-token").unwrap();

    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600,
        "a config containing credentials must not retain group/world access"
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    set_npmrc(&path, "registry", "https://next.example").unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600,
        "updating a file that already contains credentials must also tighten it"
    );
    assert!(
        fs::read_to_string(path)
            .unwrap()
            .contains("sensitive-token")
    );
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
    let dir =
        temp_dir_with_mgc_toml("name = \"myapp\"\necosystem = \"web\"\nversion = \"0.1.0\"\n");
    let val = toml_value(&dir.path().join("mgc.toml"), "ecosystem");
    assert_eq!(val.as_deref(), Some("web"));
    let ver = toml_value(&dir.path().join("mgc.toml"), "version");
    assert_eq!(ver.as_deref(), Some("0.1.0"));
}

#[test]
fn toml_value_reads_dot_notation() {
    let dir =
        temp_dir_with_mgc_toml("[game]\nengine = \"bevy\"\n\n[iot]\nframework = \"esp-idf\"\n");
    let val = toml_value(&dir.path().join("mgc.toml"), "game.engine");
    assert_eq!(val.as_deref(), Some("bevy"));
    let iot_val = toml_value(&dir.path().join("mgc.toml"), "iot.framework");
    assert_eq!(iot_val.as_deref(), Some("esp-idf"));
}

#[test]
fn project_toml_script_policy_edits_refuse_to_race_an_install() {
    use std::time::Duration;

    let dir = temp_dir_with_mgc_toml(
        "name = \"test-project\"\necosystem = \"web\"\nversion = \"0.1.0\"\n[lock]\nacquire_timeout_ms = 1\n[scripts]\ndeny = [\"sample-package\"]\n",
    );
    let path = dir.path().join("mgc.toml");
    let before = fs::read(&path).expect("original project config must be readable");
    let _held =
        mgc_lockfile::project_lock::ProjectWriteLock::acquire(dir.path(), Duration::from_secs(1))
            .expect("dependency mutation lock must be acquired");

    let set_result = set_toml_at(&path, "scripts.allow", "[\"sample-package\"]");
    let delete_result = delete_toml_at(&path, "scripts.deny");

    assert!(set_result.is_err(), "TOML set must not race install");
    assert!(delete_result.is_err(), "TOML delete must not race install");
    assert_eq!(
        fs::read(path).expect("project config must remain readable"),
        before,
        "a blocked policy edit must leave the project config byte-identical"
    );
}

#[test]
fn config_get_redacts_secret_keys_and_nested_parent_tables() {
    let dir = temp_dir_with_mgc_toml(
        "[registry]\ntoken = \"secret-token\"\npassword = \"secret-password\"\nurl = \"https://registry.example\"\n",
    );
    let path = dir.path().join("mgc.toml");

    let token = toml_value(&path, "registry.token").expect("token key exists");
    let registry = toml_value(&path, "registry").expect("registry table exists");

    assert_eq!(token, "***");
    assert!(registry.contains("token = \"***\""));
    assert!(registry.contains("password = \"***\""));
    assert!(registry.contains("https://registry.example"));
    assert!(!registry.contains("secret-token"));
    assert!(!registry.contains("secret-password"));
}

#[test]
fn config_get_redacts_sensitive_keys_from_all_sources() {
    assert_eq!(
        display_config_value("MGC_REGISTRY_TOKEN", "actual-secret"),
        "***"
    );
    assert_eq!(display_config_value("_authToken", "actual-secret"), "***");
    assert_eq!(
        display_config_value("npm_config__auth", "actual-secret"),
        "***"
    );
    assert_eq!(
        display_config_value("authorization", "actual-secret"),
        "***"
    );
    assert_eq!(display_config_value("private_key", "actual-secret"), "***");
    assert_eq!(
        display_config_value("registry", "https://registry.example"),
        "https://registry.example"
    );
}

#[cfg(unix)]
#[test]
fn npmrc_read_and_write_reject_symlinks_without_touching_target() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("outside.npmrc");
    let path = dir.path().join(".npmrc");
    fs::write(&target, "_authToken=do-not-touch\n").unwrap();
    let before = fs::read(&target).unwrap();
    symlink(&target, &path).unwrap();

    assert!(file_value(&path, "_authToken").is_err());
    assert!(read_lines(&path).is_err());
    assert!(set_npmrc(&path, "registry", "https://example.test").is_err());
    assert_eq!(fs::read(&target).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn npmrc_read_rejects_fifo_without_blocking() {
    use std::os::unix::fs::FileTypeExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".npmrc");
    let status = std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .expect("POSIX test runner must provide mkfifo");
    assert!(status.success());

    let read_path = path.clone();
    let result = std::thread::spawn(move || file_value(&read_path, "registry"))
        .join()
        .unwrap();

    assert!(result.is_err());
    assert!(fs::symlink_metadata(path).unwrap().file_type().is_fifo());
}

#[test]
fn toml_config_list_redacts_sensitive_values() {
    let rendered = redact_toml_config(
        "name = \"sample\"\napi_token = \"secret\"\n[registry]\npassword = \"p4ss\"\nurl = \"https://example.test\"\n[[sources]]\nendpoint = \"https://registry.example\"\naccess_token = \"inline-secret\"\n",
    )
    .unwrap();

    assert!(rendered.contains("name = \"sample\""));
    assert!(rendered.contains("api_token = \"***\""));
    assert!(rendered.contains("password = \"***\""));
    assert!(rendered.contains("url = \"https://example.test\""));
    assert!(rendered.contains("access_token = \"***\""));
    assert!(!rendered.contains("secret"));
    assert!(!rendered.contains("p4ss"));
}

#[cfg(unix)]
#[test]
fn toml_value_refuses_symlinked_config() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("outside.toml");
    let path = dir.path().join("mgc.toml");
    fs::write(&target, "name = \"outside-secret\"\n").unwrap();
    symlink(&target, &path).unwrap();

    assert_eq!(toml_value(&path, "name"), None);
}

#[test]
fn set_toml_cannot_reassign_core_identity_and_preserves_file() {
    let dir = temp_dir_with_mgc_toml("name = \"sample\"\necosystem = \"web\"\n");
    let path = dir.path().join("mgc.toml");
    let before = fs::read(&path).unwrap();

    let err = set_toml_at(&path, "ecosystem", "ai").unwrap_err();

    assert!(err.to_string().contains("mgc init --signature"));
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn delete_toml_cannot_remove_core_identity_and_preserves_file() {
    let dir = temp_dir_with_mgc_toml("name = \"sample\"\necosystem = \"web\"\n");
    let path = dir.path().join("mgc.toml");
    let before = fs::read(&path).unwrap();

    let err = delete_toml_at(&path, "ecosystem").unwrap_err();

    assert!(err.to_string().contains("mgc init --signature"));
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn config_cannot_write_nested_ecosystem_keys() {
    let dir = temp_dir_with_mgc_toml("name = \"sample\"\necosystem = \"web\"\n");
    let path = dir.path().join("mgc.toml");
    let before = fs::read(&path).unwrap();

    let err = set_toml_at(&path, "ecosystem.core", "ai").unwrap_err();

    assert!(err.to_string().contains("mgc init --signature"));
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn set_toml_rejects_symlink_without_modifying_target() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("outside.toml");
    let path = dir.path().join("mgc.toml");
    fs::write(&target, "name = \"outside\"\necosystem = \"web\"\n").unwrap();
    let before = fs::read(&target).unwrap();
    symlink(&target, &path).unwrap();

    let err = set_toml_at(&path, "name", "overwritten").unwrap_err();

    assert!(err.to_string().contains("must not be a symlink"));
    assert_eq!(fs::read(&target).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn delete_toml_rejects_symlink_without_modifying_target() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("outside.toml");
    let path = dir.path().join("mgc.toml");
    fs::write(&target, "name = \"outside\"\necosystem = \"web\"\n").unwrap();
    let before = fs::read(&target).unwrap();
    symlink(&target, &path).unwrap();

    let err = delete_toml_at(&path, "name").unwrap_err();

    assert!(err.to_string().contains("must not be a symlink"));
    assert_eq!(fs::read(&target).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn config_toml_mutations_reject_fifo_without_blocking() {
    use std::os::unix::fs::FileTypeExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mgc.toml");
    let status = std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .expect("POSIX test runner must provide mkfifo");
    assert!(status.success(), "mkfifo exited with {status}");

    let set_result = std::thread::spawn({
        let path = path.clone();
        move || set_toml_at(&path, "name", "blocked")
    })
    .join()
    .unwrap();
    let delete_result = std::thread::spawn({
        let path = path.clone();
        move || delete_toml_at(&path, "name")
    })
    .join()
    .unwrap();

    assert!(set_result.is_err());
    assert!(delete_result.is_err());
    assert!(
        std::fs::symlink_metadata(path)
            .unwrap()
            .file_type()
            .is_fifo()
    );
}

#[test]
fn set_toml_still_allows_non_identity_project_settings() {
    let dir = temp_dir_with_mgc_toml("name = \"sample\"\necosystem = \"web\"\n");
    let path = dir.path().join("mgc.toml");

    set_toml_at(&path, "build.target", "release").unwrap();

    let content = fs::read_to_string(path).unwrap();
    assert!(content.contains("ecosystem = \"web\""));
    assert!(content.contains("target = \"release\""));
}

#[cfg(unix)]
#[test]
fn set_toml_preserves_private_file_mode() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir_with_mgc_toml("name = \"sample\"\necosystem = \"web\"\n");
    let path = dir.path().join("mgc.toml");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    set_toml_at(&path, "name", "updated").unwrap();

    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn is_sensitive_catches_token_keys() {
    assert!(is_sensitive("_authToken"));
    assert!(is_sensitive("npm_token"));
    assert!(is_sensitive("my_password"));
    assert!(is_sensitive("_auth"));
    assert!(is_sensitive("authorization"));
    assert!(is_sensitive("private_key"));
    assert!(is_sensitive("api_key"));
    assert!(is_sensitive("signing_key"));
    assert!(is_sensitive("private-key"));
    assert!(is_sensitive("api-key"));
    assert!(is_sensitive("registry.private-key"));
    assert!(is_sensitive("apiKey"));
    assert!(is_sensitive("APIKey"));
    assert!(is_sensitive("privateKey"));
    assert!(is_sensitive("accessKey"));
    assert!(!is_sensitive("release_public_key"));
    assert!(!is_sensitive("release-public-key"));
    assert!(!is_sensitive("releasePublicKey"));
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

#[test]
fn is_sensitive_catches_short_password_spellings() {
    for key in [
        "pass",
        "db_pass",
        "passwd",
        "user_passwd",
        "pwd",
        "deploy_pwd",
        "key",
        "api_key",
        "my_token",
        "secret",
        "auth",
        "registry_auth",
    ] {
        assert!(is_sensitive(key), "{key} must be treated as sensitive");
    }
}

#[test]
fn is_sensitive_leaves_ordinary_words_visible() {
    // `bypass_proxy` contains "pass" as a substring but is not a secret;
    // `monkey`/`keyboard` contain "key" but are ordinary words.
    for key in ["bypass_proxy", "monkey", "keyboard", "registry", "timeout"] {
        assert!(!is_sensitive(key), "{key} must stay visible");
    }
}
