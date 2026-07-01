//! MGPM Workspace Crate
//!
//! Discovers and manages monorepo workspace packages.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use mgpm_core::{MgpmConfig, WorkspaceConfig, SecurityConfig, LinkerMode};

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("workspace not found at {0}")]
    NotFound(PathBuf),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("glob error: {0}")]
    Glob(String),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("toml error: {0}")]
    Toml(#[from] toml::de::Error),
}

/// Parsed contents of a package.json file.
#[derive(Debug, Clone)]
pub struct ParsedPackageJson {
    pub name: String,
    pub version: Option<String>,
    pub dependencies: HashMap<String, String>,
    pub dev_dependencies: HashMap<String, String>,
    pub peer_dependencies: HashMap<String, String>,
    pub scripts: HashMap<String, String>,
}

/// A member package within a workspace.
#[derive(Debug, Clone)]
pub struct WorkspaceMember {
    pub name: String,
    pub path: PathBuf,
    pub package_json: ParsedPackageJson,
}

/// Filter selector for workspace member filtering.
#[derive(Debug, Clone)]
pub enum FilterSelector {
    /// Filter by exact package name.
    Name(String),
    /// Filter by glob pattern.
    Glob(String),
    /// Package and all its dependents (^name).
    Dependents(String),
    /// Package and all its dependencies (...name).
    Dependencies(String),
    /// Package + dependents + dependencies.
    All(String),
}

/// A workspace with its configuration and member packages.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
    config: WorkspaceConfig,
    members: Vec<WorkspaceMember>,
}

impl Workspace {
    /// Discovers a workspace at the given path.
    /// Checks for `mgpm.yaml` or `package.json` with a workspaces field.
    pub fn discover(path: &Path) -> Result<Self, WorkspaceError> {
        let mgpm_yaml = path.join("mgpm.yaml");
        if mgpm_yaml.exists() {
            let config: MgpmConfig = MgpmConfig::load(&mgpm_yaml)
                .map_err(|e| WorkspaceError::Parse(format!("config parse error: {e}")))?;
            if let Some(ws_config) = config.workspace {
                return Self::from_workspace_config(path, ws_config);
            }
        }

        let package_json = path.join("package.json");
        if package_json.exists() {
            let content = std::fs::read_to_string(&package_json)?;
            let value: serde_json::Value = serde_json::from_str(&content)?;
            if let Some(workspaces) = value.get("workspaces").and_then(|w| w.as_array()) {
                let patterns: Vec<String> = workspaces
                    .iter()
                    .filter_map(|w| w.as_str().map(String::from))
                    .collect();
                let ws_config = WorkspaceConfig {
                    packages: patterns,
                    catalog: None,
                    link_ws_packages: true,
                    scripts: HashMap::new(),
                    security: SecurityConfig::default(),
                    linker: LinkerMode::default(),
                };
                return Self::from_workspace_config(path, ws_config);
            }
        }

        Err(WorkspaceError::NotFound(path.to_path_buf()))
    }

    fn from_workspace_config(root: &Path, config: WorkspaceConfig) -> Result<Self, WorkspaceError> {
        let mut members = Vec::new();

        for pattern in &config.packages {
            let full_pattern = root.join(pattern);
            let pattern_str = full_pattern.to_str().ok_or_else(|| {
                WorkspaceError::Parse(format!("invalid path: {}", full_pattern.display()))
            })?;

            for entry in glob::glob(pattern_str).map_err(|e| WorkspaceError::Glob(e.to_string()))? {
                let path = entry.map_err(|e| WorkspaceError::Glob(e.to_string()))?;
                let package_json = path.join("package.json");
                if package_json.exists() {
                    let parsed = parse_package_json(&package_json)?;
                    let name = parsed.name.clone();
                    members.push(WorkspaceMember {
                        name,
                        path,
                        package_json: parsed,
                    });
                }
            }
        }

        Ok(Self {
            root: root.to_path_buf(),
            config,
            members,
        })
    }

    /// Returns the workspace root path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Finds a workspace member by name.
    pub fn find_member(&self, name: &str) -> Option<&WorkspaceMember> {
        self.members.iter().find(|m| m.name == name)
    }

    /// Returns all workspace members.
    pub fn members(&self) -> &[WorkspaceMember] {
        &self.members
    }

