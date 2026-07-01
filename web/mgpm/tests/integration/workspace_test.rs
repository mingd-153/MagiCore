//! Integration tests for workspace discovery, dependency graph, topological sort, and filtering.

use std::fs;
use std::path::Path;

use mgpm_core::{MgpmConfig, WorkspaceConfig, SecurityConfig, LinkerMode};
use std::collections::HashMap;
use mgpm_workspace::{FilterSelector, Workspace};

fn create_member(root: &Path, subdir: &str, name: &str, version: &str, deps: &[(&str, &str)]) {
    let dir = root.join(subdir);
    fs::create_dir_all(&dir).unwrap();

    let mut deps_map = serde_json::Map::new();
    for (k, v) in deps {
        deps_map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
    }
    let deps_val = serde_json::Value::Object(deps_map);

    let pkg_json = serde_json::json!({
        "name": name,
        "version": version,
        "dependencies": deps_val,
    });
    fs::write(dir.join("package.json"), serde_json::to_string_pretty(&pkg_json).unwrap()).unwrap();
}

fn create_workspace_with_members(packages_pattern: &str, members: &[(&str, &str, &str, &[(&str, &str)])]) -> Workspace {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();

    let config = MgpmConfig {
        workspace: Some(WorkspaceConfig {
            packages: vec![packages_pattern.to_string()],
            catalog: None,
            link_ws_packages: true,
            catalogs: HashMap::new(),
            shared_lockfile: true,
            hoist: false,
            scripts: HashMap::new(),
            security: SecurityConfig::default(),
            linker: LinkerMode::default(),
        }),
        ..Default::default()
    };
    config.save(&root.join("mgpm.yaml")).unwrap();

    let pkgs_dir = packages_pattern.trim_end_matches("/*");
    for (subdir, name, version, deps) in members {
        create_member(&root, &format!("{}/{}", pkgs_dir, subdir), name, version, deps);
    }

    Workspace::discover(&root).unwrap()
}

fn create_workspace_from_json(root: &Path, packages_pattern: &str, members: &[(&str, &str, &str, &[(&str, &str)])]) -> Workspace {
    let pkg_json = serde_json::json!({
        "name": "root-ws",
        "version": "0.0.0",
        "workspaces": [packages_pattern]
    });
    fs::write(root.join("package.json"), serde_json::to_string_pretty(&pkg_json).unwrap()).unwrap();

    let pkgs_dir = packages_pattern.trim_end_matches("/*");
    for (subdir, name, version, deps) in members {
        create_member(root, &format!("{}/{}", pkgs_dir, subdir), name, version, deps);
    }

    Workspace::discover(root).unwrap()
}

#[test]
fn test_workspace_discovery_with_glob() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-a", "pkg-a", "1.0.0", &[]),
            ("pkg-b", "pkg-b", "2.0.0", &[]),
            ("pkg-c", "pkg-c", "3.0.0", &[]),
        ],
    );
    assert_eq!(ws.member_count(), 3);
    assert!(ws.find_member("pkg-a").is_some());
    assert!(ws.find_member("pkg-b").is_some());
    assert!(ws.find_member("pkg-c").is_some());
}

#[test]
fn test_workspace_discovery_via_package_json() {
    let dir = tempfile::tempdir().unwrap();
    let ws = create_workspace_from_json(
        dir.path(),
        "pkgs/*",
        &[
            ("pkg-x", "pkg-x", "0.1.0", &[]),
            ("pkg-y", "pkg-y", "0.2.0", &[]),
        ],
    );
    assert_eq!(ws.member_count(), 2);
    assert_eq!(ws.members()[0].name, "pkg-x");
    assert_eq!(ws.members()[1].name, "pkg-y");
}

#[test]
fn test_workspace_discovery_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let err = Workspace::discover(dir.path());
    assert!(err.is_err());
}

