//! `mgc config` — read/write configuration (pnpm config parity + mgc.toml native).
//!
//! ## Nguồn cấu hình (theo thứ tự ưu tiên — cao → thấp)
//!
//! 1. `MGC_<KEY>` / `npm_config_<KEY>` — environment variables
//! 2. `mgc.toml` project (CWD hoặc parent root) — MagiCore-native
//! 3. `.npmrc` project (CWD) — npm-compat, --local
//! 4. `.npmrc` user (~/.npmrc) — npm-compat, global default
//!
//! ## Commands
//!
//! - `mgc config get <key>`           — đọc theo thứ tự ưu tiên trên
//! - `mgc config set <key> <value>`   — ghi vào .npmrc (mặc định) hoặc mgc.toml (--toml)
//! - `mgc config delete <key>`        — xóa khỏi .npmrc hoặc mgc.toml (--toml)
//! - `mgc config unset <key>`         — alias cho delete
//! - `mgc config list`                — liệt kê tất cả key từ mọi nguồn (phân biệt nguồn)

use anyhow::Result;
use clap::Subcommand;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Sub-commands for `mgc config` (help text must stay English — RULE §7).
/// Sub-commands cho `mgc config` (text help phải giữ tiếng Anh — RULE §7).
#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCmd {
    /// Get a key value (priority: env → mgc.toml → local .npmrc → user .npmrc)
    Get {
        /// Key to read
        key: String,
    },
    /// Write a key=value pair to the config file
    Set {
        /// Key to set
        key: String,
        /// Value to set
        value: String,
        /// Write to mgc.toml instead of .npmrc
        #[arg(long, help = "write to mgc.toml instead of .npmrc")]
        toml: bool,
    },
    /// Remove a key from the config file
    Delete {
        /// Key to remove
        key: String,
        /// Remove from mgc.toml instead of .npmrc
        #[arg(long, help = "remove from mgc.toml instead of .npmrc")]
        toml: bool,
    },
    /// Alias for delete
    Unset {
        /// Key to remove
        key: String,
        /// Remove from mgc.toml instead of .npmrc
        #[arg(long, help = "remove from mgc.toml instead of .npmrc")]
        toml: bool,
    },
    /// List all config entries (shows the source of each)
    List {
        /// Only show project-local config (.npmrc local + mgc.toml)
        #[arg(long, help = "only show project-local config")]
        local: bool,
    },
}

/// Entry point — được gọi từ dispatch/common.rs
pub async fn run(cmd: ConfigCmd, global_local: bool) -> Result<()> {
    match cmd {
        ConfigCmd::Get { key } => get(&key),
        ConfigCmd::Set { key, value, toml } => {
            if toml {
                set_toml(&key, &value)
            } else {
                let path = npmrc_path(global_local)?;
                set_npmrc(&path, &key, &value)
            }
        }
        ConfigCmd::Delete { key, toml } | ConfigCmd::Unset { key, toml } => {
            if toml {
                delete_toml(&key)
            } else {
                let path = npmrc_path(global_local)?;
                delete_npmrc(&path, &key)
            }
        }
        ConfigCmd::List { local } => list(local || global_local),
    }
}

// ────────────────────────────────────────────────────────────────
// Path helpers
// ────────────────────────────────────────────────────────────────

fn npmrc_path(local: bool) -> Result<PathBuf> {
    if local {
        return Ok(std::env::current_dir()?.join(".npmrc"));
    }
    Ok(dirs::home_dir()
        .ok_or_else(crate::error::no_home_dir)?
        .join(".npmrc"))
}