    /// Resolves a dependency to a workspace member path, if it exists.
    pub fn resolve_dependency(&self, name: &str) -> Option<PathBuf> {
        self.find_member(name).map(|m| m.path.clone())
    }

    /// Returns the workspace config.
    pub fn config(&self) -> &WorkspaceConfig {
        &self.config
    }

    /// Returns the number of workspace members.
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Returns members in topological order (dependencies before dependents).
    /// Uses Kahn's algorithm on the inter-workspace dependency graph.
    /// Returns an error if a cycle is detected.
    pub fn topological_sort(&self) -> Result<Vec<&WorkspaceMember>, WorkspaceError> {
        let n = self.members.len();
        if n == 0 {
            return Ok(Vec::new());
        }

        let indices: HashMap<&str, usize> = self
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| (m.name.as_str(), i))
            .collect();

        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut in_degree = vec![0usize; n];

        for (i, member) in self.members.iter().enumerate() {
            for dep_name in member
                .package_json
                .dependencies
                .keys()
                .chain(member.package_json.dev_dependencies.keys())
                .chain(member.package_json.peer_dependencies.keys())
            {
                if let Some(&j) = indices.get(dep_name.as_str()) {
                    adj[j].push(i);
                    in_degree[i] += 1;
                }
            }
        }

        let mut queue: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut result = Vec::with_capacity(n);

        while let Some(i) = queue.pop() {
            result.push(&self.members[i]);
            for &next in &adj[i] {
                in_degree[next] -= 1;
                if in_degree[next] == 0 {
                    queue.push(next);
                }
            }
        }

        if result.len() != n {
            return Err(WorkspaceError::Parse(
                "circular dependency detected in workspace members".to_string(),
            ));
        }

