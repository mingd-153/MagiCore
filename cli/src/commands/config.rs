//! `mg config` — read/write configuration (pnpm config parity + mg.toml native).
//!
//! ## Nguồn cấu hình (theo thứ tự ưu tiên — cao → thấp)
//!
//! 1. `MG_<KEY>` / `npm_config_<KEY>` — environment variables
//! 2. `mg.toml` project (CWD hoặc parent root) — MegaGate-native
//! 3. `.npmrc` project (CWD) — npm-compat, --local
//! 4. `.npmrc` user (~/.npmrc) — npm-compat, global default
//!
//! ## Commands
//!
//! - `mg config get <key>`           — đọc theo thứ tự ưu tiên trên
//! - `mg config set <key> <value>`   — ghi vào .npmrc (mặc định) hoặc mg.toml (--toml)
//! - `mg config delete <key>`        — xóa khỏi .npmrc hoặc mg.toml (--toml)
//! - `mg config unset <key>`         — alias cho delete
//! - `mg config list`                — liệt kê tất cả key từ mọi nguồn (phân biệt nguồn)

use anyhow::Result;
use clap::Subcommand;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Sub-commands cho `mg config`
#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCmd {
    /// Lấy giá trị một key (ưu tiên: env → mg.toml → .npmrc local → .npmrc user)
    Get {
        /// Tên key cần lấy
        key: String,
    },
    /// Ghi một key=value vào file cấu hình
    Set {
        /// Tên key
        key: String,
        /// Giá trị cần set
        value: String,
        /// Ghi vào mg.toml thay vì .npmrc
        #[arg(long, help = "write to mg.toml instead of .npmrc")]
        toml: bool,
    },
    /// Xóa một key khỏi file cấu hình
    Delete {
        /// Tên key cần xóa
        key: String,
        /// Xóa từ mg.toml thay vì .npmrc
        #[arg(long, help = "remove from mg.toml instead of .npmrc")]
        toml: bool,
    },
    /// Alias for delete
    Unset {
        /// Tên key cần xóa
        key: String,
        /// Xóa từ mg.toml thay vì .npmrc
        #[arg(long, help = "remove from mg.toml instead of .npmrc")]
        toml: bool,
    },
    /// Liệt kê tất cả cấu hình (hiện thị nguồn gốc)
    List {
        /// Chỉ hiển thị config trong project (.npmrc local + mg.toml)
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

/// Tìm mg.toml project root từ CWD leo lên parent (giống find_project_root)
fn find_mg_toml() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join("mg.toml");
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
        mg_ui::info(&format!("[env] {key} = {value}"));
        println!("{value}");
        return Ok(());
    }
    // 2. mg.toml project
    if let Some(toml_path) = find_mg_toml() {
        if let Some(value) = toml_value(&toml_path, key) {
            mg_ui::info(&format!("[mg.toml] {key} = {value}"));
            println!("{value}");
            return Ok(());
        }
    }
    // 3. .npmrc local (project CWD)
    let project_npmrc = std::env::current_dir()?.join(".npmrc");
    if let Some(value) = file_value(&project_npmrc, key) {
        mg_ui::info(&format!("[.npmrc local] {key} = {value}"));
        println!("{value}");
        return Ok(());
    }
    // 4. .npmrc user (~/)
    if let Some(home) = dirs::home_dir() {
        let user_npmrc = home.join(".npmrc");
        if let Some(value) = file_value(&user_npmrc, key) {
            mg_ui::info(&format!("[.npmrc user] {key} = {value}"));
            println!("{value}");
            return Ok(());
        }
    }
    Err(crate::error::config_key_missing(key))
}

// ────────────────────────────────────────────────────────────────
// SET — ghi vào .npmrc hoặc mg.toml
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
    mg_ui::success(&format!("set {key} = {value}  →  {}", path.display()));
    Ok(())
}

fn set_toml(key: &str, value: &str) -> Result<()> {
    let toml_path = find_mg_toml()
        .ok_or_else(|| anyhow::anyhow!("mg.toml not found — run `mg init <core>` first"))?;
    // Đọc raw TOML, ghi lại key theo dot-notation (vd: "ecosystem", "version", "mode")
    let content = std::fs::read_to_string(&toml_path)?;
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
    std::fs::write(&toml_path, doc.to_string())?;
    mg_ui::success(&format!(
        "set {key} = {value}  →  {}",
        toml_path.display()
    ));
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
    mg_ui::success(&format!("unset {key}  →  {}", path.display()));
    Ok(())
}