/// Tìm mgc.toml project root từ CWD leo lên parent (giống find_project_root)
fn find_mgc_toml() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join("mgc.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

// ────────────────────────────────────────────────────────────────
// GET — đọc theo thứ tự ưu tiên
// ────────────────────────────────────────────────────────────────

fn get(key: &str) -> Result<()> {
    // 1. Environment variable
    if let Some(value) = env_value(key) {
        let shown = display_config_value(key, &value);
        mgc_ui::info(&format!("[env] {key} = {shown}"));
        println!("{shown}");
        return Ok(());
    }
    // 2. mgc.toml project
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Some(toml_path) = find_mgc_toml()
        && let Some(value) = toml_value(&toml_path, key)
    {
        let shown = display_config_value(key, &value);
        mgc_ui::info(&format!("[mgc.toml] {key} = {shown}"));
        println!("{shown}");
        return Ok(());
    }
    // 3. .npmrc local (project CWD)
    let project_npmrc = std::env::current_dir()?.join(".npmrc");
    if let Some(value) = file_value(&project_npmrc, key)? {
        let shown = display_config_value(key, &value);
        mgc_ui::info(&format!("[.npmrc local] {key} = {shown}"));
        println!("{shown}");
        return Ok(());
    }
    // 4. .npmrc user (~/)
    if let Some(home) = dirs::home_dir() {
        let user_npmrc = home.join(".npmrc");
        if let Some(value) = file_value(&user_npmrc, key)? {
            let shown = display_config_value(key, &value);
            mgc_ui::info(&format!("[.npmrc user] {key} = {shown}"));
            println!("{shown}");
            return Ok(());
        }
    }
    Err(crate::error::config_key_missing(key))
}

// ────────────────────────────────────────────────────────────────
// SET — ghi vào .npmrc hoặc mgc.toml
// ────────────────────────────────────────────────────────────────

fn set_npmrc(path: &Path, key: &str, value: &str) -> Result<()> {
    let new_line = format!("{key}={value}");
    let lines = read_lines(path)?;
    let mut out: Vec<String> = Vec::new();
    let mut replaced = false;
    for line in lines {
        if line.starts_with(&format!("{key}=")) {
            out.push(new_line.clone());
            replaced = true;
        } else {
            out.push(line);
        }
    }
    if !replaced {
        out.push(new_line);
    }
    write_lines(path, &out)?;
    let shown = display_config_value(key, value);
    mgc_ui::success(&format!("set {key} = {shown}  →  {}", path.display()));
    Ok(())
}

fn set_toml(key: &str, value: &str) -> Result<()> {
    let toml_path = find_mgc_toml()
        .ok_or_else(|| anyhow::anyhow!("mgc.toml not found — run `mgc init <core>` first"))?;
    set_toml_at(&toml_path, key, value)
}

fn set_toml_at(toml_path: &Path, key: &str, value: &str) -> Result<()> {
    let _project_lock = acquire_project_config_lock(toml_path)?;
    ensure_toml_key_mutable(key)?;
    ensure_regular_mgc_toml(toml_path)?;
    // Đọc raw TOML, ghi lại key theo dot-notation (vd: "ecosystem", "version", "mode")
    let content = mgc_adapter_base::project_file::read_regular_text(toml_path, "mgc.toml")?;
    let mut doc: toml_edit::DocumentMut = content.parse()?;
    // Hỗ trợ dot-notation: "game.engine" → doc["game"]["engine"]
    let parts: Vec<&str> = key.splitn(2, '.').collect();
    if parts.len() == 2 {
        let (table, field) = (parts[0], parts[1]);
        if doc.get(table).is_none() {
            doc[table] = toml_edit::table();
        }
        doc[table][field] = toml_edit::value(value);
    } else {
        doc[key] = toml_edit::value(value);
    }
    atomic_write_toml(toml_path, doc.to_string().as_bytes())?;
    let shown = display_config_value(key, value);
    mgc_ui::success(&format!("set {key} = {shown}  →  {}", toml_path.display()));
    Ok(())
}

// ────────────────────────────────────────────────────────────────
// DELETE / UNSET
// ────────────────────────────────────────────────────────────────

fn delete_npmrc(path: &Path, key: &str) -> Result<()> {
    if !path.exists() {
        return Err(crate::error::config_key_missing(key));
    }
    let lines = read_lines(path)?;
    let out: Vec<String> = lines
        .into_iter()
        .filter(|line| !line.starts_with(&format!("{key}=")))
        .collect();
    write_lines(path, &out)?;
    mgc_ui::success(&format!("unset {key}  →  {}", path.display()));
    Ok(())
}

fn delete_toml(key: &str) -> Result<()> {
    let toml_path = find_mgc_toml()
        .ok_or_else(|| anyhow::anyhow!("mgc.toml not found — run `mgc init <core>` first"))?;
    delete_toml_at(&toml_path, key)
}

fn delete_toml_at(toml_path: &Path, key: &str) -> Result<()> {
    let _project_lock = acquire_project_config_lock(toml_path)?;
    ensure_toml_key_mutable(key)?;
    ensure_regular_mgc_toml(toml_path)?;
    let content = mgc_adapter_base::project_file::read_regular_text(toml_path, "mgc.toml")?;
    let mut doc: toml_edit::DocumentMut = content.parse()?;
    let parts: Vec<&str> = key.splitn(2, '.').collect();
    if parts.len() == 2 {
        let (table, field) = (parts[0], parts[1]);
        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        if let Some(t) = doc.get_mut(table)
            && let Some(tbl) = t.as_table_like_mut()
        {
            tbl.remove(field);
        }
    } else {
        doc.remove(key);
    }
    atomic_write_toml(toml_path, doc.to_string().as_bytes())?;
    mgc_ui::success(&format!("unset {key}  →  {}", toml_path.display()));
    Ok(())
}

fn acquire_project_config_lock(
    toml_path: &Path,
) -> Result<mgc_lockfile::project_lock::ProjectWriteLock> {
    let project_root = toml_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let lock = mgc_lockfile::project_lock::ProjectWriteLock::acquire(
        project_root,
        crate::commands::core::shared::writer_lock_timeout(project_root),
    )
    .map_err(|error| crate::error::config_project_lock_failed(project_root, &error))?;
    crate::commands::core::shared::ensure_no_pending_remove_journal(project_root, &lock)?;
    Ok(lock)
}

fn ensure_toml_key_mutable(key: &str) -> Result<()> {
    if key.split('.').next() == Some("ecosystem") {
        return Err(crate::error::config_core_identity_managed_by_signature());
    }
    Ok(())
}

fn ensure_regular_mgc_toml(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(crate::error::config_project_file_symlink(path));
    }
    if !metadata.is_file() {
        anyhow::bail!("project config '{}' must be a regular file", path.display());
    }
    Ok(())
}

