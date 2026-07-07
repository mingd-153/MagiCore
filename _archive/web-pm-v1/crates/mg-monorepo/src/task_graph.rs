use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

use mg_core::ScriptConfig;
use mg_workspace::{PackageGraph, Workspace, WorkspaceMember};

use crate::error::TaskGraphError;

/// Unique identifier for a task node, composed of package + script name.
///
/// Display format: `package_name#script_name`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId {
    pub package_name: String,
    pub script_name: String,
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.package_name, self.script_name)
    }
}

impl TaskId {
    pub fn new(package_name: &str, script_name: &str) -> Self {
        Self {
            package_name: package_name.to_string(),
            script_name: script_name.to_string(),
        }
    }
}

/// A single task node in the graph, representing one script for one package.
#[derive(Debug, Clone)]
pub struct TaskNode {
    pub id: TaskId,
    pub package: String,
    pub script_command: String,
    pub config: ScriptConfig,
}

/// Directed acyclic graph of tasks with adjacency, reverse edges, and levels.
///
/// Built via [`TaskGraph::new`] or [`TaskGraph::new_multi`]. After construction,
/// callers can inspect the topological order, level groups, and dependency edges.
#[derive(Debug, Clone)]
pub struct TaskGraph {
    nodes: HashMap<TaskId, TaskNode>,
    adjacency: HashMap<TaskId, Vec<TaskId>>,
    reverse: HashMap<TaskId, Vec<TaskId>>,
    levels: Vec<Vec<TaskId>>,
    scripts_config: HashMap<String, ScriptConfig>,
}

impl TaskGraph {
    /// Build a task graph for a single script across filtered packages.
    pub fn new(
        workspace: &Workspace,
        package_graph: &PackageGraph,
        filtered_packages: &[&WorkspaceMember],
        script_name: &str,
        scripts_config: &HashMap<String, ScriptConfig>,
    ) -> Result<Self, TaskGraphError> {
        Self::build(
            workspace,
            package_graph,
            filtered_packages,
            &[script_name.to_string()],
            scripts_config,
        )
    }

    /// Build a task graph for multiple scripts across filtered packages.
    pub fn new_multi(
        workspace: &Workspace,
        package_graph: &PackageGraph,
        filtered_packages: &[&WorkspaceMember],
        script_names: &[String],
        scripts_config: &HashMap<String, ScriptConfig>,
    ) -> Result<Self, TaskGraphError> {
        Self::build(
            workspace,
            package_graph,
            filtered_packages,
            script_names,
            scripts_config,
        )
    }