fn delete_toml(key: &str) -> Result<()> {
    let toml_path = find_mg_toml()
        .ok_or_else(|| anyhow::anyhow!("mg.toml not found — run `mg init <core>` first"))?;
    let content = std::fs::read_to_string(&toml_path)?;
    let mut doc: toml_edit::DocumentMut = content.parse()?;
    let parts: Vec<&str> = key.splitn(2, '.').collect();
    if parts.len() == 2 {
        let (table, field) = (parts[0], parts[1]);
        if let Some(t) = doc.get_mut(table) {
            if let Some(tbl) = t.as_table_like_mut() {
                tbl.remove(field);
            }
        }
    } else {
        doc.remove(key);
    }
    std::fs::write(&toml_path, doc.to_string())?;
    mg_ui::success(&format!("unset {key}  →  {}", toml_path.display()));
    Ok(())
}

// ────────────────────────────────────────────────────────────────
// LIST — hiển thị tất cả nguồn
// ────────────────────────────────────────────────────────────────

fn list(local_only: bool) -> Result<()> {
    let mut any = false;

    // 1. MG_* env vars
    if !local_only {
        let env_keys: Vec<(String, String)> = std::env::vars()
            .filter(|(k, _)| k.starts_with("MG_") || k.starts_with("npm_config_"))
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

    // 2. mg.toml
    if let Some(toml_path) = find_mg_toml() {
        let content = std::fs::read_to_string(&toml_path).unwrap_or_default();
        if !content.trim().is_empty() {
            println!("# [mg.toml] {}", toml_path.display());
            // In dạng flat (bỏ comment, chỉ key = value)
            for line in content.lines() {
                let t = line.trim();
                if t.is_empty() || t.starts_with('#') {
                    continue;
                }
                println!("  {t}");
            }
            any = true;
        }
    }

    // 3. .npmrc project
    let project_npmrc = std::env::current_dir()?.join(".npmrc");
    if project_npmrc.exists() {
        println!("# [.npmrc local] {}", project_npmrc.display());
        print_npmrc_file(&project_npmrc);
        any = true;
    }

    // 4. .npmrc user
    if !local_only {
        if let Some(home) = dirs::home_dir() {
            let user_npmrc = home.join(".npmrc");
            if user_npmrc.exists() {
                println!("# [.npmrc user] {}", user_npmrc.display());
                print_npmrc_file(&user_npmrc);
                any = true;
            }
        }
    }

    if !any {
        mg_ui::info("no configuration found (.npmrc / mg.toml)");
    }
    Ok(())
}

fn print_npmrc_file(path: &Path) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
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
}

// ────────────────────────────────────────────────────────────────
// Value readers
// ────────────────────────────────────────────────────────────────

fn env_value(key: &str) -> Option<String> {
    let normalized = key.to_uppercase().replace('-', "_");
    for candidate in [format!("MG_{normalized}"), format!("npm_config_{normalized}")] {
        if let Ok(value) = std::env::var(&candidate) {
            return Some(value);
        }
    }
    None
}

fn file_value(path: &Path, key: &str) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    content.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            return None;
        }
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim().to_string())
    })
}

/// Đọc giá trị từ mg.toml theo key đơn hoặc dot-notation (vd: "game.engine")
fn toml_value(toml_path: &Path, key: &str) -> Option<String> {
    let content = std::fs::read_to_string(toml_path).ok()?;
    let doc: toml_edit::DocumentMut = content.parse().ok()?;
    let parts: Vec<&str> = key.splitn(2, '.').collect();
    if parts.len() == 2 {
        let val = doc.get(parts[0])?.as_table_like()?.get(parts[1])?;
        Some(toml_val_to_string(val))
    } else {
        let val = doc.get(key)?;
        Some(toml_val_to_string(val))
    }
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
    key.contains("_authToken") || key.contains("_password") || key.contains("token")
}

fn merge_file(merged: &mut BTreeMap<String, String>, path: &Path) {
    if !path.exists() {
        return;
    }
    let Ok(content) = std::fs::read_to_string(path) else {
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
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)?;
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
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, out)?;
    Ok(())
}

// ────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "test/config.rs"]
mod tests;
