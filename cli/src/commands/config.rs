//! `mg config` — read/write .npmrc configuration (pnpm config parity).
//! get/set/delete/list trên user ~/.npmrc (mặc định) hoặc project .npmrc (--local).

use anyhow::Result;
use clap::Subcommand;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCmd {
    Get { key: String },
    Set { key: String, value: String },
    Delete { key: String },
    List,
}

pub async fn run(cmd: ConfigCmd, local: bool) -> Result<()> {
    let target = npmrc_path(local)?;
    match cmd {
        ConfigCmd::Get { key } => get(&key),
        ConfigCmd::Set { key, value } => set(&target, &key, &value),
        ConfigCmd::Delete { key } => delete(&target, &key),
        ConfigCmd::List => list(local),
    }
}

fn npmrc_path(local: bool) -> Result<PathBuf> {
    if local {
        return Ok(std::env::current_dir()?.join(".npmrc"));
    }
    Ok(dirs::home_dir()
        .ok_or_else(crate::error::no_home_dir)?
        .join(".npmrc"))
}

fn get(key: &str) -> Result<()> {
    if let Some(value) = env_value(key) {
        println!("{value}");
        return Ok(());
    }
    let project = std::env::current_dir()?.join(".npmrc");
    if let Some(value) = file_value(&project, key) {
        println!("{value}");
        return Ok(());
    }
    if let Some(home) = dirs::home_dir() {
        let user = home.join(".npmrc");
        if let Some(value) = file_value(&user, key) {
            println!("{value}");
            return Ok(());
        }
    }
    Err(crate::error::config_key_missing(key))
}

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

fn set(path: &Path, key: &str, value: &str) -> Result<()> {
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
    mg_ui::success(&format!(
        "{} = {}  →  {}",
        key,
        value,
        path.display()
    ));
    Ok(())
}

fn delete(path: &Path, key: &str) -> Result<()> {
    if !path.exists() {
        return Err(crate::error::config_key_missing(key));
    }
    let lines = read_lines(path)?;
    let out: Vec<String> = lines
        .into_iter()
        .filter(|line| !line.starts_with(&format!("{key}=")))
        .collect();
    write_lines(path, &out)?;
    mg_ui::success(&format!("{key} deleted from {}", path.display()));
    Ok(())
}

fn list(local: bool) -> Result<()> {
    let project = std::env::current_dir()?.join(".npmrc");
    let mut merged: BTreeMap<String, String> = BTreeMap::new();
    if let Some(home) = dirs::home_dir() {
        merge_file(&mut merged, &home.join(".npmrc"));
    }
    if local || project.exists() {
        merge_file(&mut merged, &project);
    }
    if merged.is_empty() {
        mg_ui::info("no configuration found (no .npmrc files)");
        return Ok(());
    }
    for (key, value) in &merged {
        let shown = if is_sensitive(key) {
            "***"
        } else {
            value
        };
        println!("{key} = {shown}");
    }
    Ok(())
}

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
