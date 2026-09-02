// Optimizer env loader — load .mgc-optimizer/*.env files for runtime config
// Bộ tải env từ optimizer — đọc file .mgc-optimizer/*.env cho runtime config

use crate::commands::optimizer::runtime_detect::DetectedRuntime;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Load optimizer env files for detected runtime
/// Tải các file env từ optimizer cho runtime được phát hiện
///
/// SAFETY: Only loads env files matching the detected runtime to avoid stale config
/// AN TOÀN: Chỉ tải file env khớp với runtime phát hiện được để tránh config cũ
pub fn load_optimizer_env(
    project_root: &Path,
    runtime: &DetectedRuntime,
) -> Result<HashMap<String, String>> {
    let optimizer_dir = project_root.join(".mgc-optimizer");
    let mut env_vars = HashMap::new();

    if !optimizer_dir.exists() {
        // No optimizer output, return empty
        // Không có output từ optimizer, trả về rỗng
        return Ok(env_vars);
    }

    // Map runtime to env file(s)
    // Ánh xạ runtime sang file env
    let env_files = get_env_files_for_runtime(runtime);

    for (filename, runtime_name) in env_files {
        let env_file = optimizer_dir.join(filename);
        if env_file.exists() {
            // Parse based on file extension
            // Phân tích dựa trên phần mở rộng file
            let vars = if filename.ends_with(".toml") {
                parse_toml_file(&env_file)
                    .with_context(|| format!("Failed to parse {} TOML file", runtime_name))?
            } else {
                parse_env_file(&env_file)
                    .with_context(|| format!("Failed to parse {} env file", runtime_name))?
            };

            if !vars.is_empty() {
                mgc_ui::info(&format!(
                    "Loaded {} optimizer config: {} variables",
                    runtime_name,
                    vars.len()
                ));
            }

            env_vars.extend(vars);
        }
    }

    Ok(env_vars)
}

/// Get env file names for a detected runtime
/// Lấy tên file env cho runtime đã phát hiện
fn get_env_files_for_runtime(runtime: &DetectedRuntime) -> Vec<(&'static str, &'static str)> {
    match runtime {
        // Web runtimes — runtime web
        DetectedRuntime::Bun => vec![("bun_env.env", "Bun")],
        DetectedRuntime::Deno => vec![("deno_env.env", "Deno")],
        DetectedRuntime::NodeJs { .. } => vec![("node_env.env", "Node.js")],

        // AI runtimes — runtime AI
        DetectedRuntime::PythonPyTorch => vec![
            ("pytorch_runtime.env", "PyTorch"),
            ("pytorch_docker.env", "PyTorch Docker"),
        ],
        DetectedRuntime::RustCandle => vec![("candle_runtime.env", "Candle")],
        DetectedRuntime::GoTensorFlow => vec![
            ("go_ai_runtime.env", "Go AI"),
            ("go_build.env", "Go Build"),
        ],

        // Lib runtimes — runtime thư viện
        DetectedRuntime::RustLib => vec![("rust_cargo_profile.toml", "Rust Lib")],
        DetectedRuntime::GoLib => vec![("go_lib_runtime.env", "Go Lib")],
        DetectedRuntime::PythonLib => vec![("python_lib_runtime.env", "Python Lib")],
        DetectedRuntime::TypeScriptLib => vec![("typescript_lib_env.env", "TypeScript Lib")],

        // App runtimes — runtime ứng dụng
        DetectedRuntime::Flutter => vec![("flutter_env.env", "Flutter")],
        DetectedRuntime::ReactNative => vec![("react_native_env.env", "React Native")],
        DetectedRuntime::RustNative => vec![("rust_cargo_profile.toml", "Rust Native")],

        // Fallback — dự phòng
        DetectedRuntime::Unknown => vec![],
    }
}

/// Parse .env file format (KEY=value lines)
/// Phân tích định dạng file .env (dòng KEY=value)
pub(crate) fn parse_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read env file: {}", path.display()))?;

    let mut vars = HashMap::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        // Bỏ qua dòng trống và comment
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse KEY=value format
        // Phân tích định dạng KEY=value
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();

            // Remove quotes if present
            // Xóa dấu ngoặc kép nếu có
            let value = if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                value[1..value.len() - 1].to_string()
            } else {
                value
            };

            vars.insert(key, value);
        }
    }

    Ok(vars)
}

/// Parse Cargo profile TOML format to extract RUSTFLAGS
/// Phân tích định dạng Cargo profile TOML để trích xuất RUSTFLAGS
fn parse_toml_file(path: &Path) -> Result<HashMap<String, String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read TOML file: {}", path.display()))?;

    let toml: toml::Value = toml::from_str(&content)
        .with_context(|| format!("Failed to parse TOML: {}", path.display()))?;

    let mut vars = HashMap::new();

    // Extract [build] rustflags = [...]
    // Trích xuất [build] rustflags
    if let Some(build) = toml.get("build") {
        if let Some(rustflags_array) = build.get("rustflags").and_then(|v| v.as_array()) {
            // Convert array to space-separated string
            // Chuyển array thành string phân cách bằng space
            let rustflags: Vec<String> = rustflags_array
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();

            if !rustflags.is_empty() {
                vars.insert("RUSTFLAGS".to_string(), rustflags.join(" "));
            }
        }
    }

    // Extract other env vars from [env] section if present
    // Trích xuất env vars khác từ section [env] nếu có
    if let Some(env_table) = toml.get("env").and_then(|v| v.as_table()) {
        for (key, value) in env_table {
            if let Some(val_str) = value.as_str() {
                vars.insert(key.clone(), val_str.to_string());
            }
        }
    }

    Ok(vars)
}

// Tests moved to test/env_loader.rs per RULE §5
// Tests đã chuyển sang test/env_loader.rs theo RULE §5
#[cfg(test)]
#[path = "test/env_loader.rs"]
mod tests;
