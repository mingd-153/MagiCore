//! Topo order — Kahn levels: level i install song song, level-to-level tuần tự.

use crate::WorkspaceGraph;

#[derive(Debug, PartialEq, Eq)]
pub enum TopoError {
    Cycle(Vec<String>),
}

impl std::fmt::Display for TopoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TopoError::Cycle(path) => {
                write!(f, "workspace dependency cycle: {}", path.join(" -> "))
            }
        }
    }
}

impl std::error::Error for TopoError {}

/// Kahn: trả về levels — mỗi level = Vec node index install song song được.
/// Node phụ thuộc node khác → ở level sau. Cycle → Err kèm path minh hoạ.
pub fn topo_levels(graph: &WorkspaceGraph) -> Result<Vec<Vec<usize>>, TopoError> {
    let n = graph.nodes.len();
    let mut indegree = vec![0usize; n];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n];

    for edge in &graph.edges {
        indegree[edge.to] += 1;
        dependents[edge.from].push(edge.to);
    }

    let mut queue: std::collections::VecDeque<usize> = indegree
        .iter()
        .enumerate()
        .filter(|(_, &degree)| degree == 0)
        .map(|(index, _)| index)
        .collect();

    let mut levels: Vec<Vec<usize>> = Vec::new();
    let mut processed = 0usize;

    while !queue.is_empty() {
        let mut frontier = std::collections::VecDeque::new();
        let mut level = Vec::new();
        while let Some(node) = queue.pop_front() {
            processed += 1;
            level.push(node);
            for &next in &dependents[node] {
                indegree[next] -= 1;
                if indegree[next] == 0 {
                    frontier.push_back(next);
                }
            }
        }
        levels.push(level);
        queue = frontier;
    }

    if processed != n {
        let cycle = cycle_path(graph, &indegree);
        return Err(TopoError::Cycle(cycle));
    }

    Ok(levels)
}

/// Truy vết 1 vòng cycle (nodes còn indegree>0) để chẩn đoán.
fn cycle_path(graph: &WorkspaceGraph, indegree: &[usize]) -> Vec<String> {
    let remaining: Vec<usize> = indegree
        .iter()
        .enumerate()
        .filter(|(_, &degree)| degree > 0)
        .map(|(index, _)| index)
        .collect();
    if remaining.is_empty() {
        return Vec::new();
    }
    // Đi từ 1 node còn lại, follow edge đến khi gặp lại node đã thấy.
    let start = remaining[0];
    let mut path = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut current = start;
    loop {
        if !seen.insert(current) {
            break;
        }
        path.push(current);
        let next = graph
            .edges
            .iter()
            .find(|edge| edge.from == current)
            .map(|edge| edge.to)
            .or_else(|| remaining.iter().copied().find(|&r| r != current));
        let Some(next) = next else { break };
        current = next;
    }
    path.iter()
        .map(|&index| format!("'{}'", graph.nodes[index].name))
        .collect()
}
