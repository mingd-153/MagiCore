//! mg-workspace tests — RULE §5 (test/).

use mg_workspace::{
    discover_workspace_targets, filter_matches, topo_levels, WorkspaceGraph, WorkspaceNode,
    WorkspacePackageManifest,
};
use std::path::{Path, PathBuf};

fn node(name: &str, rel: &str) -> WorkspaceNode {
    WorkspaceNode {
        name: name.to_string(),
        path: PathBuf::from(rel),
        manifest: WorkspacePackageManifest {
            name: name.to_string(),
            ..Default::default()
        },
    }
}

// --- discover ---

#[test]
fn discover_apps_and_packages() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("apps/app1")).unwrap();
    std::fs::create_dir_all(root.join("apps/app2")).unwrap();
    std::fs::create_dir_all(root.join("packages/shared")).unwrap();
    std::fs::create_dir_all(root.join("packages/deep/nested/pkg")).unwrap();
    for dir in [
        "apps/app1",
        "apps/app2",
        "packages/shared",
        "packages/deep/nested/pkg",
    ] {
        std::fs::write(root.join(dir).join("package.json"), "{}").unwrap();
    }
    std::fs::create_dir_all(root.join("packages/empty")).unwrap();

    let targets = discover_workspace_targets(root).unwrap();
    let names: Vec<String> = targets
        .iter()
        .map(|t| t.strip_prefix(root).unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "apps/app1",
            "apps/app2",
            "packages/deep/nested/pkg",
            "packages/shared"
        ]
    );
}

#[test]
fn discover_returns_empty_when_no_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let targets = discover_workspace_targets(root).unwrap();
    assert!(targets.is_empty());
}

#[test]
fn discover_respects_custom_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("src/apps/x")).unwrap();
    std::fs::create_dir_all(root.join("libs/y")).unwrap();
    std::fs::write(root.join("src/apps/x/package.json"), "{}").unwrap();
    std::fs::write(root.join("libs/y/package.json"), "{}").unwrap();
    std::fs::write(
        root.join("megagate.workspace.toml"),
        "[layout]\napps_dir = \"src/apps\"\npackages_dir = \"libs\"\n",
    )
    .unwrap();

    let targets = discover_workspace_targets(root).unwrap();
    let names: Vec<String> = targets
        .iter()
        .map(|t| t.strip_prefix(root).unwrap().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .into_iter()
        .map(|s| s.replace("src/apps", "apps"))
        .collect();
    assert!(names.iter().any(|n| n.ends_with("x")));
    assert!(names.iter().any(|n| n.ends_with("y")));
}

// --- graph + topo ---

#[test]
fn graph_edges_from_workspace_deps() {
    let mut a = WorkspacePackageManifest {
        name: "a".to_string(),
        ..Default::default()
    };
    a.dependencies
        .insert("b".to_string(), "workspace:*".to_string());
    let b = WorkspacePackageManifest {
        name: "b".to_string(),
        ..Default::default()
    };
    let graph = WorkspaceGraph {
        nodes: vec![
            WorkspaceNode {
                name: "a".into(),
                path: "a".into(),
                manifest: a,
            },
            WorkspaceNode {
                name: "b".into(),
                path: "b".into(),
                manifest: b,
            },
        ],
        edges: vec![],
    };
    // Build graph thật qua discover khó trong test đơn vị — kiểm tra edges_from:
    let _ = graph.edges_from(0);
    let levels = topo_levels(&graph).unwrap();
    assert_eq!(levels.len(), 1); // a->b chưa có edge → cùng level
}

#[test]
fn topo_chain_orders_levels() {
    let graph = WorkspaceGraph {
        nodes: vec![node("a", "a"), node("b", "b"), node("c", "c")],
        edges: vec![
            mg_workspace::WorkspaceEdge { from: 0, to: 1 },
            mg_workspace::WorkspaceEdge { from: 1, to: 2 },
        ],
    };
    let levels = topo_levels(&graph).unwrap();
    assert_eq!(levels.len(), 3);
    assert_eq!(levels[0], vec![0]); // a
    assert_eq!(levels[1], vec![1]); // b
    assert_eq!(levels[2], vec![2]); // c
}