    fn build(
        _workspace: &Workspace,
        package_graph: &PackageGraph,
        filtered_packages: &[&WorkspaceMember],
        script_names: &[String],
        scripts_config: &HashMap<String, ScriptConfig>,
    ) -> Result<Self, TaskGraphError> {
        for name in script_names {
            if !scripts_config.contains_key(name) {
                return Err(TaskGraphError::ScriptNotFound(name.clone()));
            }
        }

        let mut nodes: HashMap<TaskId, TaskNode> = HashMap::new();
        let mut adjacency: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
        let mut reverse: HashMap<TaskId, Vec<TaskId>> = HashMap::new();

        let filtered: HashSet<String> = filtered_packages.iter().map(|p| p.name.clone()).collect();

        for pkg in filtered_packages {
            for script_name in script_names {
                let id = TaskId::new(&pkg.name, script_name);
                let cfg = scripts_config
                    .get(script_name)
                    .ok_or_else(|| TaskGraphError::ScriptNotFound(script_name.clone()))?;
                let command = cfg.command.clone().unwrap_or_default();
                let node = TaskNode {
                    id: id.clone(),
                    package: pkg.name.clone(),
                    script_command: command,
                    config: cfg.clone(),
                };
                nodes.insert(id.clone(), node);
            }
        }

        let node_set: HashSet<&TaskId> = nodes.keys().collect();

        for pkg in filtered_packages {
            for script_name in script_names {
                let task_id = TaskId::new(&pkg.name, script_name);
                if !nodes.contains_key(&task_id) {
                    continue;
                }
                let cfg = scripts_config
                    .get(script_name)
                    .ok_or_else(|| TaskGraphError::ScriptNotFound(script_name.clone()))?;

                let mut deps: Vec<TaskId> = Vec::new();
                for dep_spec in &cfg.depends_on {
                    if let Some(rest) = dep_spec.strip_prefix('^') {
                        let dep_script = if rest.is_empty() { script_name } else { rest };
                        let pkg_deps = package_graph.dependencies(&pkg.name);
                        if let Ok(edges) = pkg_deps {
                            for edge in edges.iter().filter(|e| filtered.contains(&e.target)) {
                                let dep_id = TaskId::new(&edge.target, dep_script);
                                if node_set.contains(&dep_id) {
                                    deps.push(dep_id);
                                }
                            }
                        }
                    } else if let Some(pos) = dep_spec.find('#') {
                        let dep_pkg = &dep_spec[..pos];
                        let dep_script = &dep_spec[pos + 1..];
                        let dep_id = TaskId::new(dep_pkg, dep_script);
                        if !node_set.contains(&dep_id) {
                            return Err(TaskGraphError::MissingScript(
                                dep_pkg.to_string(),
                                dep_script.to_string(),
                            ));
                        }
                        deps.push(dep_id);
                    } else {
                        let dep_id = TaskId::new(&pkg.name, dep_spec);
                        if !node_set.contains(&dep_id) {
                            return Err(TaskGraphError::MissingScript(
                                pkg.name.clone(),
                                dep_spec.to_string(),
                            ));
                        }
                        deps.push(dep_id);
                    }
                }

                adjacency
                    .entry(task_id.clone())
                    .or_default()
                    .extend(deps.clone());
                for dep in deps {
                    reverse.entry(dep).or_default().push(task_id.clone());
                }
            }
        }

        for task_id in nodes.keys() {
            adjacency.entry(task_id.clone()).or_default();
            reverse.entry(task_id.clone()).or_default();
        }

        let topo = topological_sort(&nodes, &adjacency)?;
        let levels = compute_levels(&topo, &adjacency);

        Ok(Self {
            nodes,
            adjacency,
            reverse,
            levels,
            scripts_config: scripts_config.clone(),
        })
    }

    /// Return all task IDs in topological order (flat list).
    pub fn topological_order(&self) -> Vec<&TaskId> {
        self.levels.iter().flat_map(|level| level.iter()).collect()
    }

    /// Return task IDs grouped by topological level.
    ///
    /// Tasks in the same level have no dependency between them and can run in parallel.
    pub fn levels(&self) -> &[Vec<TaskId>] {
        &self.levels
    }

    /// Return the dependencies of a given task (tasks that must complete first).
    pub fn dependencies(&self, task: &TaskId) -> Result<&[TaskId], TaskGraphError> {
        self.adjacency
            .get(task)
            .map(|v| v.as_slice())
            .ok_or_else(|| TaskGraphError::Internal(format!("task '{}' not found in graph", task)))
    }

    /// Return the dependents of a given task (tasks waiting on this task).
    pub fn dependents(&self, task: &TaskId) -> Result<&[TaskId], TaskGraphError> {
        self.reverse
            .get(task)
            .map(|v| v.as_slice())
            .ok_or_else(|| TaskGraphError::Internal(format!("task '{}' not found in graph", task)))
    }

    /// Look up a task node by its ID.
    pub fn get_node(&self, task: &TaskId) -> Option<&TaskNode> {
        self.nodes.get(task)
    }

    /// Total number of tasks in the graph.
    pub fn task_count(&self) -> usize {
        self.nodes.len()
    }

    /// Check whether a task exists in the graph.
    pub fn has_task(&self, task: &TaskId) -> bool {
        self.nodes.contains_key(task)
    }

