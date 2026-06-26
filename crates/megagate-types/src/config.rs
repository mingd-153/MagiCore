use crate::error::{MegagateError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MegagateConfig {
    pub store_dir: PathBuf,
    pub registry: String,
    pub minimum_release_age_hours: u32,
    pub approve_builds: bool,
    pub lockdown_mode: bool,
    pub link_strategy: LinkStrategy,
    pub max_concurrency: usize,
    pub offline_mode: bool,
    pub prefer_offline: bool,
    pub workspace: Option<WorkspaceConfig>,
}

impl Default for MegagateConfig {
    fn default() -> Self {
        Self {
            store_dir: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".megagate")
                .join("store"),
            registry: "https://registry.npmjs.org".to_string(),
            minimum_release_age_hours: 24,
            approve_builds: false,
            lockdown_mode: false,
            link_strategy: LinkStrategy::Symlink,
            max_concurrency: 16,
            offline_mode: false,
            prefer_offline: false,
            workspace: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkStrategy {
    Hardlink,
    Symlink,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub packages: Vec<String>,
    pub catalog: HashMap<String, String>,
    pub overrides: HashMap<String, String>,
    pub link_workspace_packages: LinkWorkspacePackages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkWorkspacePackages {
    Shallow,
    Deep,
    False,
}

impl MegagateConfig {
    pub fn load_from_path(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| MegagateError::ConfigError(e.to_string()))?;
        let config: Self = toml::from_str(&content)
            .map_err(|e| MegagateError::ConfigError(e.to_string()))?;
        Ok(config)
    }

    pub fn load_global() -> Result<Self> {
        let global_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".megagaterc");
        if global_path.exists() {
            Self::load_from_path(&global_path)
        } else {
            Ok(Self::default())
        }
    }

    pub fn merge_with_global(&mut self, global: MegagateConfig) {
        if self.registry == MegagateConfig::default().registry {
            self.registry = global.registry;
        }
        if self.store_dir == MegagateConfig::default().store_dir {
            self.store_dir = global.store_dir;
        }
    }
}