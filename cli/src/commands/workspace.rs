//! mgc workspace — workspace graph management (T4).
//! (In graph workspace: nodes + edges workspace:* deps, filter select subset)

use anyhow::{bail, Result};
use clap::Subcommand;
use mgc_config::project::ProjectConfig;
use std::path::Path;

#[derive(Subcommand, Debug, Clone)]
pub enum WorkspaceCmd {
    /// Show workspace graph (nodes + workspace:* edges)
    List {
        /// Filter subset — glob on package name (@core/*) or relative path (./apps/*)
        #[arg(long)]
        filter: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

/// mgc workspace list — in graph tại project root.
pub async fn run(cmd: WorkspaceCmd) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let project_root =
        ProjectConfig::find_project_root(&cwd).ok_or_else(crate::error::project_root_missing)?;

    match cmd {
        WorkspaceCmd::List { filter, json } => list(&project_root, filter.as_deref(), json),
    }
}

fn list(project_root: &Path, filter: Option<&str>, json: bool) -> Result<()> {
    let targets = mgc_workspace::discover_workspace_targets(project_root)?;
    let graph = mgc_workspace::build_workspace_graph(&targets)?;

    if graph.is_empty() {
        if json {
            println!("{}", serde_json::json!({ "nodes": [], "edges": [] }));
        } else {
            mgc_ui::info("No workspace packages found (expect apps/ + packages/ with manifests).");
        }
        return Ok(());
    }

    let mut selected: Vec<usize> = (0..graph.nodes.len()).collect();
    if let Some(pattern) = filter {
        selected = (0..graph.nodes.len())
            .filter(|&i| {
                let node = &graph.nodes[i];
                let relative = node.path.strip_prefix(project_root).unwrap_or(&node.path);
                mgc_workspace::filter_matches(pattern, relative, &node.name)
            })
            .collect();
        if selected.is_empty() {
            bail!("filter '{pattern}' matched no workspace package");
        }
    }

    let selected_set: std::collections::HashSet<usize> = selected.iter().copied().collect();
    let edges: Vec<serde_json::Value> = graph
        .edges
        .iter()
        .filter(|edge| selected_set.contains(&edge.from))
        .map(|edge| {
            serde_json::json!({
                "from": graph.nodes[edge.from].name,
                "to": graph.nodes[edge.to].name,
            })
        })
        .collect();

    if json {
        let nodes: Vec<serde_json::Value> = selected
            .iter()
            .map(|&i| {
                serde_json::json!({
                    "name": graph.nodes[i].name,
                    "path": graph.nodes[i].path.to_string_lossy(),
                })
            })
            .collect();
        println!("{}", serde_json::json!({ "nodes": nodes, "edges": edges }));
        return Ok(());
    }

    for &i in &selected {
        println!(
            "{}  ({})",
            graph.nodes[i].name,
            graph.nodes[i].path.display()
        );
    }
    for edge in &edges {
        println!(
            "  {} -> {}",
            edge["from"].as_str().unwrap_or("?"),
            edge["to"].as_str().unwrap_or("?")
        );
    }
    Ok(())
}
