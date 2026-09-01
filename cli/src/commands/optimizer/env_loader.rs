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
fn parse_env_file(path: &Path) -> Result<HashMap<String, String>> {
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
