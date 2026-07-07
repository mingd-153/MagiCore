use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use crate::{Workspace, WorkspaceMember};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    White,
    Gray,
    Black,
}

/// Package dependency graph for a workspace.
///
/// Builds dependency relationships from workspace member `package.json` files,
/// supporting topological sort, cycle detection, and transitive queries.
#[derive(Debug, Clone)]
pub struct PackageGraph {
    /// Workspace root path
    root: PathBuf,

    /// All workspace members (keyed by name)
    members: HashMap<String, WorkspaceMember>,

    /// Forward edges: package → packages it depends on
    adjacency: HashMap<String, Vec<DependencyEdge>>,

    /// Reverse edges: package → packages that depend on it
    reverse: HashMap<String, Vec<DependencyEdge>>,
}

/// A dependency edge between two packages.
#[derive(Debug, Clone)]
pub struct DependencyEdge {
    /// Target package name
    pub target: String,
    /// Dependency kind
    pub kind: DepKind,
    /// Original version specifier (e.g., "workspace:*", "^1.0.0")
    pub specifier: String,
}

/// Kind of dependency relationship.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepKind {
    /// Internal workspace dependency
    Internal,
    /// External dependency (npm registry, etc.)
    External,
}

/// Errors that can occur during package graph operations.
#[derive(Debug, thiserror::Error)]
pub enum PackageGraphError {
    #[error("package '{0}' not found in workspace")]
    PackageNotFound(String),

    #[error("circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl PackageGraph {
    fn build_edges(
        deps: &HashMap<String, String>,
        members: &HashMap<String, WorkspaceMember>,
        seen: &mut HashSet<String>,
    ) -> Vec<DependencyEdge> {
        let mut edges = Vec::new();
        for (dep_name, specifier) in deps {
            if !seen.insert(dep_name.clone()) {
                continue;
            }
            let kind = if members.contains_key(dep_name.as_str()) {
                DepKind::Internal
            } else {
                DepKind::External
            };
            edges.push(DependencyEdge {
                target: dep_name.clone(),
                kind,
                specifier: specifier.clone(),
            });
        }
        edges
    }

    /// Build a package graph from a workspace.
    pub fn from_workspace(ws: &Workspace) -> Self {
        let members: HashMap<String, WorkspaceMember> = ws
            .members()
            .iter()
            .map(|m| (m.name.clone(), m.clone()))
            .collect();

        let mut adjacency: HashMap<String, Vec<DependencyEdge>> = HashMap::new();
        let mut reverse: HashMap<String, Vec<DependencyEdge>> = HashMap::new();

        for member in ws.members() {
            let mut seen = HashSet::new();
            let mut edges = Vec::new();

            edges.extend(Self::build_edges(
                &member.package_json.dependencies,
                &members,
                &mut seen,
            ));
            edges.extend(Self::build_edges(
                &member.package_json.dev_dependencies,
                &members,
                &mut seen,
            ));
            edges.extend(Self::build_edges(
                &member.package_json.peer_dependencies,
                &members,
                &mut seen,
            ));

            adjacency.insert(member.name.clone(), edges.clone());

            for edge in &edges {
                reverse
                    .entry(edge.target.clone())
                    .or_default()
                    .push(DependencyEdge {
                        target: member.name.clone(),
                        kind: edge.kind.clone(),
                        specifier: edge.specifier.clone(),
                    });
            }
        }

        for member in ws.members() {
            reverse.entry(member.name.clone()).or_default();
        }

        Self {
            root: ws.root().to_path_buf(),
            members,
            adjacency,
            reverse,
        }
    }