#[test]
fn test_dependency_graph_returns_correct_adjacency() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-a", "pkg-a", "1.0.0", &[]),
            ("pkg-b", "pkg-b", "1.0.0", &[("pkg-a", "^1.0.0")]),
            ("pkg-c", "pkg-c", "1.0.0", &[("pkg-b", "^1.0.0")]),
        ],
    );
    let graph = ws.dependency_graph();

    assert_eq!(graph.len(), 3);
    assert!(graph.get("pkg-a").unwrap().is_empty());
    assert_eq!(graph.get("pkg-b").unwrap(), &vec!["pkg-a".to_string()]);
    assert_eq!(graph.get("pkg-c").unwrap(), &vec!["pkg-b".to_string()]);
}

#[test]
fn test_dependency_graph_no_internal_deps() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-a", "pkg-a", "1.0.0", &[]),
            ("pkg-b", "pkg-b", "1.0.0", &[]),
        ],
    );
    let graph = ws.dependency_graph();
    for deps in graph.values() {
        assert!(deps.is_empty());
    }
}

#[test]
fn test_dependency_graph_external_deps_not_included() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-a", "pkg-a", "1.0.0", &[("react", "^18.0.0")]),
        ],
    );
    let graph = ws.dependency_graph();
    assert!(graph.get("pkg-a").unwrap().is_empty());
}

#[test]
fn test_topological_sort_returns_correct_order() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-a", "pkg-a", "1.0.0", &[]),
            ("pkg-b", "pkg-b", "1.0.0", &[("pkg-a", "^1.0.0")]),
            ("pkg-c", "pkg-c", "1.0.0", &[("pkg-b", "^1.0.0")]),
        ],
    );
    let sorted = ws.topological_sort().unwrap();
    let names: Vec<&str> = sorted.iter().map(|m| m.name.as_str()).collect();

    assert_eq!(names.len(), 3);
    let a_pos = names.iter().position(|n| *n == "pkg-a").unwrap();
    let b_pos = names.iter().position(|n| *n == "pkg-b").unwrap();
    let c_pos = names.iter().position(|n| *n == "pkg-c").unwrap();
    assert!(a_pos < b_pos, "pkg-a should come before pkg-b");
    assert!(b_pos < c_pos, "pkg-b should come before pkg-c");
}

#[test]
fn test_topological_sort_no_deps_returns_all() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-a", "pkg-a", "1.0.0", &[]),
            ("pkg-b", "pkg-b", "1.0.0", &[]),
        ],
    );
    let sorted = ws.topological_sort().unwrap();
    assert_eq!(sorted.len(), 2);
}

#[test]
fn test_topological_sort_cycle_detected() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-x", "pkg-x", "1.0.0", &[("pkg-y", "^1.0.0")]),
            ("pkg-y", "pkg-y", "1.0.0", &[("pkg-z", "^1.0.0")]),
            ("pkg-z", "pkg-z", "1.0.0", &[("pkg-x", "^1.0.0")]),
        ],
    );
    let result = ws.topological_sort();
    assert!(result.is_err(), "Expected error due to circular dependency");
}

#[test]
fn test_topological_sort_self_cycle() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-a", "pkg-a", "1.0.0", &[("pkg-a", "^1.0.0")]),
        ],
    );
    let result = ws.topological_sort();
    assert!(result.is_err(), "Self-dependency should be detected as a cycle");
}

#[test]
fn test_topological_sort_diamond_deps() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("base", "base", "1.0.0", &[]),
            ("left", "left", "1.0.0", &[("base", "^1.0.0")]),
            ("right", "right", "1.0.0", &[("base", "^1.0.0")]),
            ("top", "top", "1.0.0", &[("left", "^1.0.0"), ("right", "^1.0.0")]),
        ],
    );
    let sorted = ws.topological_sort().unwrap();
    let names: Vec<&str> = sorted.iter().map(|m| m.name.as_str()).collect();

    assert_eq!(names.len(), 4);
    let base_pos = names.iter().position(|n| *n == "base").unwrap();
    let left_pos = names.iter().position(|n| *n == "left").unwrap();
    let right_pos = names.iter().position(|n| *n == "right").unwrap();
    let top_pos = names.iter().position(|n| *n == "top").unwrap();

    assert!(base_pos < left_pos);
    assert!(base_pos < right_pos);
    assert!(left_pos < top_pos);
    assert!(right_pos < top_pos);
}

