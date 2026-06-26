use megagate_types::package::ResolvedDependency;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct DependencyNode {
    pub name: String,
    pub range: String,
    pub resolved: Option<ResolvedDependency>,
    pub dependencies: HashMap<String, DependencyNode>,
    pub dependents: HashSet<String>,
}

impl DependencyNode {
    pub fn key(&self) -> String {
        format!("{}@{}", self.name, self.range)
    }
}

#[derive(Debug, Clone)]
pub struct DependencyGraph {
    pub nodes: HashMap<String, DependencyNode>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, name: String, range: String) -> &mut DependencyNode {
        let key = format!("{}@{}", name, range);
        self.nodes
            .entry(key.clone())
            .or_insert_with(|| DependencyNode {
                name,
                range,
                resolved: None,
                dependencies: HashMap::new(),
                dependents: HashSet::new(),
            })
    }

    pub fn get_node(&self, name: &str, version: &semver::Version) -> Option<&DependencyNode> {
        self.nodes.get(&format!("{}@{}", name, version))
    }

    pub fn detect_conflicts(&self) -> Vec<Conflict> {
        let mut by_name: HashMap<String, Vec<&DependencyNode>> = HashMap::new();
        for node in self.nodes.values() {
            by_name
                .entry(node.name.clone())
                .or_default()
                .push(node);
        }

        let mut conflicts = Vec::new();
        for (name, nodes) in by_name {
            let mut versions = Vec::new();
            for node in nodes {
                if let Some(resolved) = &node.resolved {
                    versions.push((resolved.version.clone(), node.dependents.clone()));
                }
            }
            if versions.len() > 1 {
                versions.sort_by(|a, b| b.0.cmp(&a.0));
                let highest = versions[0].0.clone();
                let compatible: Vec<_> = versions
                    .iter()
                    .filter(|(v, _)| semver::VersionReq::parse(&format!("^{}", highest)).unwrap().matches(v))
                    .collect();
                if compatible.len() != versions.len() {
                    conflicts.push(Conflict {
                        name,
                        versions: versions
                            .into_iter()
                            .map(|(v, r)| VersionConflict {
                                version: v,
                                required_by: r.into_iter().collect(),
                            })
                            .collect(),
                    });
                }
            }
        }
        conflicts
    }

    pub fn get_resolution_order(&self) -> Vec<&DependencyNode> {
        let mut visited = HashSet::new();
        let mut order = Vec::new();

        fn visit<'a>(
            node: &'a DependencyNode,
            nodes: &HashMap<String, DependencyNode>,
            visited: &mut HashSet<String>,
            order: &mut Vec<&'a DependencyNode>,
        ) {
            let key = node.key();
            if visited.contains(&key) {
                return;
            }
            visited.insert(key);
            for dep in node.dependencies.values() {
                visit(dep, nodes, visited, order);
            }
            order.push(node);
        }

        for node in self.nodes.values() {
            visit(node, &self.nodes, &mut visited, &mut order);
        }
        order
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct Conflict {
    pub name: String,
    pub versions: Vec<VersionConflict>,
}

#[derive(Debug, Clone)]
pub struct VersionConflict {
    pub version: semver::Version,
    pub required_by: Vec<String>,
}