use std::path::Path;
use crate::config::{ConfigError, ConfigLoader, MgpmConfig};

/// Loads configuration from YAML files
pub struct YamlLoader;

impl ConfigLoader for YamlLoader {
    fn load(&self, path: &Path) -> Result<MgpmConfig, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io { path: path.to_path_buf(), msg: e.to_string() })?;
        serde_yaml::from_str(&content).map_err(|e| {
            let line = e.location().map(|loc| loc.line());
            let column = e.location().map(|loc| loc.column());
            ConfigError::Parse {
                path: path.to_path_buf(),
                msg: e.to_string(),
                line,
                column,
            }
        })
    }
}

/// Loads configuration from TOML files
pub struct TomlLoader;

impl ConfigLoader for TomlLoader {
    fn load(&self, path: &Path) -> Result<MgpmConfig, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io { path: path.to_path_buf(), msg: e.to_string() })?;
        toml::from_str(&content).map_err(|e| {
            ConfigError::Parse {
                path: path.to_path_buf(),
                msg: e.to_string(),
                line: None,
                column: None,
            }
        })
    }
}

/// Auto-detects format from file extension and delegates to the appropriate loader
pub struct AutoConfigLoader;

impl ConfigLoader for AutoConfigLoader {
    fn load(&self, path: &Path) -> Result<MgpmConfig, ConfigError> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "yaml" | "yml" => YamlLoader.load(path),
            _ => TomlLoader.load(path),
        }
    }
}
