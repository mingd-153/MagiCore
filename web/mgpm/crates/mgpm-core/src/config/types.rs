use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

/// Main mgpm configuration (mgpm.yaml)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MgpmConfig {
    #[serde(default)]
    pub workspace: Option<WorkspaceConfig>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub catalogs: HashMap<String, Catalog>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub overrides: HashMap<String, String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub registries: Vec<crate::protocol::RegistryConfig>,

    #[serde(default)]
    pub install: InstallConfig,

    #[serde(default)]
    pub store: StoreConfig,

    #[serde(default)]
    pub cli: CliConfig,

    #[serde(default)]
    pub trusted: Vec<String>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub scoped_registries: HashMap<String, String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_registries: Vec<String>,
}

impl Default for MgpmConfig {
    fn default() -> Self {
        Self {
            workspace: None,
            catalogs: HashMap::new(),
            overrides: HashMap::new(),
            registries: vec![crate::protocol::RegistryConfig::npm()],
            install: InstallConfig::default(),
            store: StoreConfig::default(),
            cli: CliConfig::default(),
            trusted: Vec::new(),
            scoped_registries: HashMap::new(),
            trusted_registries: Vec::new(),
        }
    }
}

impl MgpmConfig {
    pub fn load(path: &PathBuf) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io { path: path.clone(), msg: e.to_string() })?;
        toml::from_str(&content)
            .map_err(|e| ConfigError::Parse { path: path.clone(), msg: e.to_string() })
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::Serialize(e.to_string()))?;
        std::fs::write(path, content)
            .map_err(|e| ConfigError::Io { path: path.clone(), msg: e.to_string() })
    }

    pub fn get_catalog(&self, name: &str) -> Option<&Catalog> {
        if name == "default" {
            self.catalogs.get("default")
        } else {
            self.catalogs.get(name)
        }
    }
}

/// Linker mode for workspace packages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum LinkerMode {
    #[default]
    Hoisted,
    Isolated,
    PnpmLike,
}

/// Workspace configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceConfig {
    pub packages: Vec<String>,
    pub catalog: Option<String>,
    #[serde(default = "default_true")]
    pub link_ws_packages: bool,
    #[serde(default)]
    pub scripts: HashMap<String, ScriptConfig>,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub linker: LinkerMode,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            packages: Vec::new(),
            catalog: None,
            link_ws_packages: true,
            scripts: HashMap::new(),
            security: SecurityConfig::default(),
            linker: LinkerMode::default(),
        }
    }
}

/// Script configuration within workspace
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct ScriptConfig {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// Security configuration for workspace
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SecurityConfig {
    #[serde(default)]
    pub trusted_registries: Vec<String>,
    #[serde(default = "default_min_release_age")]
    pub min_release_age: String,
}

fn default_min_release_age() -> String {
    "0s".to_string()
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            trusted_registries: Vec::new(),
            min_release_age: default_min_release_age(),
        }
    }
}

fn default_true() -> bool {
    true
}

/// Package catalog for version pinning
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct Catalog {
    #[serde(default)]
    pub packages: HashMap<String, String>,
}

impl Catalog {
    pub fn get(&self, package: &str) -> Option<&String> {
        self.packages.get(package)
    }

    pub fn set(&mut self, package: &str, version: &str) {
        self.packages.insert(package.to_string(), version.to_string());
    }
}

/// Installation configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InstallConfig {
    #[serde(default)]
    pub hoist: bool,
    #[serde(default = "default_hoist_pattern")]
    pub hoist_pattern: Vec<String>,
    #[serde(default)]
    pub public_hoist_pattern: Vec<String>,
    #[serde(default = "default_true")]
    pub symlinks: bool,
    #[serde(default = "default_true")]
    pub strict_peer_deps: bool,
    #[serde(default)]
    pub auto_peer_deps: bool,
    #[serde(default)]
    pub global_virtual_store: bool,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_retries")]
    pub retries: u32,
}

fn default_hoist_pattern() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_concurrency() -> usize {
    16
}

fn default_retries() -> u32 {
    3
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            hoist: false,
            hoist_pattern: default_hoist_pattern(),
            public_hoist_pattern: Vec::new(),
            symlinks: true,
            strict_peer_deps: true,
            auto_peer_deps: false,
            global_virtual_store: false,
            concurrency: default_concurrency(),
            retries: default_retries(),
        }
    }
}

/// Store configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StoreConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_path: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub dedupe: bool,
    #[serde(default = "default_true")]
    pub verify_integrity: bool,
    #[serde(default = "default_cache_max_age")]
    pub cache_max_age: u64,
}

fn default_cache_max_age() -> u64 {
    3600
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            path: None,
            cache_path: None,
            dedupe: true,
            verify_integrity: true,
            cache_max_age: default_cache_max_age(),
        }
    }
}

impl StoreConfig {
    pub fn store_path(&self) -> PathBuf {
        self.path.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".mgpm")
                .join("store")
        })
    }

    pub fn cache_path(&self) -> PathBuf {
        self.cache_path.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".mgpm")
                .join("cache")
        })
    }

    pub fn gvs_path(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".mgpm")
            .join("gvs")
            .join("v1")
    }
}

/// CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CliConfig {
    #[serde(default = "default_true")]
    pub color: bool,
    #[serde(default)]
    pub emoji: bool,
    #[serde(default = "default_true")]
    pub progress: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub json: bool,
    #[serde(default)]
    pub dry_run: bool,
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            color: true,
            emoji: false,
            progress: true,
            log_level: default_log_level(),
            json: false,
            dry_run: false,
        }
    }
}

/// Detailed validation error
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigErrorDetail {
    pub path: PathBuf,
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub field: Option<String>,
}

/// Configuration errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    Io { path: PathBuf, msg: String },
    Parse { path: PathBuf, msg: String },
    Serialize(String),
    Validation(Vec<ConfigErrorDetail>),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, msg } => write!(f, "failed to read config from '{}': {}", path.display(), msg),
            Self::Parse { path, msg } => write!(f, "failed to parse config from '{}': {}", path.display(), msg),
            Self::Serialize(msg) => write!(f, "failed to serialize config: {}", msg),
            Self::Validation(errors) => {
                write!(f, "config validation failed ({} errors):", errors.len())?;
                for err in errors {
                    write!(f, "\n  - {}", err.message)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog() {
        let mut catalog = Catalog::default();
        catalog.set("react", "^18.2.0");
        catalog.set("typescript", "^5.0.0");
        assert_eq!(catalog.get("react"), Some(&"^18.2.0".to_string()));
        assert_eq!(catalog.get("missing"), None);
    }

    #[test]
    fn test_store_path() {
        let store = StoreConfig::default();
        let path = store.store_path();
        assert!(path.to_str().unwrap().contains(".mgpm"));
    }

    #[test]
    fn test_linker_mode_default() {
        assert_eq!(LinkerMode::default(), LinkerMode::Hoisted);
    }
}
