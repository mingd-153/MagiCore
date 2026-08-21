//! Workspace discovery — scan apps/ + packages/ (layout từ megagate.workspace.toml).

use crate::{
    read_package_manifest, WorkspaceEdge, WorkspaceGraph, WorkspaceNode, WorkspacePackageManifest,
};
use std::path::{Path, PathBuf};

/// Layout dirs — mặc định apps/ + packages/, đọc từ megagate.workspace.toml [layout].
#[derive(Debug, Clone)]
pub struct DiscoverOptions {
    pub apps_dir: String,
    pub packages_dir: String,
}

/// Đọc cấu hình workspace layout từ megagate.workspace.toml.
pub fn discover_workspace_targets(project_root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let options = workspace_layout(project_root)?;
    let mut targets = Vec::new();
    collect_projects(project_root.join(&options.apps_dir), &mut targets)?;
    collect_projects(project_root.join(&options.packages_dir), &mut targets)?;
    targets.sort();
    targets.dedup();
    Ok(targets)
}

/// Đọc layout config; không có file → mặc định apps/ + packages/.
pub fn workspace_layout(project_root: &Path) -> anyhow::Result<DiscoverOptions> {
    let workspace_path = project_root.join("megagate.workspace.toml");
    if !workspace_path.exists() {
        return Ok(DiscoverOptions {
            apps_dir: "apps".to_string(),
            packages_dir: "packages".to_string(),
        });
    }
    #[derive(serde::Deserialize)]
    struct Config {
        #[serde(default)]
        layout: Option<Layout>,
    }
    #[derive(serde::Deserialize)]
    struct Layout {
        #[serde(default)]
        apps_dir: Option<String>,
        #[serde(default)]
        packages_dir: Option<String>,
    }
    let contents = std::fs::read_to_string(&workspace_path)?;
    let config: Config = toml::from_str(&contents)?;
    Ok(DiscoverOptions {
        apps_dir: config
            .layout
            .as_ref()
            .and_then(|l| l.apps_dir.clone())
            .unwrap_or_else(|| "apps".to_string()),
        packages_dir: config
            .layout
            .as_ref()
            .and_then(|l| l.packages_dir.clone())
            .unwrap_or_else(|| "packages".to_string()),
    })
}

/// Thu thập thư mục có manifest (package.json / backend manifest) — đệ quy.
fn collect_projects(root: PathBuf, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !root.exists() || !root.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join("package.json").exists() || has_backend_manifest(&path) {
            out.push(path);
            continue;
        }
        collect_projects(path, out)?;
    }
    Ok(())
}

/// Backend manifest detect — hợp nhất detect web.rs cũ (manage.py/main.py/pom.xml...)
/// + detect native mới (mg.toml/pyproject.toml/west.yml...) để không đổi hành vi.
pub fn has_backend_manifest(path: &Path) -> bool {
    [
        // native (mới)
        "mg.toml",
        "pyproject.toml",
        "west.yml",
        "platformio.ini",
        "project.godot",
        // web.rs cũ
        "go.mod",
        "Cargo.toml",
        "manage.py",
        "main.py",
        "src/main.py",
        "pom.xml",
        "artisan",
        "composer.json",
    ]
    .iter()
    .any(|name| path.join(name).exists())
}

/// Build workspace graph: node mỗi package có manifest, edge `workspace:` dep → node khác.
pub fn build_workspace_graph(targets: &[PathBuf]) -> anyhow::Result<WorkspaceGraph> {
    let mut nodes = Vec::new();
    let mut name_to_index: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for path in targets {
        if let Some(manifest) = read_package_manifest(path)? {
            let index = nodes.len();
            name_to_index.insert(manifest.name.clone(), index);
            nodes.push(WorkspaceNode {
                name: manifest.name.clone(),
                path: path.clone(),
                manifest,
            });
        }
    }

    let mut edges = Vec::new();
    for (from_index, node) in nodes.iter().enumerate() {
        for (dep_name, spec) in manifest_deps(&node.manifest) {
            let trimmed = spec.trim();
            if !trimmed.starts_with("workspace:") {
                continue;
            }
            let Some(&to_index) = name_to_index.get(&dep_name) else {
                return Err(anyhow::anyhow!(
                    "workspace dependency '{}' referenced by '{}' was not found in workspace targets",
                    dep_name,
                    node.path.display()
                ));
            };
            edges.push(WorkspaceEdge {
                from: from_index,
                to: to_index,
            });
        }
    }

    Ok(WorkspaceGraph { nodes, edges })
}

/// Tất cả dep entries (dependencies + dev + peer + optional) cho graph attention.
pub fn manifest_deps(manifest: &WorkspacePackageManifest) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (name, spec) in &manifest.dependencies {
        out.push((name.clone(), spec.clone()));
    }
    for (name, spec) in &manifest.dev_dependencies {
        out.push((name.clone(), spec.clone()));
    }
    for (name, spec) in &manifest.peer_dependencies {
        out.push((name.clone(), spec.clone()));
    }
    for (name, spec) in &manifest.optional_dependencies {
        out.push((name.clone(), spec.clone()));
    }
    out
}