    /// Get all workspace package names.
    pub fn packages(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.members.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    /// Get direct dependencies of a package.
    pub fn dependencies(&self, package: &str) -> Result<&[DependencyEdge], PackageGraphError> {
        self.adjacency
            .get(package)
            .map(|v| v.as_slice())
            .ok_or_else(|| PackageGraphError::PackageNotFound(package.to_string()))
    }

    /// Get direct dependents of a package.
    pub fn dependents(&self, package: &str) -> Result<&[DependencyEdge], PackageGraphError> {
        self.reverse
            .get(package)
            .map(|v| v.as_slice())
            .ok_or_else(|| PackageGraphError::PackageNotFound(package.to_string()))
    }

    /// Get all transitive dependencies of a package (BFS).
    pub fn transitive_dependencies(&self, package: &str) -> Result<Vec<&str>, PackageGraphError> {
        if !self.members.contains_key(package) {
            return Err(PackageGraphError::PackageNotFound(package.to_string()));
        }

        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(package);

        while let Some(current) = queue.pop_front() {
            if let Some(edges) = self.adjacency.get(current) {
                for edge in edges {
                    if edge.kind == DepKind::Internal && visited.insert(edge.target.as_str()) {
                        result.push(edge.target.as_str());
                        queue.push_back(edge.target.as_str());
                    }
                }
            }
        }

        Ok(result)
    }

    /// Get all transitive dependents of a package (BFS).
    pub fn transitive_dependents(&self, package: &str) -> Result<Vec<&str>, PackageGraphError> {
        if !self.members.contains_key(package) {
            return Err(PackageGraphError::PackageNotFound(package.to_string()));
        }

        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(package);

        while let Some(current) = queue.pop_front() {
            if let Some(edges) = self.reverse.get(current) {
                for edge in edges {
                    if visited.insert(edge.target.as_str()) {
                        result.push(edge.target.as_str());
                        queue.push_back(edge.target.as_str());
                    }
                }
            }
        }

        Ok(result)
    }

    /// Topological sort using Kahn's algorithm.
    /// Returns package names in dependency order (dependencies before dependents).
    pub fn topological_sort(&self) -> Result<Vec<&str>, PackageGraphError> {
        let n = self.members.len();
        if n == 0 {
            return Ok(Vec::new());
        }

        let names: Vec<&str> = self.members.keys().map(|s| s.as_str()).collect();
        let indices: HashMap<&str, usize> = names
            .iter()
            .enumerate()
            .map(|(i, name)| (*name, i))
            .collect();

        let mut in_degree = vec![0usize; n];
        for (i, name) in names.iter().enumerate() {
            if let Some(edges) = self.adjacency.get(*name) {
                for edge in edges {
                    if edge.kind == DepKind::Internal {
                        if let Some(&_j) = indices.get(edge.target.as_str()) {
                            in_degree[i] += 1;
                        }
                    }
                }
            }
        }

        let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();

        let mut result = Vec::with_capacity(n);
        while let Some(i) = queue.pop_front() {
            result.push(names[i]);
            if let Some(edges) = self.reverse.get(names[i]) {
                for edge in edges {
                    if edge.kind == DepKind::Internal {
                        if let Some(&j) = indices.get(edge.target.as_str()) {
                            in_degree[j] = in_degree[j].saturating_sub(1);
                            if in_degree[j] == 0 {
                                queue.push_back(j);
                            }
                        }
                    }
                }
            }
        }

        if result.len() != n {
            let remaining: Vec<&str> = names
                .iter()
                .copied()
                .enumerate()
                .filter(|&(i, _)| in_degree[i] > 0)
                .map(|(_, name)| name)
                .collect();
            return Err(PackageGraphError::CircularDependency(format!(
                "packages involved in cycle(s): {}",
                remaining.join(", ")
            )));
        }

        Ok(result)
    }

    /// Detect all cycles in the graph using DFS white/gray/black coloring.
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        let names: Vec<&str> = self.members.keys().map(|s| s.as_str()).collect();

        let mut color: HashMap<&str, Color> = names.iter().map(|n| (*n, Color::White)).collect();
        let mut cycles = Vec::new();
        let mut path_stack: Vec<&str> = Vec::new();

        for name in &names {
            if color[name] == Color::White {
                Self::dfs_cycle(
                    name,
                    &self.adjacency,
                    &mut color,
                    &mut path_stack,
                    &mut cycles,
                );
            }
        }

        cycles
    }