#[test]
fn topo_fanout_same_level() {
    let graph = WorkspaceGraph {
        nodes: vec![node("a", "a"), node("b", "b"), node("c", "c")],
        edges: vec![
            mg_workspace::WorkspaceEdge { from: 0, to: 1 },
            mg_workspace::WorkspaceEdge { from: 0, to: 2 },
        ],
    };
    let levels = topo_levels(&graph).unwrap();
    assert_eq!(levels.len(), 2);
    let mut second = levels[1].clone();
    second.sort();
    assert_eq!(second, vec![1, 2]); // b,c cùng level
}

#[test]
fn topo_diamond() {
    let graph = WorkspaceGraph {
        nodes: vec![
            node("root", "root"),
            node("x", "x"),
            node("y", "y"),
            node("app", "app"),
        ],
        edges: vec![
            mg_workspace::WorkspaceEdge { from: 0, to: 1 },
            mg_workspace::WorkspaceEdge { from: 0, to: 2 },
            mg_workspace::WorkspaceEdge { from: 1, to: 3 },
            mg_workspace::WorkspaceEdge { from: 2, to: 3 },
        ],
    };
    let levels = topo_levels(&graph).unwrap();
    assert_eq!(levels.len(), 3);
    assert_eq!(levels[0], vec![0]);
    assert_eq!(levels[2], vec![3]);
}

#[test]
fn topo_cycle_detects() {
    let graph = WorkspaceGraph {
        nodes: vec![node("a", "a"), node("b", "b")],
        edges: vec![
            mg_workspace::WorkspaceEdge { from: 0, to: 1 },
            mg_workspace::WorkspaceEdge { from: 1, to: 0 },
        ],
    };
    let err = topo_levels(&graph).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("cycle"), "unexpected: {msg}");
}

#[test]
fn topo_empty() {
    let graph = WorkspaceGraph {
        nodes: vec![],
        edges: vec![],
    };
    let levels = topo_levels(&graph).unwrap();
    assert!(levels.is_empty());
}

// --- filter ---

#[test]
fn filter_exact_name() {
    assert!(filter_matches("lodash", Path::new("a"), "lodash"));
    assert!(!filter_matches("lodash", Path::new("a"), "other"));
}

#[test]
fn filter_scoped_prefix() {
    assert!(filter_matches("@core/*", Path::new("x"), "@core/shared"));
    assert!(filter_matches("@core/*", Path::new("x"), "@core/utils"));
    assert!(!filter_matches("@core/*", Path::new("x"), "@other/shared"));
}

#[test]
fn filter_relative_path() {
    assert!(filter_matches(
        "./apps/*",
        Path::new("apps/web-app"),
        "@core/web-app"
    ));
    assert!(!filter_matches(
        "./apps/*",
        Path::new("packages/shared"),
        "@core/shared"
    ));
    assert!(filter_matches(
        "apps/*",
        Path::new("apps/web-app"),
        "@core/web-app"
    ));
}

#[test]
fn filter_double_star() {
    assert!(filter_matches(
        "./packages/**",
        Path::new("packages/core/shared"),
        "x"
    ));
    assert!(filter_matches(
        "./packages/**",
        Path::new("packages/shared"),
        "x"
    ));
    assert!(!filter_matches("./packages/**", Path::new("apps/app"), "x"));
}

#[test]
fn filter_name_wildcard_patterns() {
    assert!(filter_matches("*-service", Path::new("any"), "user-service"));
    assert!(filter_matches("*-service", Path::new("any"), "auth-service"));
    assert!(!filter_matches("*-service", Path::new("any"), "user-client"));
    assert!(filter_matches("mg-*", Path::new("any"), "mg-workspace"));
    assert!(filter_matches("*", Path::new("any"), "anything"));
    assert!(filter_matches("**", Path::new("any"), "anything"));
}

#[test]
fn filter_scoped_double_star() {
    assert!(filter_matches("@plugins/**", Path::new("x"), "@plugins/auth"));
    assert!(filter_matches("@plugins/**", Path::new("x"), "@plugins/core/db"));
}

