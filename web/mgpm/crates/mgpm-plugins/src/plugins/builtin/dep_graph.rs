use std::collections::{HashMap, HashSet, VecDeque};

use serde::Serialize;

use super::super::{DepGraph, PluginResult};

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct GraphReport {
    pub dot_format: String,
    pub adjacency_list: HashMap<String, Vec<String>>,
    pub degree_counts: Vec<NodeDegrees>,
    pub circular_dependencies: Vec<Vec<String>>,
    pub max_depth: usize,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct NodeDegrees {
    pub name: String,
    pub in_degree: usize,
    pub out_degree: usize,
}

pub struct DepGraphPlugin;

impl DepGraphPlugin {
    pub fn name(&self) -> &'static str {
        "builtin:dep-graph"
    }

    pub fn generate_graph(graph: &DepGraph) -> PluginResult {
        let nodes = &graph.nodes;
        let edges = &graph.edges;

        // Build adjacency list
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        let mut reverse_adj: HashMap<String, Vec<String>> = HashMap::new();
        let mut node_set: HashSet<String> = HashSet::new();

        for node in nodes {
            adj.entry(node.clone()).or_default();
            reverse_adj.entry(node.clone()).or_default();
            node_set.insert(node.clone());
        }

        for (from, to) in edges {
            node_set.insert(from.clone());
            node_set.insert(to.clone());
            adj.entry(from.clone()).or_default().push(to.clone());
            reverse_adj.entry(to.clone()).or_default().push(from.clone());
        }

        // Ensure all edges' nodes are in adj
        for (from, to) in edges {
            adj.entry(from.clone()).or_default();
            adj.entry(to.clone()).or_default();
            reverse_adj.entry(from.clone()).or_default();
            reverse_adj.entry(to.clone()).or_default();
        }

        // Degree counts
        let mut degree_counts: Vec<NodeDegrees> = node_set.iter().map(|name| NodeDegrees {
            name: name.clone(),
            in_degree: reverse_adj.get(name).map_or(0, |v| v.len()),
            out_degree: adj.get(name).map_or(0, |v| v.len()),
        }).collect();
        degree_counts.sort_by(|a, b| b.out_degree.cmp(&a.out_degree).then(b.in_degree.cmp(&a.in_degree)));

        // Circular dependency detection (DFS with coloring)
        let mut circular = find_circular_dependencies(&adj, &node_set);

        // Deduplicate cycles (rotate to canonical form)
        circular.sort();
        circular.dedup();

        // Max depth calculation (BFS from root nodes)
        let max_depth = calculate_max_depth(&adj, &reverse_adj, &node_set);

        // DOT format
        let dot_format = generate_dot(&node_set, edges);

        // Build adjacency list for JSON output
        let adjacency_list: HashMap<String, Vec<String>> = node_set.iter()
            .map(|n| (n.clone(), adj.get(n).cloned().unwrap_or_default()))
            .collect();

        let report = GraphReport {
            dot_format,
            adjacency_list,
            degree_counts,
            circular_dependencies: circular,
            max_depth,
        };

        let data = serde_json::to_string(&report).unwrap_or_default();

        PluginResult {
            success: true,
            message: format!(
                "Dep graph: {} nodes, {} edges, {} circular deps, max depth {}",
                node_set.len(),
                edges.len(),
                report.circular_dependencies.len(),
                max_depth,
            ),
            data: Some(data),
        }
    }
}

fn generate_dot(nodes: &HashSet<String>, edges: &[(String, String)]) -> String {
    let mut dot = String::from("digraph G {\n");
    dot.push_str("  rankdir=LR;\n");
    dot.push_str("  node [shape=box, style=rounded];\n");

    for node in nodes {
        let safe = node.replace('"', "\\\"");
        dot.push_str(&format!("  \"{}\";\n", safe));
    }

    for (from, to) in edges {
        let from_safe = from.replace('"', "\\\"");
        let to_safe = to.replace('"', "\\\"");
        dot.push_str(&format!("  \"{}\" -> \"{}\";\n", from_safe, to_safe));
    }

    dot.push_str("}\n");
    dot
}