        Ok(result)
    }

    /// Returns members whose files have changed since the given git ref,
    /// plus any members that transitively depend on them.
    ///
    /// Runs `git diff --name-only <git_ref> HEAD` and cross-references
    /// changed file paths against workspace member directories.
    pub fn changed_since(&self, git_ref: &str) -> Result<Vec<&WorkspaceMember>, WorkspaceError> {
        let output = std::process::Command::new("git")
            .args([
                "-C",
                self.root.to_str().unwrap_or("."),
                "diff",
                "--name-only",
                git_ref,
                "HEAD",
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorkspaceError::Parse(format!("git diff failed: {stderr}")));
        }

        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| WorkspaceError::Parse(format!("invalid utf-8 from git: {e}")))?;
        let changed_files: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();

        if changed_files.is_empty() {
            return Ok(Vec::new());
        }

        let indices: HashMap<&str, usize> = self
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| (m.name.as_str(), i))
            .collect();

        let mut directly_changed: HashSet<usize> = HashSet::new();
        for (i, member) in self.members.iter().enumerate() {
            let rel = member.path.strip_prefix(&self.root).unwrap_or(&member.path);
            let prefix = format!("{}/", rel.display());
            if changed_files.iter().any(|f| f.starts_with(&prefix)) {
                directly_changed.insert(i);
            }
        }

        let mut rev: Vec<Vec<usize>> = vec![Vec::new(); self.members.len()];
        for (i, member) in self.members.iter().enumerate() {
            for dep_name in member
                .package_json
                .dependencies
                .keys()
                .chain(member.package_json.dev_dependencies.keys())
                .chain(member.package_json.peer_dependencies.keys())
            {
                if let Some(&j) = indices.get(dep_name.as_str()) {
                    rev[j].push(i);
                }
            }
        }

        let mut affected = directly_changed.clone();
        let mut queue: Vec<usize> = directly_changed.into_iter().collect();
        while let Some(i) = queue.pop() {
            for &dep in &rev[i] {
                if affected.insert(dep) {
                    queue.push(dep);
                }
            }
        }

        Ok(self
            .members
            .iter()
            .enumerate()
            .filter(|(i, _)| affected.contains(i))
            .map(|(_, m)| m)
            .collect())
    }

    /// Returns an adjacency list of inter-workspace dependencies.
    /// Maps each member name to the list of other workspace members it depends on.
    pub fn dependency_graph(&self) -> HashMap<String, Vec<String>> {
        let mut graph = HashMap::new();
        for member in &self.members {
            let deps = graph.entry(member.name.clone()).or_insert_with(Vec::new);
            for dep_name in member
                .package_json
                .dependencies
                .keys()
                .chain(member.package_json.dev_dependencies.keys())
                .chain(member.package_json.peer_dependencies.keys())
            {
                if self.find_member(dep_name).is_some() {
                    deps.push(dep_name.clone());
                }
            }
        }
        graph
    }

    /// Filters workspace members by the given selector.
    pub fn filter(&self, selector: &FilterSelector) -> Vec<&WorkspaceMember> {
        let graph = self.dependency_graph();
        let mut reverse_graph: HashMap<&str, Vec<&str>> = HashMap::new();
        for member in &self.members {
            reverse_graph.entry(member.name.as_str()).or_default();
        }
        for (pkg, deps) in &graph {
            for dep in deps {
                reverse_graph
                    .entry(dep.as_str())
                    .or_default()
                    .push(pkg.as_str());
            }
        }

        match selector {
            FilterSelector::Name(name) => self.find_member(name).into_iter().collect(),
            FilterSelector::Glob(pattern) => {
                let pat = glob::Pattern::new(pattern).ok();
                self.members
                    .iter()
                    .filter(|m| pat.as_ref().is_some_and(|p| p.matches(&m.name)))
                    .collect()
            }
            FilterSelector::Dependents(name) => {
                let target = match self.find_member(name) {
                    Some(m) => m,
                    None => return Vec::new(),
                };
                let mut visited = HashSet::new();
                let mut result = Vec::new();
                let mut stack = vec![target.name.as_str()];
                while let Some(current) = stack.pop() {
                    if !visited.insert(current) {
                        continue;
                    }
                    if let Some(member) = self.find_member(current) {
                        result.push(member);
                    }
                    if let Some(dependents) = reverse_graph.get(current) {
                        for &dep in dependents {
                            if !visited.contains(dep) {
                                stack.push(dep);
                            }
                        }
                    }
                }
                result
            }
            FilterSelector::Dependencies(name) => {
                let target = match self.find_member(name) {
                    Some(m) => m,
                    None => return Vec::new(),
                };
                let mut visited = HashSet::new();
                let mut result = Vec::new();
                let mut stack = vec![target.name.as_str()];
                while let Some(current) = stack.pop() {
                    if !visited.insert(current) {
                        continue;
                    }
                    if let Some(member) = self.find_member(current) {
                        result.push(member);
                    }
                    if let Some(deps) = graph.get(current) {
                        for dep in deps {
                            if !visited.contains(dep.as_str()) {
                                stack.push(dep.as_str());
                            }
                        }
                    }
                }
                result
            }
            FilterSelector::All(name) => {
                let target = match self.find_member(name) {
                    Some(m) => m,
                    None => return Vec::new(),
                };
                let mut visited = HashSet::new();
                let mut result = Vec::new();
                let mut stack = vec![target.name.as_str()];
                while let Some(current) = stack.pop() {
                    if !visited.insert(current) {
                        continue;
                    }
                    if let Some(member) = self.find_member(current) {
                        result.push(member);
                    }
                    if let Some(deps) = graph.get(current) {
                        for dep in deps {
                            if !visited.contains(dep.as_str()) {
                                stack.push(dep.as_str());
                            }
                        }
                    }
                    if let Some(dependents) = reverse_graph.get(current) {
                        for &dep in dependents {
                            if !visited.contains(dep) {
                                stack.push(dep);
                            }
                        }
                    }
                }
                result
            }
        }
    }
}

/// Parses a package.json file and extracts relevant fields.
pub fn parse_package_json(path: &Path) -> Result<ParsedPackageJson, WorkspaceError> {
    let content = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let name = value
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();

    let version = value
        .get("version")
        .and_then(|v| v.as_str())
        .map(String::from);

    let dependencies = parse_string_map(value.get("dependencies"));
    let dev_dependencies = parse_string_map(value.get("devDependencies"));
    let peer_dependencies = parse_string_map(value.get("peerDependencies"));
    let scripts = parse_string_map(value.get("scripts"));

    Ok(ParsedPackageJson {
        name,
        version,
        dependencies,
        dev_dependencies,
        peer_dependencies,
        scripts,
    })
}