fn atomic_write_toml(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure_regular_mgc_toml(path)?;
    atomic_write_config_file(path, bytes)
}

// ────────────────────────────────────────────────────────────────
// LIST — hiển thị tất cả nguồn
// ────────────────────────────────────────────────────────────────

fn list(local_only: bool) -> Result<()> {
    let mut any = false;

    // 1. MGC_* env vars
    if !local_only {
        let env_keys: Vec<(String, String)> = std::env::vars()
            .filter(|(k, _)| k.starts_with("MGC_") || k.starts_with("npm_config_"))
            .collect();
        if !env_keys.is_empty() {
            println!("# [env]");
            for (k, v) in &env_keys {
                let shown = if is_sensitive(k) { "***" } else { v.as_str() };
                println!("  {k} = {shown}");
            }
            any = true;
        }
    }

    // 2. mgc.toml
    if let Some(toml_path) = find_mgc_toml() {
        let content = mgc_adapter_base::project_file::read_regular_text(&toml_path, "mgc.toml")?;
        if !content.trim().is_empty() {
            println!("# [mgc.toml] {}", toml_path.display());
            // In dạng flat (bỏ comment, chỉ key = value)
            for line in redact_toml_config(&content)?.lines() {
                println!("  {line}");
            }
            any = true;
        }
    }

    // 3. .npmrc project
    let project_npmrc = std::env::current_dir()?.join(".npmrc");
    if read_optional_config_text(&project_npmrc, ".npmrc")?.is_some() {
        println!("# [.npmrc local] {}", project_npmrc.display());
        print_npmrc_file(&project_npmrc)?;
        any = true;
    }

    // 4. .npmrc user
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if !local_only && let Some(home) = dirs::home_dir() {
        let user_npmrc = home.join(".npmrc");
        if read_optional_config_text(&user_npmrc, ".npmrc")?.is_some() {
            println!("# [.npmrc user] {}", user_npmrc.display());
            print_npmrc_file(&user_npmrc)?;
            any = true;
        }
    }

    if !any {
        mgc_ui::info("no configuration found (.npmrc / mgc.toml)");
    }
    Ok(())
}