    /// Return the scripts configuration map used to build this graph.
    pub fn scripts_config(&self) -> &HashMap<String, ScriptConfig> {
        &self.scripts_config
    }
}

/// Kahn's algorithm for topological sort.
///
/// Returns an error if a cycle is detected.
fn topological_sort(
    nodes: &HashMap<TaskId, TaskNode>,
    adjacency: &HashMap<TaskId, Vec<TaskId>>,
) -> Result<Vec<TaskId>, TaskGraphError> {
    let mut in_degree: HashMap<&TaskId, usize> = HashMap::new();
    let mut reverse: HashMap<&TaskId, Vec<&TaskId>> = HashMap::new();

    for id in nodes.keys() {
        in_degree.entry(id).or_insert(0);
        reverse.entry(id).or_default();
    }

    for (id, deps) in adjacency {
        for dep in deps {
            if let Some(deg) = in_degree.get_mut(id) {
                *deg += 1;
            }
            reverse.entry(dep).or_default().push(id);
        }
    }

    let mut queue: VecDeque<&TaskId> = VecDeque::new();
    for (id, deg) in &in_degree {
        if *deg == 0 {
            queue.push_back(id);
        }
    }

    let mut sorted: Vec<TaskId> = Vec::with_capacity(nodes.len());
    while let Some(id) = queue.pop_front() {
        sorted.push((*id).clone());
        if let Some(dependents) = reverse.get(id) {
            for dep_id in dependents {
                if let Some(deg) = in_degree.get_mut(dep_id) {
                    *deg = deg.wrapping_sub(1);
                    if *deg == 0 {
                        queue.push_back(dep_id);
                    }
                }
            }
        }
    }

    if sorted.len() != nodes.len() {
        let visited: HashSet<&TaskId> = sorted.iter().collect();
        let cycle_nodes: Vec<String> = nodes
            .keys()
            .filter(|id| !visited.contains(id))
            .map(|id| id.to_string())
            .collect();
        return Err(TaskGraphError::CircularDependency(cycle_nodes.join(", ")));
    }

    Ok(sorted)
}

