//! Workspace graph data types + manifest parsing.

use std::collections::HashMap;
use std::path::PathBuf;

/// Workspace package manifest — subset đủ cho graph + workspace:* edges.
#[derive(Debug, Clone, Default)]
pub struct WorkspacePackageManifest {
    pub name: String,
    pub dependencies: HashMap<String, String>,
    pub dev_dependencies: HashMap<String, String>,
    pub peer_dependencies: HashMap<String, String>,
    pub optional_dependencies: HashMap<String, String>,
}

/// 1 node graph = 1 workspace package.
#[derive(Debug, Clone)]
pub struct WorkspaceNode {
    pub name: String,
    pub path: PathBuf,
    pub manifest: WorkspacePackageManifest,
}

/// Edge from→to (workspace:* dependency).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceEdge {
    pub from: usize,
    pub to: usize,
}

/// Workspace graph: nodes index theo thứ tự targets, edges = workspace:* deps.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceGraph {
    pub nodes: Vec<WorkspaceNode>,
    pub edges: Vec<WorkspaceEdge>,
}

impl WorkspaceGraph {
    /// Cạnh đi từ node `index`.
    pub fn edges_from(&self, index: usize) -> Vec<usize> {
        self.edges
            .iter()
            .filter(|edge| edge.from == index)
            .map(|edge| edge.to)
            .collect()
    }

    /// Số node.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Đọc package.json thành WorkspacePackageManifest (None nếu thiếu file).
pub fn read_package_manifest(
    path: &std::path::Path,
) -> anyhow::Result<Option<WorkspacePackageManifest>> {
    let manifest_path = path.join("package.json");
    if !manifest_path.exists() {
        return Ok(None);
    }
    #[derive(serde::Deserialize)]
    struct Raw {
        #[serde(rename = "name")]
        name: String,
        #[serde(default)]
        dependencies: HashMap<String, String>,
        #[serde(default, rename = "devDependencies")]
        dev_dependencies: HashMap<String, String>,
        #[serde(default, rename = "peerDependencies")]
        peer_dependencies: HashMap<String, String>,
        #[serde(default, rename = "optionalDependencies")]
        optional_dependencies: HashMap<String, String>,
    }
    let contents = std::fs::read_to_string(&manifest_path)?;
    let raw: Raw = serde_json::from_str(&contents)?;
    Ok(Some(WorkspacePackageManifest {
        name: raw.name,
        dependencies: raw.dependencies,
        dev_dependencies: raw.dev_dependencies,
        peer_dependencies: raw.peer_dependencies,
        optional_dependencies: raw.optional_dependencies,
    }))
}
