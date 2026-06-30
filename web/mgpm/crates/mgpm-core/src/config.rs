//! Configuration types for mgpm
//!
//! Configuration is loaded from:
//! 1. `mgpm.yaml` in project root
//! 2. `mgpm.lock` (lockfile)
//! 3. Environment variables
//! 4. CLI flags

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Main mgpm configuration (mgpm.yaml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MgpmConfig {
    /// Workspace configuration
    #[serde(default)]
    pub workspace: Option<WorkspaceConfig>,
    
    /// Package catalogs for version pinning
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub catalogs: HashMap<String, Catalog>,
    
    /// Dependency overrides
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub overrides: HashMap<String, String>,
    
    /// Registry configuration
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub registries: Vec<super::protocol::RegistryConfig>,
    
    /// Installation options
    #[serde(default)]
    pub install: InstallConfig,
    
    /// Store configuration
    #[serde(default)]
    pub store: StoreConfig,
    
    /// CLI options
    #[serde(default)]
    pub cli: CliConfig,

    /// Trusted packages (skip signature verify)
    #[serde(default)]
    pub trusted: Vec<String>,

    /// Scoped registry mapping (@scope -> registry_url)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub scoped_registries: HashMap<String, String>,

    /// Allowed registries for resolution (dependency confusion prevention)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_registries: Vec<String>,
}

impl Default for MgpmConfig {
    fn default() -> Self {
        Self {
            workspace: None,
            catalogs: HashMap::new(),
            overrides: HashMap::new(),
            registries: vec![super::protocol::RegistryConfig::npm()],
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
    /// Loads configuration from a file.
    pub fn load(path: &PathBuf) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io { path: path.clone(), msg: e.to_string() })?;
        
        toml::from_str(&content)
            .map_err(|e| ConfigError::Parse { path: path.clone(), msg: e.to_string() })
    }

    /// Saves configuration to a file.
    pub fn save(&self, path: &PathBuf) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::Serialize(e.to_string()))?;
        
        std::fs::write(path, content)
            .map_err(|e| ConfigError::Io { path: path.clone(), msg: e.to_string() })
    }

    /// Returns the catalog with the given name, or the default catalog.
    pub fn get_catalog(&self, name: &str) -> Option<&Catalog> {
        if name == "default" {
            self.catalogs.get("default")
        } else {
            self.catalogs.get(name)
        }
    }
}

/// Workspace configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// List of workspace patterns (globs)
    pub packages: Vec<String>,
    /// Catalog to use for this workspace
    pub catalog: Option<String>,
    /// Link workspace packages directly (no copies)
    #[serde(default = "default_true")]
    pub link_ws_packages: bool,
}

fn default_true() -> bool {
    true
}

/// A package catalog for version pinning
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Catalog {
    /// Package version specifications
    #[serde(default)]
    pub packages: HashMap<String, String>,
}

impl Catalog {
    /// Gets the version for a package.
    pub fn get(&self, package: &str) -> Option<&String> {
        self.packages.get(package)
    }

    /// Sets a package version.
    pub fn set(&mut self, package: &str, version: &str) {
        self.packages.insert(package.to_string(), version.to_string());
    }
}

/// Installation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallConfig {
    /// Hoist all packages to node_modules root (legacy compatibility)
    #[serde(default)]
    pub hoist: bool,
    
    /// Hoist pattern for packages to hoist (glob)
    #[serde(default = "default_hoist_pattern")]
    pub hoist_pattern: Vec<String>,
    
    /// Public hoisted packages
    #[serde(default)]
    pub public_hoist_pattern: Vec<String>,
    
    /// Enable symlinks for hoisted packages
    #[serde(default = "default_true")]
    pub symlinks: bool,
    
    /// Strict peer dependencies
    #[serde(default = "default_true")]
    pub strict_peer_deps: bool,
    
    /// Auto-install peer dependencies
    #[serde(default)]
    pub auto_peer_deps: bool,
    
    /// Enable global virtual store
    #[serde(default)]
    pub global_virtual_store: bool,
    
    /// Concurrency limit for downloads
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    
    /// Retry count for failed downloads
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreConfig {
    /// Path to the global store (default: ~/.mgpm/store)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    
    /// Cache directory (default: ~/.mgpm/cache)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_path: Option<PathBuf>,
    
    /// Enable content-addressable deduplication
    #[serde(default = "default_true")]
    pub dedupe: bool,
    
    /// Verify integrity on import
    #[serde(default = "default_true")]
    pub verify_integrity: bool,
    
    /// Max age for cached metadata (seconds)
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
    /// Returns the store path, or the default.
    pub fn store_path(&self) -> PathBuf {
        self.path.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".mgpm")
                .join("store")
        })
    }

    /// Returns the cache path, or the default.
    pub fn cache_path(&self) -> PathBuf {
        self.cache_path.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".mgpm")
                .join("cache")
        })
    }

    /// Returns the global virtual store path (~/.mgpm/gvs/v1).
    pub fn gvs_path(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".mgpm")
            .join("gvs")
            .join("v1")
    }
}

/// CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Use color output
    #[serde(default = "default_true")]
    pub color: bool,
    
    /// Use emoji in output
    #[serde(default)]
    pub emoji: bool,
    
    /// Progress display
    #[serde(default = "default_true")]
    pub progress: bool,
    
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub log_level: String,
    
    /// JSON output
    #[serde(default)]
    pub json: bool,
    
    /// Dry run mode
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

/// Configuration errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    Io { path: PathBuf, msg: String },
    Parse { path: PathBuf, msg: String },
    Serialize(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, msg } => write!(f, "failed to read config from '{}': {}", path.display(), msg),
            Self::Parse { path, msg } => write!(f, "failed to parse config from '{}': {}", path.display(), msg),
            Self::Serialize(msg) => write!(f, "failed to serialize config: {}", msg),
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
}