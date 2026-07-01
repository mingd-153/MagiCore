use std::path::Path;

pub mod types;
pub mod validate;
pub mod schema;

pub use types::*;

/// Trait for loading configuration from different sources
pub trait ConfigLoader {
    fn load(&self, path: &Path) -> Result<MgpmConfig, ConfigError>;

    fn load_with_env(&self, path: &Path) -> Result<MgpmConfig, ConfigError> {
        let mut config = self.load(path)?;
        Self::apply_env(&mut config);
        Ok(config)
    }

    fn apply_env(config: &mut MgpmConfig) {
        for (key, val) in std::env::vars() {
            if !key.starts_with("MGPM_") {
                continue;
            }
            let config_key = key.trim_start_matches("MGPM_").to_lowercase();
            match config_key.as_str() {
                "registry" => {
                    if let Some(reg) = config.registries.first_mut() {
                        reg.url = val;
                    }
                }
                "offline" => config.cli.dry_run = val == "true" || val == "1",
                "concurrency" => {
                    if let Ok(n) = val.parse::<usize>() {
                        config.install.concurrency = n;
                    }
                }
                "cache_max_age" => {
                    if let Ok(n) = val.parse::<u64>() {
                        config.store.cache_max_age = n;
                    }
                }
                _ => {}
            }
        }
    }
}

/// Default loader that auto-detects format from file extension
pub struct DefaultConfigLoader;

impl ConfigLoader for DefaultConfigLoader {
    fn load(&self, path: &Path) -> Result<MgpmConfig, ConfigError> {
        MgpmConfig::load(&path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_override() {
        let mut config = MgpmConfig::default();
        std::env::set_var("MGPM_OFFLINE", "true");
        DefaultConfigLoader::apply_env(&mut config);
        assert!(config.cli.dry_run);
        std::env::remove_var("MGPM_OFFLINE");
    }

    #[test]
    fn test_loader_yaml() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mgpm.toml");
        let content = r#"
workspace = { packages = ["packages/*", "apps/*"], linker = "hoisted" }
"#;
        std::fs::write(&path, content).unwrap();
        let loader = DefaultConfigLoader;
        let config = loader.load(&path).unwrap();
        let ws = config.workspace.unwrap();
        assert_eq!(ws.packages.len(), 2);
        assert_eq!(ws.linker, LinkerMode::Hoisted);
    }
}