#[test]
fn test_filter_by_name() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-a", "pkg-a", "1.0.0", &[]),
            ("pkg-b", "pkg-b", "1.0.0", &[]),
        ],
    );
    let result = ws.filter(&FilterSelector::Name("pkg-a".to_string()));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "pkg-a");
}

#[test]
fn test_filter_by_name_not_found() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[("pkg-a", "pkg-a", "1.0.0", &[])],
    );
    let result = ws.filter(&FilterSelector::Name("nonexistent".to_string()));
    assert!(result.is_empty());
}

#[test]
fn test_filter_by_glob() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-a", "pkg-a", "1.0.0", &[]),
            ("pkg-b", "pkg-b", "1.0.0", &[]),
            ("other", "other", "1.0.0", &[]),
        ],
    );
    let result = ws.filter(&FilterSelector::Glob("pkg-*".to_string()));
    assert_eq!(result.len(), 2);
    assert!(result.iter().any(|m| m.name == "pkg-a"));
    assert!(result.iter().any(|m| m.name == "pkg-b"));
}

#[test]
fn test_filter_by_glob_no_match() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[("pkg-a", "pkg-a", "1.0.0", &[])],
    );
    let result = ws.filter(&FilterSelector::Glob("nomatch-*".to_string()));
    assert!(result.is_empty());
}

#[test]
fn test_filter_dependents() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-a", "pkg-a", "1.0.0", &[]),
            ("pkg-b", "pkg-b", "1.0.0", &[("pkg-a", "^1.0.0")]),
            ("pkg-c", "pkg-c", "1.0.0", &[("pkg-b", "^1.0.0")]),
        ],
    );
    let result = ws.filter(&FilterSelector::Dependents("pkg-a".to_string()));
    let names: Vec<&str> = result.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(result.len(), 3);
    assert!(names.contains(&"pkg-a"));
    assert!(names.contains(&"pkg-b"));
    assert!(names.contains(&"pkg-c"));
}

#[test]
fn test_filter_dependents_leaf() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-a", "pkg-a", "1.0.0", &[]),
            ("pkg-b", "pkg-b", "1.0.0", &[("pkg-a", "^1.0.0")]),
        ],
    );
    let result = ws.filter(&FilterSelector::Dependents("pkg-b".to_string()));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "pkg-b");
}

#[test]
fn test_filter_dependencies() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-a", "pkg-a", "1.0.0", &[]),
            ("pkg-b", "pkg-b", "1.0.0", &[("pkg-a", "^1.0.0")]),
            ("pkg-c", "pkg-c", "1.0.0", &[("pkg-b", "^1.0.0")]),
        ],
    );
    let result = ws.filter(&FilterSelector::Dependencies("pkg-c".to_string()));
    let names: Vec<&str> = result.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(result.len(), 3);
    assert!(names.contains(&"pkg-c"));
    assert!(names.contains(&"pkg-b"));
    assert!(names.contains(&"pkg-a"));
}

#[test]
fn test_filter_dependencies_root() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("pkg-a", "pkg-a", "1.0.0", &[]),
            ("pkg-b", "pkg-b", "1.0.0", &[("pkg-a", "^1.0.0")]),
        ],
    );
    let result = ws.filter(&FilterSelector::Dependencies("pkg-a".to_string()));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "pkg-a");
}

#[test]
fn test_resolve_dependency() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[("my-pkg", "my-pkg", "1.0.0", &[])],
    );
    let resolved = ws.resolve_dependency("my-pkg");
    assert!(resolved.is_some());
    assert!(resolved.unwrap().ends_with("my-pkg"));

    let missing = ws.resolve_dependency("not-here");
    assert!(missing.is_none());
}

#[test]
fn test_member_count() {
    let ws = create_workspace_with_members(
        "packages/*",
        &[
            ("a", "a", "1.0.0", &[]),
            ("b", "b", "1.0.0", &[]),
            ("c", "c", "1.0.0", &[]),
        ],
    );
    assert_eq!(ws.member_count(), 3);
}

#[test]
fn test_config() {
    let ws = create_workspace_with_members(
        "libs/*",
        &[("lib-a", "lib-a", "1.0.0", &[])],
    );
    assert!(ws.config().link_ws_packages);
    assert_eq!(ws.config().packages, vec!["libs/*"]);
}