/// Group a topologically-sorted task list into levels.
///
/// Level 0 = no dependencies. Level N = depends on tasks in level N-1.
fn compute_levels(topo: &[TaskId], adjacency: &HashMap<TaskId, Vec<TaskId>>) -> Vec<Vec<TaskId>> {
    let mut depth: HashMap<&TaskId, usize> = HashMap::new();
    let mut max_depth: usize = 0;

    for id in topo {
        let d = adjacency
            .get(id)
            .map(|deps| {
                deps.iter()
                    .filter_map(|dep| depth.get(dep))
                    .max()
                    .map(|d| d + 1)
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        depth.insert(id, d);
        if d > max_depth {
            max_depth = d;
        }
    }

    let mut levels: Vec<Vec<TaskId>> = vec![Vec::new(); max_depth + 1];
    for id in topo {
        if let Some(&d) = depth.get(id) {
            levels[d].push(id.clone());
        }
    }

    levels
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use mg_core::ScriptConfig;

    fn make_workspace_member_with_deps(name: &str, deps: &[(&str, &str)]) -> WorkspaceMember {
        let mut dep_map = std::collections::HashMap::new();
        for (dep_name, version) in deps {
            dep_map.insert(dep_name.to_string(), version.to_string());
        }
        WorkspaceMember {
            name: name.to_string(),
            path: std::path::PathBuf::from(name),
            package_json: mg_workspace::ParsedPackageJson {
                name: name.to_string(),
                version: Some("1.0.0".to_string()),
                dependencies: dep_map,
                dev_dependencies: std::collections::HashMap::new(),
                peer_dependencies: std::collections::HashMap::new(),
                scripts: std::collections::HashMap::new(),
            },
        }
    }

    fn make_workspace_member(name: &str) -> WorkspaceMember {
        make_workspace_member_with_deps(name, &[])
    }

    fn make_script_config(depends_on: Vec<&str>) -> ScriptConfig {
        ScriptConfig {
            command: Some("echo hello".to_string()),
            depends_on: depends_on.into_iter().map(String::from).collect(),
            cache: true,
            inputs: Vec::new(),
            outputs: Vec::new(),
            persistent: false,
        }
    }

    pub fn build_chain_graph(
        package_names: &[&str],
    ) -> (Workspace, PackageGraph, Vec<WorkspaceMember>) {
        let members: Vec<WorkspaceMember> = package_names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                if i + 1 < package_names.len() {
                    make_workspace_member_with_deps(n, &[(package_names[i + 1], "workspace:*")])
                } else {
                    make_workspace_member(n)
                }
            })
            .collect();

        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let config = mg_core::WorkspaceConfig::default();

        let ws = Workspace::new(root, config, members.clone());
        let pg = PackageGraph::from_workspace(&ws);

        (ws, pg, members)
    }

    pub fn build_caret_graph() -> (Workspace, PackageGraph, Vec<WorkspaceMember>) {
        build_chain_graph(&["apps/web", "packages/ui", "packages/shared"])
    }

    pub fn make_scripts_config(script: &str, deps: Vec<&str>) -> HashMap<String, ScriptConfig> {
        let mut map = HashMap::new();
        map.insert(script.to_string(), make_script_config(deps));
        map
    }

    #[test]
    fn test_basic_graph() {
        let (_ws, pg, members) = build_caret_graph();
        let scripts = make_scripts_config("build", vec![]);
        let graph = TaskGraph::new(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            "build",
            &scripts,
        )
        .unwrap();

        assert_eq!(graph.task_count(), 3);
        assert!(graph.has_task(&TaskId::new("apps/web", "build")));
        assert!(graph.has_task(&TaskId::new("packages/ui", "build")));
        assert!(graph.has_task(&TaskId::new("packages/shared", "build")));
    }

    #[test]
    fn test_caret_build_deps() {
        let (_ws, pg, members) = build_caret_graph();
        let scripts = make_scripts_config("build", vec!["^build"]);
        let graph = TaskGraph::new(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            "build",
            &scripts,
        )
        .unwrap();

        let web = TaskId::new("apps/web", "build");
        let ui = TaskId::new("packages/ui", "build");
        let shared = TaskId::new("packages/shared", "build");

        let web_deps = graph.dependencies(&web).unwrap();
        assert!(
            web_deps.contains(&ui),
            "web build should depend on ui build"
        );

        let ui_deps = graph.dependencies(&ui).unwrap();
        assert!(
            ui_deps.contains(&shared),
            "ui build should depend on shared build"
        );

        let shared_deps = graph.dependencies(&shared).unwrap();
        assert!(shared_deps.is_empty(), "shared build should have no deps");
    }

    #[test]
    fn test_self_dep() {
        let (_ws, pg, members) = build_caret_graph();
        let scripts = make_scripts_config("test", vec!["build"]);
        let build_scripts = make_scripts_config("build", vec![]);
        let mut all_scripts = scripts.clone();
        all_scripts.extend(build_scripts);

        let graph = TaskGraph::new_multi(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            &["test".to_string(), "build".to_string()],
            &all_scripts,
        )
        .unwrap();

        let test_task = TaskId::new("apps/web", "test");
        let build_task = TaskId::new("apps/web", "build");
        let deps = graph.dependencies(&test_task).unwrap();
        assert!(
            deps.contains(&build_task),
            "test should depend on own build"
        );
    }

    #[test]
    fn test_specific_pkg_dep() {
        let (_ws, pg, members) = build_caret_graph();
        let scripts = make_scripts_config("lint", vec!["packages/shared#build"]);
        let build_scripts = make_scripts_config("build", vec![]);
        let mut all_scripts = scripts.clone();
        all_scripts.extend(build_scripts);

        let graph = TaskGraph::new_multi(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            &["lint".to_string(), "build".to_string()],
            &all_scripts,
        )
        .unwrap();

        let lint = TaskId::new("apps/web", "lint");
        let shared_build = TaskId::new("packages/shared", "build");
        let deps = graph.dependencies(&lint).unwrap();
        assert!(
            deps.contains(&shared_build),
            "lint should depend on packages/shared#build"
        );
    }

    #[test]
    fn test_topological_order() {
        let (_ws, pg, members) = build_caret_graph();
        let scripts = make_scripts_config("build", vec!["^build"]);
        let graph = TaskGraph::new(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            "build",
            &scripts,
        )
        .unwrap();

        let order: Vec<String> = graph
            .topological_order()
            .iter()
            .map(|t| t.to_string())
            .collect();

        let pos_shared = order
            .iter()
            .position(|t| t == "packages/shared#build")
            .unwrap();
        let pos_ui = order.iter().position(|t| t == "packages/ui#build").unwrap();
        let pos_web = order.iter().position(|t| t == "apps/web#build").unwrap();

        assert!(pos_shared < pos_ui, "shared must come before ui");
        assert!(pos_ui < pos_web, "ui must come before web");
    }

    #[test]
    fn test_levels() {
        let (_ws, pg, members) = build_caret_graph();
        let scripts = make_scripts_config("build", vec!["^build"]);
        let graph = TaskGraph::new(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            "build",
            &scripts,
        )
        .unwrap();

        let levels = graph.levels();
        assert!(!levels.is_empty());

        let shared = TaskId::new("packages/shared", "build");
        assert!(
            levels[0].contains(&shared),
            "shared#build should be at level 0"
        );

        let ui = TaskId::new("packages/ui", "build");
        assert!(levels[1].contains(&ui), "ui#build should be at level 1");

        let web = TaskId::new("apps/web", "build");
        assert!(levels[2].contains(&web), "web#build should be at level 2");
    }

    #[test]
    fn test_cycle_detection() {
        let (_ws, pg, members) = build_chain_graph(&["a", "b", "c"]);
        let scripts_a = make_scripts_config("build", vec!["b#build"]);
        let scripts_b = make_scripts_config("build", vec!["c#build"]);
        let scripts_c = make_scripts_config("build", vec!["a#build"]);
        let mut all_scripts = HashMap::new();
        all_scripts.extend(scripts_a);
        all_scripts.extend(scripts_b);
        all_scripts.extend(scripts_c);

        let result = TaskGraph::new_multi(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            &["build".to_string()],
            &all_scripts,
        );
        assert!(result.is_err());
        match result {
            Err(TaskGraphError::CircularDependency(_)) => {}
            _ => panic!("expected CircularDependency error"),
        }
    }

    #[test]
    fn test_script_not_found() {
        let (_ws, pg, members) = build_caret_graph();
        let scripts = HashMap::new();
        let result = TaskGraph::new(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            "nonexistent",
            &scripts,
        );
        match result {
            Err(TaskGraphError::ScriptNotFound(_)) => {}
            _ => panic!("expected ScriptNotFound error"),
        }
    }

    #[test]
    fn test_missing_pkg_script_dep() {
        let (_ws, pg, members) = build_caret_graph();
        let scripts = make_scripts_config("build", vec!["nonexistent_pkg#build"]);
        let result = TaskGraph::new(
            &_ws,
            &pg,
            &members.iter().collect::<Vec<_>>(),
            "build",
            &scripts,
        );
        match result {
            Err(TaskGraphError::MissingScript(_, _)) => {}
            _ => panic!("expected MissingScript error"),
        }
    }

    #[test]
    fn test_empty_filtered() {
        let (_ws, pg, _members) = build_caret_graph();
        let scripts = make_scripts_config("build", vec![]);
        let graph = TaskGraph::new(&_ws, &pg, &[], "build", &scripts).unwrap();
        assert_eq!(graph.task_count(), 0);
        assert!(graph.topological_order().is_empty());
    }
}