/// Return local configuration with secrets redacted and without printing it.
/// Trả cấu hình local đã che bí mật mà không in ra stdout.
pub fn list_local_redacted() -> Result<String> {
    let mut output = String::new();
    if let Some(toml_path) = find_mgc_toml() {
        let content = mgc_adapter_base::project_file::read_regular_text(&toml_path, "mgc.toml")?;
        if !content.trim().is_empty() {
            output.push_str("# [mgc.toml] ");
            output.push_str(&toml_path.display().to_string());
            output.push('\n');
            for line in redact_toml_config(&content)?.lines() {
                output.push_str("  ");
                output.push_str(line);
                output.push('\n');
            }
        }
    }

    let project_npmrc = std::env::current_dir()?.join(".npmrc");
    if let Some(content) = read_optional_config_text(&project_npmrc, ".npmrc")? {
        output.push_str("# [.npmrc local] ");
        output.push_str(&project_npmrc.display().to_string());
        output.push('\n');
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                output.push_str("  ");
                output.push_str(key.trim());
                output.push_str(" = ");
                output.push_str(if is_sensitive(key) {
                    "***"
                } else {
                    value.trim()
                });
                output.push('\n');
            }
        }
    }

    if output.is_empty() {
        output.push_str("no configuration found (.npmrc / mgc.toml)");
    }
    Ok(output)
}

fn print_npmrc_file(path: &Path) -> Result<()> {
    let Some(content) = read_optional_config_text(path, ".npmrc")? else {
        return Ok(());
    };
    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with(';') {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            let shown = if is_sensitive(k) { "***" } else { v.trim() };
            println!("  {} = {}", k.trim(), shown);
        }
    }
    Ok(())
}

// ────────────────────────────────────────────────────────────────
// Value readers
// ────────────────────────────────────────────────────────────────

fn env_value(key: &str) -> Option<String> {
    let normalized = key.to_uppercase().replace('-', "_");
    for candidate in [
        format!("MGC_{normalized}"),
        format!("npm_config_{normalized}"),
    ] {
        if let Ok(value) = std::env::var(&candidate) {
            return Some(value);
        }
    }
    None
}

fn file_value(path: &Path, key: &str) -> Result<Option<String>> {
    let Some(content) = read_optional_config_text(path, ".npmrc")? else {
        return Ok(None);
    };
    Ok(content.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            return None;
        }
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim().to_string())
    }))
}

fn read_optional_config_text(path: &Path, display_name: &str) -> Result<Option<String>> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => mgc_adapter_base::project_file::read_regular_text(path, display_name)
            .map(Some)
            .map_err(anyhow::Error::from),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Đọc giá trị từ mgc.toml theo key đơn hoặc dot-notation (vd: "game.engine")
fn toml_value(toml_path: &Path, key: &str) -> Option<String> {
    if is_sensitive(key) {
        return Some("***".to_string());
    }
    let content = mgc_adapter_base::project_file::read_regular_text(toml_path, "mgc.toml").ok()?;
    // Redact nested credentials too: requesting a whole table must not
    // bypass the key-level protection applied to `config list`.
    // (Che cả credential lồng nhau để lấy nguyên bảng không lách được `config list`.)
    let redacted = redact_toml_config(&content).ok()?;
    let doc: toml_edit::DocumentMut = redacted.parse().ok()?;
    let parts: Vec<&str> = key.splitn(2, '.').collect();
    if parts.len() == 2 {
        let val = doc.get(parts[0])?.as_table_like()?.get(parts[1])?;
        Some(toml_val_to_string(val))
    } else {
        let val = doc.get(key)?;
        Some(toml_val_to_string(val))
    }
}

fn display_config_value<'a>(key: &str, value: &'a str) -> &'a str {
    if is_sensitive(key) { "***" } else { value }
}

fn redact_toml_config(content: &str) -> Result<String> {
    fn redact_value(value: &mut toml::Value) {
        match value {
            toml::Value::Table(table) => {
                for (key, nested) in table.iter_mut() {
                    if is_sensitive(key) {
                        *nested = toml::Value::String("***".to_string());
                    } else {
                        redact_value(nested);
                    }
                }
            }
            toml::Value::Array(values) => {
                for nested in values {
                    redact_value(nested);
                }
            }
            _ => {}
        }
    }

    let mut value: toml::Value = content.parse()?;
    redact_value(&mut value);
    Ok(toml::to_string_pretty(&value)?)
}