fn find_circular_dependencies(
    adj: &HashMap<String, Vec<String>>,
    node_set: &HashSet<String>,
) -> Vec<Vec<String>> {
    enum Color {
        White,
        Gray,
        Black,
    }

    let mut color: HashMap<String, Color> = HashMap::new();
    let mut parent: HashMap<String, String> = HashMap::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    for node in node_set {
        color.insert(node.clone(), Color::White);
    }

    fn dfs(
        node: &str,
        adj: &HashMap<String, Vec<String>>,
        color: &mut HashMap<String, Color>,
        parent: &mut HashMap<String, String>,
        cycles: &mut Vec<Vec<String>>,
        path: &mut Vec<String>,
    ) {
        color.insert(node.to_string(), Color::Gray);
        path.push(node.to_string());

        if let Some(neighbors) = adj.get(node) {
            for neighbor in neighbors {
                match color.get(neighbor) {
                    Some(Color::Gray) => {
                        // Found a cycle — extract it
                        let mut cycle: Vec<String> = Vec::new();
                        if let Some(pos) = path.iter().position(|n| n == neighbor) {
                            for p in path.iter().skip(pos) {
                                cycle.push(p.clone());
                            }
                        }
                        cycle.push(neighbor.clone());
                        if !cycle.is_empty() {
                            // Canonicalize: rotate so smallest element is first
                            if let Some(min_pos) = cycle.iter().enumerate()
                                .min_by(|(_, a), (_, b)| a.cmp(b))
                                .map(|(i, _)| i)
                            {
                                let mut canonical: Vec<String> = cycle.iter().skip(min_pos).cloned().collect();
                                canonical.extend(cycle.iter().take(min_pos).cloned().collect::<Vec<_>>());
                                // Remove last if it's a duplicate of first
                                if canonical.len() > 1 && canonical.first() == canonical.last() {
                                    canonical.pop();
                                }
                                cycles.push(canonical);
                            }
                        }
                    }
                    Some(Color::White) => {
                        parent.insert(neighbor.clone(), node.to_string());
                        dfs(neighbor, adj, color, parent, cycles, path);
                    }
                    _ => {}
                }
            }
        }

        path.pop();
        color.insert(node.to_string(), Color::Black);
    }

    let mut path = Vec::new();
    for node in node_set.iter() {
        if matches!(color.get(node), Some(Color::White)) {
            dfs(node, adj, &mut color, &mut parent, &mut cycles, &mut path);
        }
    }

    cycles
}

fn calculate_max_depth(
    adj: &HashMap<String, Vec<String>>,
    reverse_adj: &HashMap<String, Vec<String>>,
    node_set: &HashSet<String>,
) -> usize {
    // Find root nodes (in-degree 0)
    let roots: Vec<String> = node_set.iter()
        .filter(|n| reverse_adj.get(*n).map_or(0, |v| v.len()) == 0)
        .cloned()
        .collect();

    if roots.is_empty() {
        // If no root found, pick any node
        return 0;
    }

    // BFS from each root to find max depth
    let mut max_depth = 0;
    for root in &roots {
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.push_back((root.clone(), 0));

        while let Some((node, depth)) = queue.pop_front() {
            if !visited.insert(node.clone()) {
                continue;
            }
            max_depth = max_depth.max(depth);

            if let Some(neighbors) = adj.get(&node) {
                for neighbor in neighbors {
                    if !visited.contains(neighbor) {
                        queue.push_back((neighbor.clone(), depth + 1));
                    }
                }
            }
        }
    }

    max_depth
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_graph() {
        let graph = DepGraph {
            nodes: vec![],
            edges: vec![],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        assert!(result.success);
    }

    #[test]
    fn test_simple_graph() {
        let graph = DepGraph {
            nodes: vec!["a".into(), "b".into()],
            edges: vec![("a".into(), "b".into())],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        let report: GraphReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(report.dot_format.contains("\"a\" -> \"b\""));
        assert_eq!(report.max_depth, 1);
    }

    #[test]
    fn test_circular_dependency() {
        let graph = DepGraph {
            nodes: vec!["a".into(), "b".into(), "c".into()],
            edges: vec![
                ("a".into(), "b".into()),
                ("b".into(), "c".into()),
                ("c".into(), "a".into()),
            ],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        let report: GraphReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(!report.circular_dependencies.is_empty(), "Expected circular dependency detected");
    }

    #[test]
    fn test_degree_counts() {
        let graph = DepGraph {
            nodes: vec!["a".into(), "b".into(), "c".into()],
            edges: vec![
                ("a".into(), "b".into()),
                ("a".into(), "c".into()),
            ],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        let report: GraphReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        let a_deg = report.degree_counts.iter().find(|d| d.name == "a").unwrap();
        assert_eq!(a_deg.out_degree, 2);
        assert_eq!(a_deg.in_degree, 0);
    }
}
