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

/// Đọc package.json / Cargo.toml / pyproject.toml thành WorkspacePackageManifest.
pub fn read_package_manifest(
    path: &std::path::Path,
) -> anyhow::Result<Option<WorkspacePackageManifest>> {
    // 1. Check Node/Web package.json
    let pkg_json_path = path.join("package.json");
    if pkg_json_path.exists() {
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
        let contents = std::fs::read_to_string(&pkg_json_path)?;
        let raw: Raw = serde_json::from_str(&contents)?;
        return Ok(Some(WorkspacePackageManifest {
            name: raw.name,
            dependencies: raw.dependencies,
            dev_dependencies: raw.dev_dependencies,
            peer_dependencies: raw.peer_dependencies,
            optional_dependencies: raw.optional_dependencies,
        }));
    }

    // 2. Check Rust Cargo.toml
    let cargo_path = path.join("Cargo.toml");
    if cargo_path.exists() {
        #[derive(serde::Deserialize)]
        struct CargoRaw {
            package: Option<CargoPackage>,
            #[serde(default)]
            dependencies: HashMap<String, toml::Value>,
        }
        #[derive(serde::Deserialize)]
        struct CargoPackage {
            name: String,
        }
        let contents = std::fs::read_to_string(&cargo_path)?;
        if let Ok(raw) = toml::from_str::<CargoRaw>(&contents) {
            if let Some(pkg) = raw.package {
                let mut deps = HashMap::new();
                for (k, v) in raw.dependencies {
                    let val_str = match v {
                        toml::Value::String(s) => s,
                        toml::Value::Table(t) => {
                            if t.contains_key("path") {
                                "workspace:*".to_string()
                            } else {
                                t.get("version")
                                    .and_then(|ver| ver.as_str())
                                    .unwrap_or("*")
                                    .to_string()
                            }
                        }
                        _ => "*".to_string(),
                    };
                    deps.insert(k, val_str);
                }
                return Ok(Some(WorkspacePackageManifest {
                    name: pkg.name,
                    dependencies: deps,
                    dev_dependencies: HashMap::new(),
                    peer_dependencies: HashMap::new(),
                    optional_dependencies: HashMap::new(),
                }));
            }
        }
    }

    // 3. Check Python pyproject.toml
    let py_path = path.join("pyproject.toml");
    if py_path.exists() {
        #[derive(serde::Deserialize)]
        struct PyRaw {
            project: Option<PyProject>,
        }
        #[derive(serde::Deserialize)]
        struct PyProject {
            name: String,
        }
        let contents = std::fs::read_to_string(&py_path)?;
        if let Ok(raw) = toml::from_str::<PyRaw>(&contents) {
            if let Some(proj) = raw.project {
                return Ok(Some(WorkspacePackageManifest {
                    name: proj.name,
                    dependencies: HashMap::new(),
                    dev_dependencies: HashMap::new(),
                    peer_dependencies: HashMap::new(),
                    optional_dependencies: HashMap::new(),
                }));
            }
        }
    }

    Ok(None)
}
