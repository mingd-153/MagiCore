//! Filter engine for workspace package selection.
//!
//! Provides a structured filter syntax and engine for selecting subsets of
//! workspace packages. Inspired by pnpm's `--filter` syntax with extensions.
//!
//! # Filter Syntax
//!
//! | Syntax | Variant | Example |
//! |--------|---------|---------|
//! | `@scope/pkg` | Exact name | `--filter=@scope/pkg` |
//! | `@scope/*` | Name glob | `--filter=@scope/*` |
//! | `./apps/*` | Path glob | `--filter=./apps/*` |
//! | `pkg...` | Package + deps | `--filter=pkg-a...` |
//! | `...pkg` | Package + dependents | `--filter=...pkg-a` |
//! | `...pkg...` | Both directions | `--filter=...pkg-a...` |
//! | `[HEAD~1]` | Changed since ref | `--filter="[HEAD~1]"` |
//! | `{apps/*}[main]` | Changed + path filter | `--filter="{apps/*}[main]"` |

use std::collections::HashSet;

use crate::{PackageGraph, Workspace, WorkspaceError, WorkspaceMember};

/// Filter selector for workspace package selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterSelector {
    /// Exact package name match: @scope/pkg
    Name(String),

    /// Glob pattern on package name: @scope/*
    NameGlob(String),

    /// Glob pattern on package path: ./apps/*
    PathGlob(String),

    /// Package + its dependencies: pkg...
    Dependencies(String),

    /// Package + its dependents: ...pkg
    Dependents(String),

    /// Package + both directions: ...pkg...
    All(String),

    /// Changed packages since git ref: [HEAD~1] or {./apps/*}[main]
    ChangedSince {
        /// Optional path glob filter
        path_filter: Option<String>,
        /// Git reference to compare against
        git_ref: String,
    },
}

/// Errors that can occur during filter operations.
#[derive(Debug, thiserror::Error)]
pub enum FilterError {
    #[error("invalid filter syntax: '{0}' — expected format: <selector>")]
    InvalidSyntax(String),

    #[error("unknown filter operator: '{0}' — supported: ..., [...], {{path}}[ref]")]
    UnknownOperator(String),

    #[error("package '{0}' not found in workspace")]
    PackageNotFound(String),

    #[error("invalid glob pattern: {0}")]
    InvalidGlob(String),

    #[error("git diff failed: {0}")]
    GitError(String),

    #[error("{0}")]
    WorkspaceError(#[from] WorkspaceError),
}

/// Filter engine for selecting workspace packages.
///
/// Wraps a [`Workspace`] and [`PackageGraph`] to provide structured filtering
/// of workspace members based on [`FilterSelector`] syntax.
///
/// # Example
///
/// ```rust,ignore
/// let engine = FilterEngine::new(workspace, graph);
/// let selectors = FilterEngine::parse_selectors(&["@scope/ui...".into(), "./apps/*".into()]).unwrap();
/// let members = engine.apply(&selectors).unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct FilterEngine {
    workspace: Workspace,
    graph: PackageGraph,
}

impl FilterEngine {
    /// Create a new filter engine from a workspace and its package graph.
    pub fn new(workspace: Workspace, graph: PackageGraph) -> Self {
        Self { workspace, graph }
    }

