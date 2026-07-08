//! Universal project manifest — what a project needs to install

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::package::DependencySpec;
use crate::version::Version;
use crate::ecosystem::Ecosystem;

/// A unified project manifest parsed from package.json / Cargo.toml /
/// pyproject.toml / manifest.json / etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Project name
    pub name: String,
    /// Project version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<Version>,
    /// Which ecosystem/adapter handles this project
    pub ecosystem: Ecosystem,
    /// Runtime dependencies
    #[serde(default)]
    pub dependencies: Vec<DependencySpec>,
    /// Dev-only dependencies
    #[serde(default)]
    pub dev_dependencies: Vec<DependencySpec>,
    /// Peer dependencies (for libraries)
    #[serde(default)]
    pub peer_dependencies: Vec<DependencySpec>,
    /// Optional dependencies
    #[serde(default)]
    pub optional_dependencies: Vec<DependencySpec>,
    /// Workspace members (monorepo)
    #[serde(default)]
    pub workspace_members: Vec<String>,
    /// Extra metadata (ecosystem-specific raw fields)
    #[serde(default)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Manifest {
    pub fn new(name: impl Into<String>, ecosystem: Ecosystem) -> Self {
        Self {
            name: name.into(),
            version: None,
            ecosystem,
            dependencies: Vec::new(),
            dev_dependencies: Vec::new(),
            peer_dependencies: Vec::new(),
            optional_dependencies: Vec::new(),
            workspace_members: Vec::new(),
            extra: HashMap::new(),
        }
    }

    /// All dependencies (runtime + dev + optional, deduplicated by name)
    pub fn all_dependencies(&self) -> impl Iterator<Item = &DependencySpec> {
        self.dependencies.iter()
            .chain(self.dev_dependencies.iter())
            .chain(self.optional_dependencies.iter())
    }

    /// True if project has no dependencies at all
    pub fn is_empty(&self) -> bool {
        self.dependencies.is_empty()
            && self.dev_dependencies.is_empty()
            && self.optional_dependencies.is_empty()
    }

    /// Find a dependency by name (searches all dep groups)
    pub fn find_dep(&self, name: &str) -> Option<&DependencySpec> {
        self.all_dependencies().find(|d| d.name.as_str() == name)
    }

    /// Is this a monorepo / workspace?
    pub fn is_workspace(&self) -> bool {
        !self.workspace_members.is_empty()
    }
}
