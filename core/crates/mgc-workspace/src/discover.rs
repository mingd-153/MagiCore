//! Workspace discovery — scan apps/ + packages/ (layout từ magicore.workspace.toml).

use crate::{
    WorkspaceEdge, WorkspaceGraph, WorkspaceNode, WorkspacePackageManifest, read_package_manifest,
};
use std::path::{Component, Path, PathBuf};

/// Layout dirs — mặc định apps/ + packages/, đọc từ magicore.workspace.toml [layout].
#[derive(Debug, Clone)]
pub struct DiscoverOptions {
    pub apps_dir: String,
    pub packages_dir: String,
}

/// Đọc cấu hình workspace layout từ magicore.workspace.toml.
pub fn discover_workspace_targets(project_root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let options = workspace_layout(project_root)?;
    let mut targets = Vec::new();
    let apps_dir = resolve_workspace_layout_dir(project_root, &options.apps_dir, "apps_dir")?;
    let packages_dir =
        resolve_workspace_layout_dir(project_root, &options.packages_dir, "packages_dir")?;
    collect_projects(apps_dir, &mut targets)?;
    collect_projects(packages_dir, &mut targets)?;
    targets.sort();
    targets.dedup();
    Ok(targets)
}

/// Read a workspace config only when it is a regular, non-symlink file.
/// Đọc cấu hình workspace chỉ khi đó là file thường, không phải symlink.
pub fn read_workspace_config(project_root: &Path) -> anyhow::Result<Option<String>> {
    let path = project_root.join("magicore.workspace.toml");
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        anyhow::bail!(
            "workspace config '{}' must not be a symlink",
            path.display()
        );
    }
    if !metadata.is_file() {
        anyhow::bail!(
            "workspace config '{}' must be a regular file",
            path.display()
        );
    }
    let contents =
        mgc_adapter_base::project_file::read_regular_text(&path, "magicore.workspace.toml")?;
    Ok(Some(contents))
}

/// Resolve a workspace layout entry as a project-local, non-symlink directory.
/// Chỉ phân giải layout tương đối trong project và từ chối mọi symlink.
pub fn resolve_workspace_layout_dir(
    project_root: &Path,
    configured_path: &str,
    field_name: &str,
) -> anyhow::Result<PathBuf> {
    let relative = Path::new(configured_path);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        anyhow::bail!(
            "workspace layout path '{field_name}' must be a non-empty relative path without parent or root components"
        );
    }

    let mut candidate = project_root.to_path_buf();
    for component in relative.components() {
        candidate.push(component.as_os_str());
        match std::fs::symlink_metadata(&candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                anyhow::bail!(
                    "workspace layout path '{field_name}' must not traverse symlink '{}'",
                    candidate.display()
                );
            }
            Ok(metadata) if !metadata.is_dir() => return Ok(candidate),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidate),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(candidate)
}

/// Check whether a path is a directory without following its final symlink.
/// Kiểm tra thư mục mà không đi theo symlink ở thành phần cuối.
pub fn workspace_entry_is_directory(path: &Path) -> anyhow::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            anyhow::bail!(
                "workspace directory '{}' must not be a symlink",
                path.display()
            );
        }
        Ok(metadata) => Ok(metadata.is_dir()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Đọc layout config; không có file → mặc định apps/ + packages/.
pub fn workspace_layout(project_root: &Path) -> anyhow::Result<DiscoverOptions> {
    let Some(contents) = read_workspace_config(project_root)? else {
        return Ok(DiscoverOptions {
            apps_dir: "apps".to_string(),
            packages_dir: "packages".to_string(),
        });
    };
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
    if !workspace_entry_is_directory(&root)? {
        return Ok(());
    }
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        if !workspace_entry_is_directory(&path)? {
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
/// + detect native mới (mgc.toml/pyproject.toml/west.yml...) để không đổi hành vi.
pub fn has_backend_manifest(path: &Path) -> bool {
    [
        // native (mới)
        "mgc.toml",
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