    /// Parse a single filter string into a [`FilterSelector`].
    ///
    /// Supports all syntax variants documented at the module level.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::InvalidSyntax`] if the input does not match
    /// any recognized filter pattern.
    pub fn parse_selector(input: &str) -> Result<FilterSelector, FilterError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(FilterError::InvalidSyntax("empty filter string".into()));
        }

        // ChangedSince with path filter: {<path>}[<ref>]
        if let Some(rest) = input.strip_prefix('{') {
            if let Some(close_brace) = rest.find('}') {
                let after_brace = &rest[close_brace + 1..];
                if let Some(ref_content) = after_brace
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                {
                    if ref_content.is_empty() {
                        return Err(FilterError::InvalidSyntax(input.to_string()));
                    }
                    let path = &rest[..close_brace];
                    return Ok(FilterSelector::ChangedSince {
                        path_filter: if path.is_empty() {
                            None
                        } else {
                            Some(path.to_string())
                        },
                        git_ref: ref_content.to_string(),
                    });
                }
            }
            return Err(FilterError::InvalidSyntax(input.to_string()));
        }

        // ChangedSince without path: [<ref>]
        if input.starts_with('[') {
            if let Some(inner) = input.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                if inner.is_empty() {
                    return Err(FilterError::InvalidSyntax(input.to_string()));
                }
                return Ok(FilterSelector::ChangedSince {
                    path_filter: None,
                    git_ref: inner.to_string(),
                });
            }
            return Err(FilterError::InvalidSyntax(input.to_string()));
        }

        // ...pkg... — both sides
        if let Some(inner) = input
            .strip_prefix("...")
            .and_then(|s| s.strip_suffix("..."))
        {
            if inner.is_empty() {
                return Err(FilterError::InvalidSyntax(input.to_string()));
            }
            return Ok(FilterSelector::All(inner.to_string()));
        }

        // ...pkg — dependents prefix
        if let Some(name) = input.strip_prefix("...") {
            if name.is_empty() {
                return Err(FilterError::InvalidSyntax(input.to_string()));
            }
            return Ok(FilterSelector::Dependents(name.to_string()));
        }

        // pkg... — dependencies suffix
        if let Some(name) = input.strip_suffix("...") {
            if name.is_empty() {
                return Err(FilterError::InvalidSyntax(input.to_string()));
            }
            return Ok(FilterSelector::Dependencies(name.to_string()));
        }

        // Path prefix: ./... or /...
        if input.starts_with("./") || input.starts_with('/') {
            return Ok(FilterSelector::PathGlob(input.to_string()));
        }

        // Glob characters → name glob
        if input.contains('*') || input.contains('?') {
            return Ok(FilterSelector::NameGlob(input.to_string()));
        }

        // Default to exact name
        Ok(FilterSelector::Name(input.to_string()))
    }

    /// Parse multiple filter strings into a vector of [`FilterSelector`]s.
    ///
    /// Each string is parsed independently. Returns the first parse error
    /// encountered.
    pub fn parse_selectors(inputs: &[String]) -> Result<Vec<FilterSelector>, FilterError> {
        inputs.iter().map(|s| Self::parse_selector(s)).collect()
    }

    /// Apply a list of selectors to the workspace, returning the union of
    /// all matching members (deduplicated).
    ///
    /// Multiple selectors are combined with OR semantics: a package is
    /// included if it matches *any* of the selectors.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::PackageNotFound`] if a named package referenced
    /// by a selector does not exist (for `Dependencies`, `Dependents`, `All`).
    /// Returns [`FilterError::GitError`] if a git diff operation fails.
    pub fn apply(
        &self,
        selectors: &[FilterSelector],
    ) -> Result<Vec<&WorkspaceMember>, FilterError> {
        if selectors.is_empty() {
            return Ok(self.workspace.members().iter().collect());
        }

        let mut seen: HashSet<&str> = HashSet::new();
        let mut result: Vec<&WorkspaceMember> = Vec::new();

        for selector in selectors {
            for member in self.workspace.members() {
                if seen.contains(member.name.as_str()) {
                    continue;
                }
                if self.matches_selector(member, selector)? {
                    seen.insert(member.name.as_str());
                    result.push(member);
                }
            }
        }

        Ok(result)
    }

    /// Get changed packages since a git ref, with optional path filtering.
    ///
    /// Runs `git diff --name-only <git_ref> HEAD` (optionally with a path
    /// filter), maps changed files to workspace packages, then includes
    /// transitive dependents using the package graph.
    pub fn changed_since(
        &self,
        path_filter: Option<&str>,
        git_ref: &str,
    ) -> Result<Vec<&WorkspaceMember>, FilterError> {
        let root_str = self.workspace.root().to_str().unwrap_or(".");

        let mut cmd = std::process::Command::new("git");
        cmd.args(["-C", root_str, "diff", "--name-only", git_ref, "HEAD"]);

        if let Some(pf) = path_filter {
            cmd.arg("--").arg(pf);
        }

        let output = cmd
            .output()
            .map_err(|e| FilterError::GitError(format!("failed to execute git diff: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FilterError::GitError(format!(
                "git diff exited with {}: {stderr}",
                output.status
            )));
        }

        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| FilterError::GitError(format!("invalid utf-8 from git: {e}")))?;

        let changed_files: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
        if changed_files.is_empty() {
            return Ok(Vec::new());
        }

        let mut directly_changed: HashSet<&str> = HashSet::new();
        for member in self.workspace.members() {
            let rel = member
                .path
                .strip_prefix(self.workspace.root())
                .unwrap_or(&member.path);
            let prefix = format!("{}/", rel.display());
            if changed_files.iter().any(|f| f.starts_with(&prefix)) {
                directly_changed.insert(member.name.as_str());
            }
        }

        // Add transitive dependents using PackageGraph
        let mut affected: HashSet<&str> = directly_changed.clone();
        for &pkg in &directly_changed {
            if let Ok(dependents) = self.graph.transitive_dependents(pkg) {
                for dep in dependents {
                    affected.insert(dep);
                }
            }
        }

        Ok(self
            .workspace
            .members()
            .iter()
            .filter(|m| affected.contains(m.name.as_str()))
            .collect())
    }

    /// Check if a single member matches a selector.
    fn matches_selector(
        &self,
        member: &WorkspaceMember,
        selector: &FilterSelector,
    ) -> Result<bool, FilterError> {
        match selector {
            FilterSelector::Name(name) => Ok(member.name == *name),

            FilterSelector::NameGlob(pattern) => {
                let pat = glob::Pattern::new(pattern)
                    .map_err(|e| FilterError::InvalidGlob(e.to_string()))?;
                Ok(pat.matches(&member.name))
            }

            FilterSelector::PathGlob(pattern) => {
                let pat = glob::Pattern::new(pattern)
                    .map_err(|e| FilterError::InvalidGlob(e.to_string()))?;
                let rel = member
                    .path
                    .strip_prefix(self.workspace.root())
                    .unwrap_or(&member.path);
                let rel_str = rel.to_string_lossy();
                Ok(pat.matches(rel_str.as_ref()))
            }

            FilterSelector::Dependencies(name) => {
                if member.name == *name {
                    return Ok(true);
                }
                let deps = self
                    .graph
                    .transitive_dependencies(name)
                    .map_err(|_| FilterError::PackageNotFound(name.clone()))?;
                Ok(deps.contains(&member.name.as_str()))
            }

            FilterSelector::Dependents(name) => {
                if member.name == *name {
                    return Ok(true);
                }
                let deps = self
                    .graph
                    .transitive_dependents(name)
                    .map_err(|_| FilterError::PackageNotFound(name.clone()))?;
                Ok(deps.contains(&member.name.as_str()))
            }

            FilterSelector::All(name) => {
                if member.name == *name {
                    return Ok(true);
                }
                let deps = self
                    .graph
                    .transitive_dependencies(name)
                    .map_err(|_| FilterError::PackageNotFound(name.clone()))?;
                if deps.contains(&member.name.as_str()) {
                    return Ok(true);
                }
                let dependents = self
                    .graph
                    .transitive_dependents(name)
                    .map_err(|_| FilterError::PackageNotFound(name.clone()))?;
                Ok(dependents.contains(&member.name.as_str()))
            }

            FilterSelector::ChangedSince {
                path_filter,
                git_ref,
            } => {
                let changed = self.changed_since(path_filter.as_deref(), git_ref)?;
                Ok(changed.iter().any(|m| m.name == member.name))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LinkerMode, ParsedPackageJson, SecurityConfig, WorkspaceConfig};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn make_member(name: &str, path: &str, deps: Vec<(&str, &str)>) -> WorkspaceMember {
        let mut dependencies = HashMap::new();
        for (k, v) in deps {
            dependencies.insert(k.to_string(), v.to_string());
        }
        WorkspaceMember {
            name: name.to_string(),
            path: PathBuf::from(path),
            package_json: ParsedPackageJson {
                name: name.to_string(),
                version: Some("1.0.0".to_string()),
                dependencies,
                dev_dependencies: HashMap::new(),
                peer_dependencies: HashMap::new(),
                scripts: HashMap::new(),
            },
        }
    }

    fn make_engine(members: Vec<WorkspaceMember>) -> FilterEngine {
        let ws = Workspace::new(
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
        );
        let graph = PackageGraph::from_workspace(&ws);
        FilterEngine::new(ws, graph)
    }

    // --- parse tests ---

    #[test]
    fn test_parse_name() {
        let s = FilterEngine::parse_selector("@scope/pkg").unwrap();
        assert_eq!(s, FilterSelector::Name("@scope/pkg".to_string()));
    }

    #[test]
    fn test_parse_name_simple() {
        let s = FilterEngine::parse_selector("pkg-a").unwrap();
        assert_eq!(s, FilterSelector::Name("pkg-a".to_string()));
    }

    #[test]
    fn test_parse_name_glob() {
        let s = FilterEngine::parse_selector("@scope/*").unwrap();
        assert_eq!(s, FilterSelector::NameGlob("@scope/*".to_string()));
    }

    #[test]
    fn test_parse_name_glob_wildcard() {
        let s = FilterEngine::parse_selector("pkg-*").unwrap();
        assert_eq!(s, FilterSelector::NameGlob("pkg-*".to_string()));
    }

    #[test]
    fn test_parse_path_glob() {
        let s = FilterEngine::parse_selector("./apps/*").unwrap();
        assert_eq!(s, FilterSelector::PathGlob("./apps/*".to_string()));
    }

    #[test]
    fn test_parse_path_glob_absolute() {
        let s = FilterEngine::parse_selector("/apps/*").unwrap();
        assert_eq!(s, FilterSelector::PathGlob("/apps/*".to_string()));
    }

    #[test]
    fn test_parse_dependencies() {
        let s = FilterEngine::parse_selector("pkg-a...").unwrap();
        assert_eq!(s, FilterSelector::Dependencies("pkg-a".to_string()));
    }

    #[test]
    fn test_parse_dependents() {
        let s = FilterEngine::parse_selector("...pkg-a").unwrap();
        assert_eq!(s, FilterSelector::Dependents("pkg-a".to_string()));
    }

    #[test]
    fn test_parse_all() {
        let s = FilterEngine::parse_selector("...pkg-a...").unwrap();
        assert_eq!(s, FilterSelector::All("pkg-a".to_string()));
    }

    #[test]
    fn test_parse_changed_since() {
        let s = FilterEngine::parse_selector("[HEAD~1]").unwrap();
        assert_eq!(
            s,
            FilterSelector::ChangedSince {
                path_filter: None,
                git_ref: "HEAD~1".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_changed_since_with_path() {
        let s = FilterEngine::parse_selector("{apps/*}[main]").unwrap();
        assert_eq!(
            s,
            FilterSelector::ChangedSince {
                path_filter: Some("apps/*".to_string()),
                git_ref: "main".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_changed_since_with_path_empty() {
        let s = FilterEngine::parse_selector("{}[main]").unwrap();
        assert_eq!(
            s,
            FilterSelector::ChangedSince {
                path_filter: None,
                git_ref: "main".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_multiple_selectors() {
        let selectors =
            FilterEngine::parse_selectors(&["@scope/pkg".into(), "pkg-a...".into()]).unwrap();
        assert_eq!(selectors.len(), 2);
        assert_eq!(selectors[0], FilterSelector::Name("@scope/pkg".to_string()));
        assert_eq!(
            selectors[1],
            FilterSelector::Dependencies("pkg-a".to_string())
        );
    }

    // --- error cases ---

    #[test]
    fn test_parse_empty() {
        let err = FilterEngine::parse_selector("").unwrap_err();
        assert!(err.to_string().contains("invalid filter syntax"));
    }

    #[test]
    fn test_parse_invalid_changed_empty_ref() {
        let err = FilterEngine::parse_selector("[]").unwrap_err();
        assert!(err.to_string().contains("invalid filter syntax"));
    }

    #[test]
    fn test_parse_invalid_changed_missing_bracket() {
        let err = FilterEngine::parse_selector("[HEAD~1").unwrap_err();
        assert!(err.to_string().contains("invalid filter syntax"));
    }

    #[test]
    fn test_parse_dependencies_empty() {
        let err = FilterEngine::parse_selector("...").unwrap_err();
        assert!(err.to_string().contains("invalid filter syntax"));
    }

    // --- match tests ---

    #[test]
    fn test_match_name_exact() {
        let engine = make_engine(vec![
            make_member("pkg-a", "packages/pkg-a", vec![]),
            make_member("pkg-b", "packages/pkg-b", vec![]),
        ]);
        let members = engine
            .apply(&[FilterSelector::Name("pkg-a".to_string())])
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "pkg-a");
    }

    #[test]
    fn test_match_name_not_found() {
        let engine = make_engine(vec![make_member("pkg-a", "packages/pkg-a", vec![])]);
        let members = engine
            .apply(&[FilterSelector::Name("nonexistent".to_string())])
            .unwrap();
        assert!(members.is_empty());
    }

    #[test]
    fn test_match_name_glob() {
        let engine = make_engine(vec![
            make_member("pkg-a", "packages/pkg-a", vec![]),
            make_member("pkg-b", "packages/pkg-b", vec![]),
            make_member("other", "packages/other", vec![]),
        ]);
        let members = engine
            .apply(&[FilterSelector::NameGlob("pkg-*".to_string())])
            .unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.iter().any(|m| m.name == "pkg-a"));
        assert!(members.iter().any(|m| m.name == "pkg-b"));
    }

    #[test]
    fn test_match_path_glob() {
        let engine = make_engine(vec![
            make_member("pkg-a", "packages/pkg-a", vec![]),
            make_member("pkg-b", "packages/pkg-b", vec![]),
            make_member("other", "libs/other", vec![]),
        ]);
        let members = engine
            .apply(&[FilterSelector::PathGlob("packages/*".to_string())])
            .unwrap();
        assert_eq!(members.len(), 2);
    }

    #[test]
    fn test_match_dependencies_with_graph() {
        let engine = make_engine(vec![
            make_member("a", "packages/a", vec![("b", "workspace:*")]),
            make_member("b", "packages/b", vec![("c", "workspace:*")]),
            make_member("c", "packages/c", vec![]),
        ]);
        let members = engine
            .apply(&[FilterSelector::Dependencies("a".to_string())])
            .unwrap();
        let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(members.len(), 3);
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
        assert!(names.contains(&"c"));
    }

    #[test]
    fn test_match_dependents_with_graph() {
        let engine = make_engine(vec![
            make_member("a", "packages/a", vec![("b", "workspace:*")]),
            make_member("b", "packages/b", vec![("c", "workspace:*")]),
            make_member("c", "packages/c", vec![]),
        ]);
        let members = engine
            .apply(&[FilterSelector::Dependents("c".to_string())])
            .unwrap();
        let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(members.len(), 3);
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
        assert!(names.contains(&"c"));
    }

    #[test]
    fn test_match_all_with_graph() {
        let engine = make_engine(vec![
            make_member("a", "packages/a", vec![("b", "workspace:*")]),
            make_member("b", "packages/b", vec![("c", "workspace:*")]),
            make_member("c", "packages/c", vec![]),
        ]);
        let members = engine
            .apply(&[FilterSelector::All("b".to_string())])
            .unwrap();
        let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
        // All gives b + its deps (c) + its dependents (a)
        assert_eq!(members.len(), 3);
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
        assert!(names.contains(&"c"));
    }

    // --- multi-filter union ---

    #[test]
    fn test_multi_filter_union() {
        let engine = make_engine(vec![
            make_member("pkg-a", "packages/pkg-a", vec![]),
            make_member("pkg-b", "packages/pkg-b", vec![]),
            make_member("other", "packages/other", vec![]),
        ]);
        let members = engine
            .apply(&[
                FilterSelector::Name("pkg-a".to_string()),
                FilterSelector::Name("other".to_string()),
            ])
            .unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.iter().any(|m| m.name == "pkg-a"));
        assert!(members.iter().any(|m| m.name == "other"));
    }

    #[test]
    fn test_multi_filter_dedup() {
        let engine = make_engine(vec![
            make_member("pkg-a", "packages/pkg-a", vec![]),
            make_member("pkg-b", "packages/pkg-b", vec![]),
        ]);
        let members = engine
            .apply(&[
                FilterSelector::Name("pkg-a".to_string()),
                FilterSelector::Name("pkg-a".to_string()),
            ])
            .unwrap();
        assert_eq!(members.len(), 1);
    }

    // --- empty workspace ---

    #[test]
    fn test_empty_workspace() {
        let engine = make_engine(vec![]);
        let members = engine
            .apply(&[FilterSelector::Name("anything".to_string())])
            .unwrap();
        assert!(members.is_empty());
    }

    #[test]
    fn test_no_selectors_returns_all() {
        let engine = make_engine(vec![
            make_member("a", "packages/a", vec![]),
            make_member("b", "packages/b", vec![]),
        ]);
        let members = engine.apply(&[]).unwrap();
        assert_eq!(members.len(), 2);
    }

    // --- package not found errors ---

    #[test]
    fn test_package_not_found_dependencies() {
        let engine = make_engine(vec![make_member("a", "packages/a", vec![])]);
        let err = engine.apply(&[FilterSelector::Dependencies("nonexistent".to_string())]);
        assert!(err.is_err());
        match err {
            Err(FilterError::PackageNotFound(name)) => assert_eq!(name, "nonexistent"),
            _ => panic!("expected PackageNotFound"),
        }
    }

    #[test]
    fn test_package_not_found_dependents() {
        let engine = make_engine(vec![make_member("a", "packages/a", vec![])]);
        let err = engine.apply(&[FilterSelector::Dependents("nonexistent".to_string())]);
        assert!(err.is_err());
    }

    #[test]
    fn test_invalid_glob_pattern() {
        let engine = make_engine(vec![make_member("a", "packages/a", vec![])]);
        let err = engine.apply(&[FilterSelector::NameGlob("[invalid".to_string())]);
        assert!(err.is_err());
        match err {
            Err(FilterError::InvalidGlob(_)) => {}
            _ => panic!("expected InvalidGlob"),
        }
    }
}