fn parse_string_map(value: Option<&serde_json::Value>) -> HashMap<String, String> {
    value
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_discover_mgpm_yaml() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        let config = MgpmConfig {
            workspace: Some(WorkspaceConfig {
                packages: vec!["packages/*".to_string()],
                catalog: None,
                link_ws_packages: true,
                scripts: HashMap::new(),
                security: SecurityConfig::default(),
                linker: LinkerMode::default(),
            }),
            ..Default::default()
        };
        config.save(&root.join("mgpm.yaml")).unwrap();

        let pkg_dir = root.join("packages").join("foo");
        fs::create_dir_all(&pkg_dir).unwrap();
        let pkg_json = serde_json::json!({
            "name": "foo",
            "version": "1.0.0",
            "dependencies": { "bar": "^1.0.0" }
        });
        std::fs::write(
            pkg_dir.join("package.json"),
            serde_json::to_string(&pkg_json).unwrap(),
        )
        .unwrap();

        let ws = Workspace::discover(root).unwrap();
        assert_eq!(ws.member_count(), 1);
        assert_eq!(
            ws.find_member("foo")
                .unwrap()
                .package_json
                .version
                .as_deref(),
            Some("1.0.0")
        );
    }

    #[test]
    fn test_discover_package_json_workspaces() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        let pkg_json = serde_json::json!({
            "name": "root",
            "version": "0.0.0",
            "workspaces": ["packages/*"]
        });
        std::fs::write(
            root.join("package.json"),
            serde_json::to_string(&pkg_json).unwrap(),
        )
        .unwrap();

        let pkg_dir = root.join("packages").join("bar");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        let sub_json = serde_json::json!({
            "name": "bar",
            "version": "2.0.0",
            "dependencies": { "baz": "^3.0.0" }
        });
        std::fs::write(
            pkg_dir.join("package.json"),
            serde_json::to_string(&sub_json).unwrap(),
        )
        .unwrap();

        let ws = Workspace::discover(root).unwrap();
        assert_eq!(ws.member_count(), 1);
        assert_eq!(ws.members()[0].name, "bar");
    }

    #[test]
    fn test_resolve_dependency() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        let config = MgpmConfig {
            workspace: Some(WorkspaceConfig {
                packages: vec!["pkgs/*".to_string()],
                catalog: None,
                link_ws_packages: true,
                scripts: HashMap::new(),
                security: SecurityConfig::default(),
                linker: LinkerMode::default(),
            }),
            ..Default::default()
        };
        config.save(&root.join("mgpm.yaml")).unwrap();

        let pkg_dir = root.join("pkgs").join("alpha");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        let sub_json = serde_json::json!({
            "name": "alpha",
            "version": "0.1.0"
        });
        std::fs::write(
            pkg_dir.join("package.json"),
            serde_json::to_string(&sub_json).unwrap(),
        )
        .unwrap();

        let ws = Workspace::discover(root).unwrap();
        let resolved = ws.resolve_dependency("alpha");
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap(), pkg_dir);
        assert!(ws.resolve_dependency("nonexistent").is_none());
    }

    #[test]
    fn test_not_found() {
        let temp = tempdir().unwrap();
        let err = Workspace::discover(temp.path());
        assert!(err.is_err());
    }

    #[test]
    fn test_parse_package_json() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("package.json");
        let data = serde_json::json!({
            "name": "test-pkg",
            "version": "1.2.3",
            "dependencies": { "lodash": "^4.0.0" },
            "devDependencies": { "jest": "^29.0.0" },
            "peerDependencies": { "react": "^18.0.0" },
            "scripts": { "build": "tsc", "test": "jest" }
        });
        std::fs::write(&path, serde_json::to_string(&data).unwrap()).unwrap();

        let parsed = parse_package_json(&path).unwrap();
        assert_eq!(parsed.name, "test-pkg");
        assert_eq!(parsed.version, Some("1.2.3".to_string()));
        assert_eq!(parsed.dependencies.get("lodash").unwrap(), "^4.0.0");
        assert_eq!(parsed.dev_dependencies.get("jest").unwrap(), "^29.0.0");
        assert_eq!(parsed.peer_dependencies.get("react").unwrap(), "^18.0.0");
        assert_eq!(parsed.scripts.get("build").unwrap(), "tsc");
        assert_eq!(parsed.scripts.get("test").unwrap(), "jest");
    }
}