    fn dfs_cycle<'a>(
        node: &'a str,
        adjacency: &'a HashMap<String, Vec<DependencyEdge>>,
        color: &mut HashMap<&'a str, Color>,
        path_stack: &mut Vec<&'a str>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        color.insert(node, Color::Gray);
        path_stack.push(node);

        if let Some(edges) = adjacency.get(node) {
            for edge in edges {
                if edge.kind != DepKind::Internal {
                    continue;
                }
                let target = edge.target.as_str();
                match color.get(target) {
                    Some(Color::Gray) => {
                        // Found a cycle — extract from path_stack
                        let cycle_start = path_stack.iter().position(|n| *n == target);
                        if let Some(start) = cycle_start {
                            let mut cycle: Vec<String> = path_stack[start..]
                                .iter()
                                .map(|s| (*s).to_string())
                                .collect();
                            cycle.push(target.to_string());
                            cycles.push(cycle);
                        }
                    }
                    Some(Color::White) => {
                        Self::dfs_cycle(target, adjacency, color, path_stack, cycles);
                    }
                    _ => {}
                }
            }
        }

        path_stack.pop();
        color.insert(node, Color::Black);
    }

    /// Extract subgraph containing only the specified packages and their edges.
    pub fn subgraph(&self, packages: &HashSet<String>) -> Self {
        let mut new_members = HashMap::new();
        let mut new_adjacency = HashMap::new();
        let mut new_reverse = HashMap::new();

        for pkg in packages {
            if let Some(member) = self.members.get(pkg) {
                new_members.insert(pkg.clone(), member.clone());
            }

            if let Some(edges) = self.adjacency.get(pkg) {
                let filtered: Vec<DependencyEdge> = edges
                    .iter()
                    .filter(|e| packages.contains(&e.target))
                    .cloned()
                    .collect();
                new_adjacency.insert(pkg.clone(), filtered);
            }

            if let Some(edges) = self.reverse.get(pkg) {
                let filtered: Vec<DependencyEdge> = edges
                    .iter()
                    .filter(|e| packages.contains(&e.target))
                    .cloned()
                    .collect();
                new_reverse.insert(pkg.clone(), filtered);
            }
        }

        for pkg in packages {
            new_reverse.entry(pkg.clone()).or_default();
            new_adjacency.entry(pkg.clone()).or_default();
        }

        Self {
            root: self.root.clone(),
            members: new_members,
            adjacency: new_adjacency,
            reverse: new_reverse,
        }
    }

    /// Returns true if `package` transitively depends on `target`.
    pub fn depends_on(&self, package: &str, target: &str) -> bool {
        if !self.members.contains_key(package) || !self.members.contains_key(target) {
            return false;
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(package);

        while let Some(current) = queue.pop_front() {
            if let Some(edges) = self.adjacency.get(current) {
                for edge in edges {
                    if edge.kind != DepKind::Internal {
                        continue;
                    }
                    if edge.target == target {
                        return true;
                    }
                    if visited.insert(edge.target.as_str()) {
                        queue.push_back(edge.target.as_str());
                    }
                }
            }
        }

        false
    }

    /// Get the dependency depth of a package
    /// (0 = no internal workspace dependencies).
    pub fn depth(&self, package: &str) -> usize {
        if !self.members.contains_key(package) {
            return 0;
        }

        let mut visited = HashSet::new();
        self.depth_with_visited(package, &mut visited)
    }

    fn depth_with_visited(&self, package: &str, visited: &mut HashSet<String>) -> usize {
        if !visited.insert(package.to_string()) {
            return 0;
        }

        let deps = match self.transitive_dependencies(package) {
            Ok(d) => d,
            Err(_) => return 0,
        };

        if deps.is_empty() {
            return 0;
        }

        let mut max_depth = 0usize;
        for dep in deps {
            let sub_depth = self.depth_with_visited(dep, visited);
            max_depth = max_depth.max(sub_depth + 1);
        }

        max_depth
    }

    /// Partition packages into levels where same level
    /// means no dependency between them (parallelizable).
    pub fn levels(&self) -> Vec<Vec<&str>> {
        if self.members.is_empty() {
            return Vec::new();
        }

        let names: Vec<&str> = self.members.keys().map(|s| s.as_str()).collect();
        let indices: HashMap<&str, usize> = names
            .iter()
            .enumerate()
            .map(|(i, name)| (*name, i))
            .collect();

        let n = names.len();
        let mut in_degree = vec![0usize; n];
        for (i, name) in names.iter().enumerate() {
            if let Some(edges) = self.adjacency.get(*name) {
                for edge in edges {
                    if edge.kind == DepKind::Internal && indices.contains_key(edge.target.as_str())
                    {
                        in_degree[i] += 1;
                    }
                }
            }
        }

        let mut result = Vec::new();
        let mut processed = vec![false; n];

        loop {
            let mut level: Vec<&str> = names
                .iter()
                .copied()
                .enumerate()
                .filter(|&(i, _)| !processed[i] && in_degree[i] == 0)
                .map(|(_, name)| name)
                .collect();

            if level.is_empty() {
                let remaining: Vec<&str> = names
                    .iter()
                    .copied()
                    .enumerate()
                    .filter(|&(i, _)| !processed[i])
                    .map(|(_, name)| name)
                    .collect();
                if !remaining.is_empty() {
                    result.push(remaining);
                }
                break;
            }

            level.sort();
            result.push(level.clone());

            for name in &level {
                let i = indices[name];
                processed[i] = true;

                if let Some(edges) = self.reverse.get(*name) {
                    for edge in edges {
                        if let Some(&j) = indices.get(edge.target.as_str()) {
                            in_degree[j] = in_degree[j].saturating_sub(1);
                        }
                    }
                }
            }
        }

        result
    }

    /// Number of nodes (workspace members) in the graph.
    pub fn node_count(&self) -> usize {
        self.members.len()
    }

    /// Number of edges (internal dependencies only) in the graph.
    pub fn edge_count(&self) -> usize {
        self.adjacency
            .values()
            .flat_map(|edges| edges.iter())
            .filter(|e| e.kind == DepKind::Internal)
            .count()
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::{LinkerMode, ParsedPackageJson, SecurityConfig, WorkspaceConfig};

    fn make_member(
        name: &str,
        deps: Vec<(&str, &str)>,
        dev_deps: Vec<(&str, &str)>,
        peer_deps: Vec<(&str, &str)>,
    ) -> WorkspaceMember {
        let mut dependencies = HashMap::new();
        for (k, v) in deps {
            dependencies.insert(k.to_string(), v.to_string());
        }
        let mut dev_dependencies = HashMap::new();
        for (k, v) in dev_deps {
            dev_dependencies.insert(k.to_string(), v.to_string());
        }
        let mut peer_dependencies = HashMap::new();
        for (k, v) in peer_deps {
            peer_dependencies.insert(k.to_string(), v.to_string());
        }

        WorkspaceMember {
            name: name.to_string(),
            path: PathBuf::from(format!("packages/{name}")),
            package_json: ParsedPackageJson {
                name: name.to_string(),
                version: Some("1.0.0".to_string()),
                dependencies,
                dev_dependencies,
                peer_dependencies,
                scripts: HashMap::new(),
            },
        }
    }

    fn make_workspace(members: Vec<WorkspaceMember>) -> Workspace {
        Workspace::new(
            PathBuf::from("/test/root"),
            WorkspaceConfig {
                packages: vec!["packages/*".to_string()],
                catalog: None,
                link_ws_packages: true,
                catalogs: HashMap::new(),
                shared_lockfile: true,
                hoist: false,
                scripts: HashMap::new(),
                security: SecurityConfig::default(),
                linker: LinkerMode::default(),
            },
            members,
        )
    }

    /// Sample graph: A → B → C
    fn sample_graph() -> PackageGraph {
        let members = vec![
            make_member("a", vec![("b", "workspace:*")], vec![], vec![]),
            make_member("b", vec![("c", "workspace:*")], vec![], vec![]),
            make_member("c", vec![], vec![], vec![]),
        ];
        let ws = make_workspace(members);
        PackageGraph::from_workspace(&ws)
    }

    /// Cyclic graph: A → B → C → A
    fn cyclic_graph() -> PackageGraph {
        let members = vec![
            make_member("a", vec![("b", "workspace:*")], vec![], vec![]),
            make_member("b", vec![("c", "workspace:*")], vec![], vec![]),
            make_member("c", vec![("a", "workspace:*")], vec![], vec![]),
        ];
        let ws = make_workspace(members);
        PackageGraph::from_workspace(&ws)
    }

    /// Mixed graph: A depends on B (internal) and lodash (external)
    fn mixed_graph() -> PackageGraph {
        let members = vec![
            make_member(
                "a",
                vec![("b", "workspace:*"), ("lodash", "^4.0.0")],
                vec![],
                vec![],
            ),
            make_member("b", vec![], vec![], vec![]),
        ];
        let ws = make_workspace(members);
        PackageGraph::from_workspace(&ws)
    }

    #[test]
    fn test_basic_graph() {
        let graph = sample_graph();

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.packages(), vec!["a", "b", "c"]);

        let deps_a = graph.dependencies("a").unwrap();
        assert_eq!(deps_a.len(), 1);
        assert_eq!(deps_a[0].target, "b");
        assert_eq!(deps_a[0].kind, DepKind::Internal);

        let deps_c = graph.dependencies("c").unwrap();
        assert_eq!(deps_c.len(), 0);
    }

    #[test]
    fn test_topological_sort() {
        let graph = sample_graph();
        let sorted = graph.topological_sort().unwrap();
        assert_eq!(sorted, vec!["c", "b", "a"]);
    }

    #[test]
    fn test_cycle_detection() {
        let graph = cyclic_graph();
        let cycles = graph.detect_cycles();
        assert!(!cycles.is_empty());

        let has_cycle = cycles
            .iter()
            .any(|c| c.len() == 4 && c.contains(&"a".to_string()));
        assert!(has_cycle);
    }

    #[test]
    fn test_topological_sort_with_cycle() {
        let graph = cyclic_graph();
        let result = graph.topological_sort();
        assert!(result.is_err());
        match result {
            Err(PackageGraphError::CircularDependency(msg)) => {
                assert!(msg.contains("a") || msg.contains("b") || msg.contains("c"));
            }
            _ => panic!("expected CircularDependency error"),
        }
    }

    #[test]
    fn test_transitive_dependencies() {
        let graph = sample_graph();
        let deps_of_a = graph.transitive_dependencies("a").unwrap();
        assert!(deps_of_a.contains(&"b"));
        assert!(deps_of_a.contains(&"c"));
        assert_eq!(deps_of_a.len(), 2);
    }

    #[test]
    fn test_transitive_dependents() {
        let graph = sample_graph();
        let deps_of_c = graph.transitive_dependents("c").unwrap();
        assert!(deps_of_c.contains(&"a"));
        assert!(deps_of_c.contains(&"b"));
        assert_eq!(deps_of_c.len(), 2);
    }

    #[test]
    fn test_depends_on() {
        let graph = sample_graph();
        assert!(graph.depends_on("a", "b"));
        assert!(graph.depends_on("a", "c"));
        assert!(!graph.depends_on("b", "a"));
        assert!(!graph.depends_on("c", "a"));
    }

    #[test]
    fn test_subgraph() {
        let graph = sample_graph();
        let packages: HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let sub = graph.subgraph(&packages);

        assert_eq!(sub.node_count(), 2);
        assert_eq!(sub.packages(), vec!["a", "b"]);

        let sorted = sub.topological_sort().unwrap();
        assert_eq!(sorted, vec!["b", "a"]);
    }

    #[test]
    fn test_levels() {
        let graph = sample_graph();
        let lvls = graph.levels();
        assert_eq!(lvls.len(), 3);
        assert_eq!(lvls[0], vec!["c"]);
        assert_eq!(lvls[1], vec!["b"]);
        assert_eq!(lvls[2], vec!["a"]);
    }

    #[test]
    fn test_depth() {
        let graph = sample_graph();
        assert_eq!(graph.depth("c"), 0);
        assert_eq!(graph.depth("b"), 1);
        assert_eq!(graph.depth("a"), 2);
    }

    #[test]
    fn test_depth_with_cycle() {
        let graph = cyclic_graph();
        let d = graph.depth("a");
        // Should not stack overflow; value depends on traversal order
        assert!(d >= 1);
    }

    #[test]
    fn test_package_not_found() {
        let graph = sample_graph();
        let err = graph.dependencies("nonexistent");
        assert!(err.is_err());
        match err {
            Err(PackageGraphError::PackageNotFound(name)) => {
                assert_eq!(name, "nonexistent");
            }
            _ => panic!("expected PackageNotFound error"),
        }
    }

    #[test]
    fn test_empty_workspace() {
        let members = vec![];
        let ws = make_workspace(members);
        let graph = PackageGraph::from_workspace(&ws);

        assert_eq!(graph.node_count(), 0);
        assert!(graph.packages().is_empty());
        assert!(graph.topological_sort().unwrap().is_empty());
        assert!(graph.levels().is_empty());
    }

    #[test]
    fn test_mixed_internal_external() {
        let graph = mixed_graph();

        assert_eq!(graph.node_count(), 2);

        let deps_a = graph.dependencies("a").unwrap();
        assert_eq!(deps_a.len(), 2);

        let internal: Vec<&DependencyEdge> = deps_a
            .iter()
            .filter(|e| e.kind == DepKind::Internal)
            .collect();
        let external: Vec<&DependencyEdge> = deps_a
            .iter()
            .filter(|e| e.kind == DepKind::External)
            .collect();
        assert_eq!(internal.len(), 1);
        assert_eq!(external.len(), 1);
        assert_eq!(external[0].target, "lodash");
    }

    #[test]
    fn test_self_dependency() {
        let members = vec![make_member("a", vec![("a", "workspace:*")], vec![], vec![])];
        let ws = make_workspace(members);
        let graph = PackageGraph::from_workspace(&ws);

        let cycles = graph.detect_cycles();
        assert!(!cycles.is_empty());

        let has_self_cycle = cycles
            .iter()
            .any(|c| c.len() == 2 && c[0] == "a" && c[1] == "a");
        assert!(has_self_cycle);
    }

    #[test]
    fn test_node_and_edge_count() {
        let graph = sample_graph();
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);

        let mixed = mixed_graph();
        assert_eq!(mixed.edge_count(), 1);
    }

    #[test]
    fn test_dev_and_peer_deps() {
        let members = vec![
            make_member("a", vec![], vec![("b", "workspace:*")], vec![]),
            make_member("b", vec![], vec![], vec![]),
        ];
        let ws = make_workspace(members);
        let graph = PackageGraph::from_workspace(&ws);

        let deps_a = graph.dependencies("a").unwrap();
        assert_eq!(deps_a.len(), 1);
        assert_eq!(deps_a[0].target, "b");

        let sorted = graph.topological_sort().unwrap();
        assert_eq!(sorted, vec!["b", "a"]);
    }

    #[test]
    fn test_dedup_same_dep_in_multiple_categories() {
        let members = vec![make_member(
            "a",
            vec![("b", "workspace:*")],
            vec![("b", "workspace:*")],
            vec![],
        )];
        let ws = make_workspace(members);
        let graph = PackageGraph::from_workspace(&ws);
        let deps_a = graph.dependencies("a").unwrap();
        assert_eq!(deps_a.len(), 1);
    }
}