fn toml_val_to_string(v: &toml_edit::Item) -> String {
    match v {
        toml_edit::Item::Value(val) => val
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| val.as_integer().map(|i| i.to_string()))
            .or_else(|| val.as_bool().map(|b| b.to_string()))
            .or_else(|| val.as_float().map(|f| f.to_string()))
            .unwrap_or_else(|| val.to_string()),
        other => other.to_string(),
    }
}

// ────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────

fn is_sensitive(key: &str) -> bool {
    let mut normalized = String::with_capacity(key.len() + 4);
    let mut previous = None;
    let mut chars = key.chars().peekable();
    while let Some(character) = chars.next() {
        if !character.is_ascii_alphanumeric() {
            if !normalized.is_empty() && !normalized.ends_with('_') {
                normalized.push('_');
            }
            previous = Some(character);
            continue;
        }

        let camel_boundary = character.is_ascii_uppercase()
            && (previous
                .is_some_and(|prev: char| prev.is_ascii_lowercase() || prev.is_ascii_digit())
                || (previous.is_some_and(|prev: char| prev.is_ascii_uppercase())
                    && chars.peek().is_some_and(|next| next.is_ascii_lowercase())));
        if camel_boundary && !normalized.is_empty() && !normalized.ends_with('_') {
            normalized.push('_');
        }
        normalized.push(character.to_ascii_lowercase());
        previous = Some(character);
    }
    while normalized.ends_with('_') {
        normalized.pop();
    }
    normalized.contains("token")
        || normalized.contains("password")
        || normalized.contains("secret")
        || normalized == "auth"
        || normalized.ends_with("_auth")
        || normalized.contains("authorization")
        || (normalized.ends_with("_key") && !normalized.ends_with("_public_key"))
        // Short password spellings (`pass`, `passwd`, `pwd`) — matched as
        // whole words or suffixes only, so `bypass_proxy` stays visible.
        // (Dạng viết tắt password — chỉ khớp từ đầy đủ/hậu tố.)
        || normalized == "pass"
        || normalized.ends_with("_pass")
        || normalized.contains("passwd")
        || normalized == "pwd"
        || normalized.ends_with("_pwd")
        // A bare `key` alone is a secret name; longer words containing it
        // (`monkey`, `keyboard`) are not.
        || normalized == "key"
}

#[allow(dead_code)]
fn merge_file(merged: &mut BTreeMap<String, String>, path: &Path) {
    let Ok(Some(content)) = read_optional_config_text(path, ".npmrc") else {
        return;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        merged.insert(k.trim().to_string(), v.trim().to_string());
    }
}

fn read_lines(path: &Path) -> Result<Vec<String>> {
    let Some(content) = read_optional_config_text(path, ".npmrc")? else {
        return Ok(Vec::new());
    };
    Ok(content.lines().map(|l| l.trim_end().to_string()).collect())
}

fn write_lines(path: &Path, lines: &[String]) -> Result<()> {
    let mut out = String::new();
    for line in lines {
        if !line.is_empty() {
            out.push_str(line);
        }
        out.push('\n');
    }
    atomic_write_config_file(path, out.as_bytes())
}

fn atomic_write_config_file(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;

    #[cfg(unix)]
    let contains_credentials = std::str::from_utf8(bytes).is_ok_and(|content| {
        content.lines().any(|line| {
            line.split_once('=')
                .is_some_and(|(key, _)| is_sensitive(key.trim()))
        })
    });
    let existing = match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() {
                return Err(crate::error::config_project_file_not_regular(path));
            }
            Some(metadata)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .ok_or_else(crate::error::config_project_file_invalid_path)?;
    let temp = parent.join(format!(
        ".{}.mgc-tmp-{}-{}",
        file_name.to_string_lossy(),
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let result = (|| -> Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options.open(&temp)?;
        if let Some(metadata) = existing {
            #[cfg(unix)]
            let permissions = if contains_credentials {
                use std::os::unix::fs::PermissionsExt;
                std::fs::Permissions::from_mode(0o600)
            } else {
                metadata.permissions()
            };
            #[cfg(not(unix))]
            let permissions = metadata.permissions();
            file.set_permissions(permissions)?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        mgc_lockfile::atomic::atomic_replace_file(&temp, path)?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

// ────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "test/config.rs"]
mod tests;
