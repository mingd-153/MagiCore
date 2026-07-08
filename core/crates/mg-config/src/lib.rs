/// Configuration management for MegaGate
/// 
/// Handles user config, project config, and registry configuration.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod registry;
pub mod project;
pub mod user;

/// Registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    pub url: String,
    pub auth_token: Option<String>,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: "https://registry.megagate.io".to_string(),
            auth_token: None,
        }
    }
}

/// User configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Registry configuration
    pub registry: RegistryConfig,
    /// Store directory
    pub store_dir: Option<PathBuf>,
    /// Cache directory
    pub cache_dir: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            registry: RegistryConfig::default(),
            store_dir: None,
            cache_dir: None,
        }
    }
}

impl Config {
    /// Load configuration from file
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get default config path
    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
        Ok(home.join(".megagate").join("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.registry.url, "https://registry.megagate.io");
    }
}
