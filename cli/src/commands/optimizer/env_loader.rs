// Optimizer env loader — load .mgc-optimizer/*.env files for runtime config
// Bộ tải env từ optimizer — đọc file .mgc-optimizer/*.env cho runtime config

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Load optimizer env files for detected runtime
/// Tải các file env từ optimizer cho runtime được phát hiện
pub fn load_optimizer_env(project_root: &Path) -> Result<HashMap<String, String>> {
    let optimizer_dir = project_root.join(".mgc-optimizer");
    let mut env_vars = HashMap::new();

    if !optimizer_dir.exists() {
        // No optimizer output, return empty
        // Không có output từ optimizer, trả về rỗng
        return Ok(env_vars);
    }

    // Check for runtime-specific env files
    // Kiểm tra các file env theo runtime
    let candidates = vec![
        ("bun_env.env", "Bun"),
        ("deno_env.env", "Deno"),
        ("node_env.env", "Node.js"),
    ];

    for (filename, runtime_name) in candidates {
        let env_file = optimizer_dir.join(filename);
        if env_file.exists() {
            let vars = parse_env_file(&env_file)
                .with_context(|| format!("Failed to parse {} env file", runtime_name))?;
            
            mgc_ui::info(&format!(
                "Loaded {} optimizer config: {} variables",
                runtime_name,
                vars.len()
            ));
            
            env_vars.extend(vars);
        }
    }

    Ok(env_vars)
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

// Tests moved to test/env_loader.rs per RULE §5
// Tests đã chuyển sang test/env_loader.rs theo RULE §5
#[cfg(test)]
#[path = "test/env_loader.rs"]
mod tests;
