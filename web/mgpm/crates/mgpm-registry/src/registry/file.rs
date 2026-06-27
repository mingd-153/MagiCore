use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use mgpm_core::PackageName;
use crate::registry::RegistryError;

pub struct FileRegistry;

impl FileRegistry {
    pub fn resolve(&self, path: &Path) -> Result<PathBuf, RegistryError> {
        let canonical = fs::canonicalize(path)?;
        Ok(canonical)
    }
}

pub struct WorkspaceRegistry {
    workspace_root: PathBuf,
}

impl WorkspaceRegistry {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    pub fn resolve(&self, name: &PackageName) -> Result<PathBuf, RegistryError> {
        let candidates = [
            self.workspace_root.join("packages").join(name.as_str()),
            self.workspace_root.join(name.as_str()),
        ];

        for candidate in candidates {
            if candidate.join("package.json").exists() {
                return Ok(candidate);
            }
        }

        Err(RegistryError::NotFound(format!("workspace package: {}", name.as_str())))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedPackageJson {
    pub name: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub dependencies: Option<HashMap<String, String>>,
    #[serde(default)]
    pub dev_dependencies: Option<HashMap<String, String>>,
    #[serde(default)]
    pub peer_dependencies: Option<HashMap<String, String>>,
    #[serde(default)]
    pub optional_dependencies: Option<HashMap<String, String>>,
    #[serde(default)]
    pub scripts: Option<HashMap<String, String>>,
    #[serde(default)]
    pub workspaces: Option<Vec<String>>,
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub types: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub bin: Option<HashMap<String, String>>,
    #[serde(default)]
    pub exports: Option<serde_json::Value>,
    #[serde(default)]
    pub private: Option<bool>,
}

pub struct PackageJsonReader;

impl PackageJsonReader {
    pub fn read(path: &Path) -> Result<ParsedPackageJson, RegistryError> {
        let content = fs::read_to_string(path)?;
        let parsed: ParsedPackageJson = serde_json::from_str(&content)
            .map_err(|e| RegistryError::NetworkError(format!("invalid package.json: {}", e)))?;
        Ok(parsed)
    }
}
