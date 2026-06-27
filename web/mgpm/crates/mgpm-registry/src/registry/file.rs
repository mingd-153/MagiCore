//! File/Link/Workspace Registry

use std::fs;
use std::path::{Path, PathBuf};
use mgpm_core::{PackageName, Version};
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
        // Look for workspace package in mgpm.yaml or package.json
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
