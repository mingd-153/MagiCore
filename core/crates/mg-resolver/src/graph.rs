/// Dependency graph representation
use mg_types::PackageId;
use std::collections::{HashMap, HashSet};

pub struct DependencyGraph {
    nodes: HashSet<PackageId>,
    edges: HashMap<PackageId, Vec<PackageId>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashSet::new(),
            edges: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, id: PackageId) {
        self.nodes.insert(id);
    }

    pub fn add_edge(&mut self, from: PackageId, to: PackageId) {
        self.edges
            .entry(from)
            .or_insert_with(Vec::new)
            .push(to);
    }

    pub fn dependencies(&self, id: &PackageId) -> Option<&[PackageId]> {
        self.edges.get(id).map(|v| v.as_slice())
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}
