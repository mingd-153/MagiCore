//! Workspace protocol support for monorepo package management.
//!
//! The workspace protocol allows workspace members to reference each other
//! using the `workspace:` prefix in `package.json` dependency specifiers.
//! This follows the [pnpm workspace protocol](https://pnpm.io/workspaces#workspace-protocol)
//! convention.
//!
//! # Supported syntax
//!
//! | Syntax | Meaning |
//! |--------|---------|
//! | `workspace:*` | Always use the local workspace package |
//! | `workspace:^1.0.0` | Keep semver range, replace with actual version on publish |
//! | `workspace:~1.0.0` | Tilde range, replace with actual version on publish |
//! | `workspace:1.0.0` | Exact version, replace on publish |
//! | `workspace:./packages/shared` | Relative path reference |
//! | `workspace:shared` | Named path (lookup by directory name) |
//!
//! # Resolution
//!
//! During development, `workspace:*` always resolves to the local workspace
//! package. During publish, specifiers are replaced with actual versions.
//!
//! # Example
//!
//! ```rust,ignore
//! let protocol = WorkspaceProtocol::new(workspace, graph);
//! let spec = WorkspaceProtocol::parse("workspace:*")?;
//! let path = protocol.resolve("@myorg/shared", &spec)?;
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{PackageGraph, Workspace, WorkspaceMember};

/// Parsed workspace protocol specifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSpecifier {
    /// `workspace:*` — always use local package
    Any,

    /// `workspace:^1.0.0` / `workspace:~1.0.0` — semver range kept for publish
    SemverRange(String),

    /// `workspace:./packages/shared` — relative path from workspace root
    RelativePath(PathBuf),

    /// `workspace:shared` — named path (lookup by directory name)
    NamedPath(String),
}

impl WorkspaceSpecifier {
    /// Create a publish-ready version specifier by replacing the workspace
    /// protocol with the actual package version.
    ///
    /// - `*` → `actual_version`
    /// - `^1.0.0` → `^actual_version`
    /// - `./path` → `actual_version`
    /// - `path` → `actual_version`
    pub fn to_publish_specifier(&self, actual_version: &str) -> String {
        match self {
            WorkspaceSpecifier::Any => actual_version.to_string(),
            WorkspaceSpecifier::SemverRange(range) => {
                let prefix: String = range.chars().take_while(|c| !c.is_ascii_digit()).collect();
                format!("{}{}", prefix, actual_version)
            }
            WorkspaceSpecifier::RelativePath(_) => actual_version.to_string(),
            WorkspaceSpecifier::NamedPath(_) => actual_version.to_string(),
        }
    }

    /// Return the inner specifier string (without the `workspace:` prefix).
    pub fn as_specifier(&self) -> &str {
        match self {
            WorkspaceSpecifier::Any => "*",
            WorkspaceSpecifier::SemverRange(r) => r.as_str(),
            WorkspaceSpecifier::RelativePath(p) => p.to_str().unwrap_or(""),
            WorkspaceSpecifier::NamedPath(p) => p.as_str(),
        }
    }
}

/// Errors that can occur during workspace protocol operations.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("not a workspace protocol specifier: {0}")]
    NotWorkspaceProtocol(String),

    #[error("invalid workspace protocol syntax: {0}")]
    InvalidSyntax(String),

    #[error("workspace package '{0}' not found")]
    PackageNotFound(String),

    #[error("relative path '{0}' does not point to a workspace member")]
    InvalidRelativePath(String),

    #[error("circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("package '{0}' has no version field")]
    NoVersion(String),

    #[error("{0}")]
    Internal(String),
}

/// Workspace protocol handler.
///
/// Manages resolution, validation, and publish-ready replacement of
/// `workspace:` protocol specifiers in `package.json` dependencies.
#[derive(Debug, Clone)]
pub struct WorkspaceProtocol {
    workspace: Workspace,
    graph: PackageGraph,
}

impl WorkspaceProtocol {
    /// Create a new workspace protocol handler.
    pub fn new(workspace: Workspace, graph: PackageGraph) -> Self {
        Self { workspace, graph }
    }

    /// Check if a version specifier uses the workspace protocol.
    pub fn is_workspace_protocol(specifier: &str) -> bool {
        specifier.trim().starts_with("workspace:")
    }

    /// Parse a workspace protocol specifier into a [`WorkspaceSpecifier`].
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::NotWorkspaceProtocol`] if the specifier does
    /// not start with `workspace:`.
    /// Returns [`ProtocolError::InvalidSyntax`] if the specifier after
    /// `workspace:` is empty.
    pub fn parse(specifier: &str) -> Result<WorkspaceSpecifier, ProtocolError> {
        let specifier = specifier.trim();
        let inner = specifier
            .strip_prefix("workspace:")
            .ok_or_else(|| ProtocolError::NotWorkspaceProtocol(specifier.to_string()))?
            .trim();

        if inner.is_empty() {
            return Err(ProtocolError::InvalidSyntax(specifier.to_string()));
        }

        match inner {
            "*" => Ok(WorkspaceSpecifier::Any),
            inner if inner.starts_with("./") || inner.starts_with("../") => {
                Ok(WorkspaceSpecifier::RelativePath(PathBuf::from(inner)))
            }
            inner if inner.starts_with('/') => {
                let trimmed = inner.trim_start_matches('/');
                Ok(WorkspaceSpecifier::RelativePath(PathBuf::from(trimmed)))
            }
            inner
                if inner.starts_with('^')
                    || inner.starts_with('~')
                    || inner.chars().all(|c| {
                        c.is_ascii_digit()
                            || c == '.'
                            || c == 'x'
                            || c == 'X'
                            || c == '*'
                            || c == '-'
                    }) =>
            {
                Ok(WorkspaceSpecifier::SemverRange(inner.to_string()))
            }
            inner if inner.contains('/') || inner.contains('\\') => {
                Ok(WorkspaceSpecifier::RelativePath(PathBuf::from(inner)))
            }
            inner => Ok(WorkspaceSpecifier::NamedPath(inner.to_string())),
        }
    }

    /// Resolve a workspace dependency to a local member path.
    ///
    /// - `Any` / `SemverRange`: find the member by dependency name
    /// - `RelativePath`: find the member at the given path (relative to
    ///   workspace root)
    /// - `NamedPath`: find the member by name first, fall back to path lookup
    pub fn resolve(
        &self,
        package_name: &str,
        specifier: &WorkspaceSpecifier,
    ) -> Result<PathBuf, ProtocolError> {
        match specifier {
            WorkspaceSpecifier::Any | WorkspaceSpecifier::SemverRange(_) => {
                let member = self.find_member_by_name(package_name)?;
                Ok(member.path.clone())
            }
            WorkspaceSpecifier::RelativePath(path) => {
                let abs_path = if path.is_relative() {
                    self.workspace.root().join(path)
                } else {
                    path.clone()
                };
                let member = self.find_member_by_path(&abs_path)?;
                Ok(member.path.clone())
            }
            WorkspaceSpecifier::NamedPath(path) => {
                if let Some(member) = self.workspace.find_member(package_name) {
                    return Ok(member.path.clone());
                }
                let abs_path = if Path::new(path).is_relative() {
                    self.workspace.root().join(path)
                } else {
                    PathBuf::from(path)
                };
                let member = self.find_member_by_path(&abs_path)?;
                Ok(member.path.clone())
            }
        }
    }

    /// Replace workspace protocol specifiers with actual versions for publish.
    ///
    /// Modifies the `dependencies` map in place, replacing each `workspace:*`
    /// or similar specifier with the actual version of the target package.
    pub fn for_publish(
        &self,
        dependencies: &mut HashMap<String, String>,
        member: &WorkspaceMember,
    ) -> Result<(), ProtocolError> {
        let mut replacements: Vec<(String, String)> = Vec::new();

        for (dep_name, specifier) in dependencies.iter() {
            if !Self::is_workspace_protocol(specifier) {
                continue;
            }

            // Skip self-referencing workspace protocol deps
            if dep_name == &member.name {
                continue;
            }

            let parsed = Self::parse(specifier)?;
            let target_version = match &parsed {
                WorkspaceSpecifier::Any | WorkspaceSpecifier::SemverRange(_) => {
                    self.actual_version(dep_name)?.to_string()
                }
                WorkspaceSpecifier::RelativePath(path) => {
                    let abs_path = if path.is_relative() {
                        self.workspace.root().join(path)
                    } else {
                        path.clone()
                    };
                    let member = self.find_member_by_path(&abs_path)?;
                    self.version_or_default(member)
                }
                WorkspaceSpecifier::NamedPath(path) => {
                    if let Ok(v) = self.actual_version(dep_name) {
                        v.to_string()
                    } else {
                        let abs_path = if Path::new(path).is_relative() {
                            self.workspace.root().join(path)
                        } else {
                            PathBuf::from(path)
                        };
                        let member = self.find_member_by_path(&abs_path)?;
                        self.version_or_default(member)
                    }
                }
            };

            let new_specifier = parsed.to_publish_specifier(&target_version);
            replacements.push((dep_name.clone(), new_specifier));
        }

        for (name, new_spec) in replacements {
            dependencies.insert(name, new_spec);
        }

        Ok(())
    }

    /// Validate workspace dependencies for a member.
    ///
    /// Checks that:
    /// - All workspace protocol targets exist in the workspace
    /// - No cyclic dependencies are introduced
    /// - Path references point to valid workspace members
    /// - No self-referencing workspace protocol dependencies
    pub fn validate(&self, member: &WorkspaceMember) -> Result<(), Vec<ProtocolError>> {
        let mut errors = Vec::new();

        let all_deps = member
            .package_json
            .dependencies
            .iter()
            .chain(member.package_json.dev_dependencies.iter())
            .chain(member.package_json.peer_dependencies.iter());

        for (dep_name, specifier) in all_deps {
            if !Self::is_workspace_protocol(specifier) {
                continue;
            }

            // Self-dependency check
            if dep_name == &member.name {
                errors.push(ProtocolError::CircularDependency(member.name.clone()));
                continue;
            }

            let parsed = match Self::parse(specifier) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };

            match &parsed {
                WorkspaceSpecifier::Any | WorkspaceSpecifier::SemverRange(_) => {
                    if self.workspace.find_member(dep_name).is_none() {
                        errors.push(ProtocolError::PackageNotFound(dep_name.clone()));
                    }
                }
                WorkspaceSpecifier::RelativePath(path) => {
                    let abs_path = if path.is_relative() {
                        self.workspace.root().join(path)
                    } else {
                        path.clone()
                    };
                    if self.find_member_by_path(&abs_path).is_err() {
                        errors.push(ProtocolError::InvalidRelativePath(
                            path.display().to_string(),
                        ));
                    }
                }
                WorkspaceSpecifier::NamedPath(path) => {
                    if self.workspace.find_member(dep_name).is_none() {
                        let abs_path = if Path::new(path).is_relative() {
                            self.workspace.root().join(path)
                        } else {
                            PathBuf::from(path)
                        };
                        if self.find_member_by_path(&abs_path).is_err() {
                            errors.push(ProtocolError::PackageNotFound(dep_name.clone()));
                        }
                    }
                }
            }
        }

        let has_cycle = self
            .graph
            .detect_cycles()
            .iter()
            .any(|cycle| cycle.contains(&member.name));
        if has_cycle {
            errors.push(ProtocolError::CircularDependency(member.name.clone()));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Get the actual version string for a workspace package.
    pub fn actual_version(&self, package_name: &str) -> Result<&str, ProtocolError> {
        let member = self
            .workspace
            .find_member(package_name)
            .ok_or_else(|| ProtocolError::PackageNotFound(package_name.to_string()))?;
        member
            .package_json
            .version
            .as_deref()
            .ok_or_else(|| ProtocolError::NoVersion(package_name.to_string()))
    }

    /// Replace workspace protocol specifier to a publish-ready specifier.
    pub fn resolve_for_publish(
        &self,
        specifier: &WorkspaceSpecifier,
        target_version: &str,
    ) -> Result<String, ProtocolError> {
        Ok(specifier.to_publish_specifier(target_version))
    }

    fn find_member_by_name(&self, name: &str) -> Result<&WorkspaceMember, ProtocolError> {
        self.workspace
            .find_member(name)
            .ok_or_else(|| ProtocolError::PackageNotFound(name.to_string()))
    }

    fn find_member_by_path(&self, path: &Path) -> Result<&WorkspaceMember, ProtocolError> {
        let root = self.workspace.root();

        let normalized_path = normalize_path(path);

        // Defense-in-depth: reject paths that escape workspace root
        if !normalized_path.starts_with(root) {
            // If path is relative, it might be a member path directly
            let rel_path = root.join(&normalized_path);
            let rel_normalized = normalize_path(&rel_path);
            if !rel_normalized.starts_with(root) {
                return Err(ProtocolError::InvalidRelativePath(format!(
                    "path '{}' escapes workspace root '{}'",
                    path.display(),
                    root.display()
                )));
            }
            let normalized = normalize_path(&rel_normalized);
            let rel = normalized.strip_prefix(root).unwrap_or(&normalized);
            for member in self.workspace.members() {
                let member_rel = member.path.strip_prefix(root).unwrap_or(&member.path);
                if member_rel == rel {
                    return Ok(member);
                }
            }
        }

        let rel_path = normalized_path
            .strip_prefix(root)
            .unwrap_or(&normalized_path);

        for member in self.workspace.members() {
            let member_rel = member.path.strip_prefix(root).unwrap_or(&member.path);
            if member_rel == rel_path || member.path == *path {
                return Ok(member);
            }
        }

        // TOCTOU-safe canonical comparison: call canonicalize once, no prior exists() check
        if let Ok(canonical) = normalized_path.canonicalize() {
            for member in self.workspace.members() {
                let member_path = if member.path.is_relative() {
                    root.join(&member.path)
                } else {
                    member.path.clone()
                };
                if let Ok(m_canon) = member_path.canonicalize() {
                    if m_canon == canonical {
                        return Ok(member);
                    }
                }
            }
        }

        Err(ProtocolError::InvalidRelativePath(
            path.display().to_string(),
        ))
    }

    fn version_or_default(&self, member: &WorkspaceMember) -> String {
        member
            .package_json
            .version
            .clone()
            .unwrap_or_else(|| "0.0.0".to_string())
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LinkerMode, ParsedPackageJson, SecurityConfig, WorkspaceConfig};
    use std::collections::HashMap;

    fn make_member(
        name: &str,
        path: &str,
        version: &str,
        deps: Vec<(&str, &str)>,
    ) -> WorkspaceMember {
        let mut dependencies = HashMap::new();
        for (k, v) in deps {
            dependencies.insert(k.to_string(), v.to_string());
        }
        WorkspaceMember {
            name: name.to_string(),
            path: PathBuf::from(path),
            package_json: ParsedPackageJson {
                name: name.to_string(),
                version: Some(version.to_string()),
                dependencies,
                dev_dependencies: HashMap::new(),
                peer_dependencies: HashMap::new(),
                scripts: HashMap::new(),
            },
        }
    }

    fn make_workspace_with_members(
        root: &str,
        members: Vec<WorkspaceMember>,
    ) -> (Workspace, PackageGraph) {
        let ws = Workspace::new(
            PathBuf::from(root),
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
        (ws, graph)
    }

    fn make_protocol(members: Vec<WorkspaceMember>) -> WorkspaceProtocol {
        let (ws, graph) = make_workspace_with_members("/test/root", members);
        WorkspaceProtocol::new(ws, graph)
    }

    // --- parse tests ---

    #[test]
    fn test_parse_any() {
        let spec = WorkspaceProtocol::parse("workspace:*").unwrap();
        assert_eq!(spec, WorkspaceSpecifier::Any);
    }

    #[test]
    fn test_parse_semver_caret() {
        let spec = WorkspaceProtocol::parse("workspace:^1.2.3").unwrap();
        assert_eq!(spec, WorkspaceSpecifier::SemverRange("^1.2.3".to_string()));
    }

    #[test]
    fn test_parse_semver_tilde() {
        let spec = WorkspaceProtocol::parse("workspace:~1.2.3").unwrap();
        assert_eq!(spec, WorkspaceSpecifier::SemverRange("~1.2.3".to_string()));
    }

    #[test]
    fn test_parse_semver_exact() {
        let spec = WorkspaceProtocol::parse("workspace:1.2.3").unwrap();
        assert_eq!(spec, WorkspaceSpecifier::SemverRange("1.2.3".to_string()));
    }

    #[test]
    fn test_parse_relative_path_dot_slash() {
        let spec = WorkspaceProtocol::parse("workspace:./packages/shared").unwrap();
        assert_eq!(
            spec,
            WorkspaceSpecifier::RelativePath(PathBuf::from("./packages/shared"))
        );
    }

    #[test]
    fn test_parse_relative_path_dot_dot() {
        let spec = WorkspaceProtocol::parse("workspace:../other").unwrap();
        assert_eq!(
            spec,
            WorkspaceSpecifier::RelativePath(PathBuf::from("../other"))
        );
    }

    #[test]
    fn test_parse_relative_path_absolute() {
        let spec = WorkspaceProtocol::parse("workspace:/apps/web").unwrap();
        assert_eq!(
            spec,
            WorkspaceSpecifier::RelativePath(PathBuf::from("apps/web"))
        );
    }

    #[test]
    fn test_parse_named_path() {
        let spec = WorkspaceProtocol::parse("workspace:shared").unwrap();
        assert_eq!(spec, WorkspaceSpecifier::NamedPath("shared".to_string()));
    }

    #[test]
    fn test_parse_named_path_with_dash() {
        let spec = WorkspaceProtocol::parse("workspace:my-pkg").unwrap();
        assert_eq!(spec, WorkspaceSpecifier::NamedPath("my-pkg".to_string()));
    }

    #[test]
    fn test_parse_invalid_empty() {
        let err = WorkspaceProtocol::parse("workspace:").unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidSyntax(_)));
    }

    #[test]
    fn test_parse_not_workspace_protocol() {
        let err = WorkspaceProtocol::parse("^1.0.0").unwrap_err();
        assert!(matches!(err, ProtocolError::NotWorkspaceProtocol(_)));
    }

    #[test]
    fn test_parse_latest() {
        let err = WorkspaceProtocol::parse("latest").unwrap_err();
        assert!(matches!(err, ProtocolError::NotWorkspaceProtocol(_)));
    }

    // --- to_publish_specifier tests ---

    #[test]
    fn test_to_publish_specifier_any() {
        let spec = WorkspaceSpecifier::Any;
        assert_eq!(spec.to_publish_specifier("1.5.0"), "1.5.0");
    }

    #[test]
    fn test_to_publish_specifier_caret() {
        let spec = WorkspaceSpecifier::SemverRange("^1.0.0".to_string());
        assert_eq!(spec.to_publish_specifier("1.5.0"), "^1.5.0");
    }

    #[test]
    fn test_to_publish_specifier_tilde() {
        let spec = WorkspaceSpecifier::SemverRange("~2.0.0".to_string());
        assert_eq!(spec.to_publish_specifier("2.1.0"), "~2.1.0");
    }

    #[test]
    fn test_to_publish_specifier_exact() {
        let spec = WorkspaceSpecifier::SemverRange("3.0.0".to_string());
        assert_eq!(spec.to_publish_specifier("3.0.0"), "3.0.0");
    }

    // --- is_workspace_protocol tests ---

    #[test]
    fn test_is_workspace_protocol_true() {
        assert!(WorkspaceProtocol::is_workspace_protocol("workspace:*"));
        assert!(WorkspaceProtocol::is_workspace_protocol("workspace:^1.0.0"));
        assert!(WorkspaceProtocol::is_workspace_protocol("workspace:./path"));
    }

    #[test]
    fn test_is_workspace_protocol_false() {
        assert!(!WorkspaceProtocol::is_workspace_protocol("^1.0.0"));
        assert!(!WorkspaceProtocol::is_workspace_protocol("latest"));
        assert!(!WorkspaceProtocol::is_workspace_protocol("*"));
    }

    // --- resolve tests ---

    #[test]
    fn test_resolve_any_by_name() {
        let protocol = make_protocol(vec![
            make_member("@myorg/shared", "packages/shared", "1.5.0", vec![]),
            make_member(
                "apps/web",
                "apps/web",
                "1.0.0",
                vec![("@myorg/shared", "workspace:*")],
            ),
        ]);
        let path = protocol
            .resolve("@myorg/shared", &WorkspaceSpecifier::Any)
            .unwrap();
        assert_eq!(path, PathBuf::from("packages/shared"));
    }

    #[test]
    fn test_resolve_semver_by_name() {
        let protocol = make_protocol(vec![
            make_member("@myorg/shared", "packages/shared", "1.5.0", vec![]),
            make_member(
                "apps/web",
                "apps/web",
                "1.0.0",
                vec![("@myorg/shared", "workspace:^1.0.0")],
            ),
        ]);
        let path = protocol
            .resolve(
                "@myorg/shared",
                &WorkspaceSpecifier::SemverRange("^1.0.0".to_string()),
            )
            .unwrap();
        assert_eq!(path, PathBuf::from("packages/shared"));
    }

    #[test]
    fn test_resolve_relative_path() {
        let protocol = make_protocol(vec![make_member(
            "@myorg/shared",
            "packages/shared",
            "1.5.0",
            vec![],
        )]);
        let path = protocol
            .resolve(
                "@myorg/shared",
                &WorkspaceSpecifier::RelativePath(PathBuf::from("./packages/shared")),
            )
            .unwrap();
        assert_eq!(path, PathBuf::from("packages/shared"));
    }

    #[test]
    fn test_resolve_named_path_found_by_name() {
        let protocol = make_protocol(vec![make_member(
            "@myorg/shared",
            "packages/shared",
            "1.5.0",
            vec![],
        )]);
        let path = protocol
            .resolve(
                "@myorg/shared",
                &WorkspaceSpecifier::NamedPath("shared".to_string()),
            )
            .unwrap();
        assert_eq!(path, PathBuf::from("packages/shared"));
    }

    #[test]
    fn test_resolve_package_not_found() {
        let protocol = make_protocol(vec![make_member("apps/web", "apps/web", "1.0.0", vec![])]);
        let err = protocol
            .resolve("@myorg/shared", &WorkspaceSpecifier::Any)
            .unwrap_err();
        assert!(matches!(err, ProtocolError::PackageNotFound(_)));
    }

    #[test]
    fn test_resolve_relative_path_not_found() {
        let protocol = make_protocol(vec![make_member("apps/web", "apps/web", "1.0.0", vec![])]);
        let err = protocol
            .resolve(
                "apps/web",
                &WorkspaceSpecifier::RelativePath(PathBuf::from("./nonexistent")),
            )
            .unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidRelativePath(_)));
    }

    // --- actual_version tests ---

    #[test]
    fn test_actual_version_exists() {
        let protocol = make_protocol(vec![make_member(
            "@myorg/shared",
            "packages/shared",
            "1.5.0",
            vec![],
        )]);
        let ver = protocol.actual_version("@myorg/shared").unwrap();
        assert_eq!(ver, "1.5.0");
    }

    #[test]
    fn test_actual_version_not_found() {
        let protocol = make_protocol(vec![]);
        let err = protocol.actual_version("@myorg/shared").unwrap_err();
        assert!(matches!(err, ProtocolError::PackageNotFound(_)));
    }

    // --- for_publish tests ---

    #[test]
    fn test_for_publish_any() {
        let protocol = make_protocol(vec![
            make_member("@myorg/shared", "packages/shared", "1.5.0", vec![]),
            make_member(
                "apps/web",
                "apps/web",
                "1.0.0",
                vec![("@myorg/shared", "workspace:*")],
            ),
        ]);
        let web = protocol.workspace.find_member("apps/web").unwrap();
        let mut deps = web.package_json.dependencies.clone();

        protocol.for_publish(&mut deps, web).unwrap();

        assert_eq!(deps.get("@myorg/shared").unwrap(), "1.5.0");
    }

    #[test]
    fn test_for_publish_semver_caret() {
        let protocol = make_protocol(vec![
            make_member("@myorg/shared", "packages/shared", "1.5.0", vec![]),
            make_member(
                "apps/web",
                "apps/web",
                "1.0.0",
                vec![("@myorg/shared", "workspace:^1.0.0")],
            ),
        ]);
        let web = protocol.workspace.find_member("apps/web").unwrap();
        let mut deps = web.package_json.dependencies.clone();

        protocol.for_publish(&mut deps, web).unwrap();

        assert_eq!(deps.get("@myorg/shared").unwrap(), "^1.5.0");
    }

    #[test]
    fn test_for_publish_semver_tilde() {
        let protocol = make_protocol(vec![
            make_member("@myorg/shared", "packages/shared", "1.5.0", vec![]),
            make_member(
                "apps/web",
                "apps/web",
                "1.0.0",
                vec![("@myorg/shared", "workspace:~1.0.0")],
            ),
        ]);
        let web = protocol.workspace.find_member("apps/web").unwrap();
        let mut deps = web.package_json.dependencies.clone();

        protocol.for_publish(&mut deps, web).unwrap();

        assert_eq!(deps.get("@myorg/shared").unwrap(), "~1.5.0");
    }

    #[test]
    fn test_for_publish_ignores_non_workspace() {
        let protocol = make_protocol(vec![make_member(
            "apps/web",
            "apps/web",
            "1.0.0",
            vec![("lodash", "^4.0.0")],
        )]);
        let web = protocol.workspace.find_member("apps/web").unwrap();
        let mut deps = web.package_json.dependencies.clone();

        protocol.for_publish(&mut deps, web).unwrap();

        assert_eq!(deps.get("lodash").unwrap(), "^4.0.0");
    }

    #[test]
    fn test_for_publish_empty_deps() {
        let protocol = make_protocol(vec![make_member("apps/web", "apps/web", "1.0.0", vec![])]);
        let web = protocol.workspace.find_member("apps/web").unwrap();
        let mut deps: HashMap<String, String> = HashMap::new();

        protocol.for_publish(&mut deps, web).unwrap();

        assert!(deps.is_empty());
    }

    #[test]
    fn test_for_publish_relative_path() {
        let protocol = make_protocol(vec![
            make_member("@myorg/shared", "packages/shared", "2.0.0", vec![]),
            make_member(
                "apps/web",
                "apps/web",
                "1.0.0",
                vec![("@myorg/shared", "workspace:./packages/shared")],
            ),
        ]);
        let web = protocol.workspace.find_member("apps/web").unwrap();
        let mut deps = web.package_json.dependencies.clone();

        protocol.for_publish(&mut deps, web).unwrap();

        assert_eq!(deps.get("@myorg/shared").unwrap(), "2.0.0");
    }

    // --- validate tests ---

    #[test]
    fn test_validate_valid() {
        let protocol = make_protocol(vec![
            make_member("@myorg/shared", "packages/shared", "1.5.0", vec![]),
            make_member(
                "apps/web",
                "apps/web",
                "1.0.0",
                vec![("@myorg/shared", "workspace:*")],
            ),
        ]);
        let web = protocol.workspace.find_member("apps/web").unwrap();
        let result = protocol.validate(web);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_package_not_found() {
        let protocol = make_protocol(vec![make_member(
            "apps/web",
            "apps/web",
            "1.0.0",
            vec![("@myorg/nonexistent", "workspace:*")],
        )]);
        let web = protocol.workspace.find_member("apps/web").unwrap();
        let result = protocol.validate(web);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ProtocolError::PackageNotFound(_))));
    }

    #[test]
    fn test_validate_skips_non_workspace_deps() {
        let protocol = make_protocol(vec![make_member(
            "apps/web",
            "apps/web",
            "1.0.0",
            vec![("lodash", "^4.0.0")],
        )]);
        let web = protocol.workspace.find_member("apps/web").unwrap();
        let result = protocol.validate(web);
        assert!(result.is_ok());
    }

    // --- as_specifier tests ---

    #[test]
    fn test_as_specifier_any() {
        assert_eq!(WorkspaceSpecifier::Any.as_specifier(), "*");
    }

    #[test]
    fn test_as_specifier_semver() {
        let spec = WorkspaceSpecifier::SemverRange("^1.0.0".to_string());
        assert_eq!(spec.as_specifier(), "^1.0.0");
    }

    #[test]
    fn test_as_specifier_relative_path() {
        let spec = WorkspaceSpecifier::RelativePath(PathBuf::from("./packages/shared"));
        assert_eq!(spec.as_specifier(), "./packages/shared");
    }

    #[test]
    fn test_as_specifier_named_path() {
        let spec = WorkspaceSpecifier::NamedPath("shared".to_string());
        assert_eq!(spec.as_specifier(), "shared");
    }

    // --- version_or_default edge cases ---

    #[test]
    fn test_version_or_default_missing_version() {
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
            vec![WorkspaceMember {
                name: "no-version".to_string(),
                path: PathBuf::from("packages/no-version"),
                package_json: ParsedPackageJson {
                    name: "no-version".to_string(),
                    version: None,
                    dependencies: HashMap::new(),
                    dev_dependencies: HashMap::new(),
                    peer_dependencies: HashMap::new(),
                    scripts: HashMap::new(),
                },
            }],
        );
        let graph = PackageGraph::from_workspace(&ws);
        let protocol = WorkspaceProtocol::new(ws, graph);

        let member = protocol.workspace.find_member("no-version").unwrap();
        let err = protocol.actual_version("no-version").unwrap_err();
        assert!(matches!(err, ProtocolError::NoVersion(_)));

        let ver = protocol.version_or_default(member);
        assert_eq!(ver, "0.0.0");
    }

    // --- resolve_for_publish ---

    #[test]
    fn test_resolve_for_publish_any() {
        let protocol = make_protocol(vec![make_member("pkg", "packages/pkg", "1.0.0", vec![])]);
        let result = protocol
            .resolve_for_publish(&WorkspaceSpecifier::Any, "2.0.0")
            .unwrap();
        assert_eq!(result, "2.0.0");
    }

    #[test]
    fn test_resolve_for_publish_caret() {
        let protocol = make_protocol(vec![make_member("pkg", "packages/pkg", "1.0.0", vec![])]);
        let result = protocol
            .resolve_for_publish(
                &WorkspaceSpecifier::SemverRange("^1.0.0".to_string()),
                "2.0.0",
            )
            .unwrap();
        assert_eq!(result, "^2.0.0");
    }

    // --- security edge cases ---

    #[test]
    fn test_parse_whitespace_after_colon() {
        let spec = WorkspaceProtocol::parse("workspace: *").unwrap();
        assert_eq!(spec, WorkspaceSpecifier::Any);
    }

    #[test]
    fn test_parse_whitespace_around_inner() {
        let spec = WorkspaceProtocol::parse("workspace:  ^1.0.0  ").unwrap();
        assert_eq!(spec, WorkspaceSpecifier::SemverRange("^1.0.0".to_string()));
    }

    #[test]
    fn test_validate_self_dependency() {
        let protocol = make_protocol(vec![make_member(
            "pkg-a",
            "packages/pkg-a",
            "1.0.0",
            vec![("pkg-a", "workspace:*")],
        )]);
        let member = protocol.workspace.find_member("pkg-a").unwrap();
        let result = protocol.validate(member);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ProtocolError::CircularDependency(_))));
    }

    #[test]
    fn test_for_publish_self_dependency_skipped() {
        let protocol = make_protocol(vec![make_member(
            "pkg-a",
            "packages/pkg-a",
            "1.0.0",
            vec![("pkg-a", "workspace:*")],
        )]);
        let member = protocol.workspace.find_member("pkg-a").unwrap();
        let mut deps = member.package_json.dependencies.clone();
        assert!(deps.contains_key("pkg-a"));

        protocol.for_publish(&mut deps, member).unwrap();
        // Self-dependency is skipped, so the key should remain unchanged
        // Actually... self-dependency with workspace:* should be skipped
        // The key itself would remain since we skip it in for_publish
        assert!(deps.contains_key("pkg-a"));
        assert_eq!(deps.get("pkg-a").unwrap(), "workspace:*");
    }

    #[test]
    fn test_validate_dev_dependencies() {
        let mut member = make_member("apps/web", "apps/web", "1.0.0", vec![]);
        member
            .package_json
            .dev_dependencies
            .insert("@myorg/nonexistent".to_string(), "workspace:*".to_string());

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
            vec![member],
        );
        let graph = PackageGraph::from_workspace(&ws);
        let protocol = WorkspaceProtocol::new(ws, graph);

        let member = protocol.workspace.find_member("apps/web").unwrap();
        let result = protocol.validate(member);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        // The devDependency on @myorg/nonexistent should be caught
        assert!(errors
            .iter()
            .any(|e| matches!(e, ProtocolError::PackageNotFound(_))));
    }

    #[test]
    fn test_validate_peer_dependencies() {
        let mut member = make_member("apps/web", "apps/web", "1.0.0", vec![]);
        member
            .package_json
            .peer_dependencies
            .insert("@myorg/nonexistent".to_string(), "workspace:*".to_string());

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
            vec![member],
        );
        let graph = PackageGraph::from_workspace(&ws);
        let protocol = WorkspaceProtocol::new(ws, graph);

        let member = protocol.workspace.find_member("apps/web").unwrap();
        let result = protocol.validate(member);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ProtocolError::PackageNotFound(_))));
    }

    #[test]
    fn test_resolve_path_traversal_outside_root() {
        let protocol = make_protocol(vec![make_member(
            "pkg-a",
            "packages/pkg-a",
            "1.0.0",
            vec![],
        )]);
        // Try to resolve a path that escapes workspace root
        let err = protocol
            .resolve(
                "pkg-a",
                &WorkspaceSpecifier::RelativePath(PathBuf::from("../../../etc/passwd")),
            )
            .unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidRelativePath(_)));
    }

    #[test]
    fn test_parse_normalize_path_dot_dot_above_root() {
        let protocol = make_protocol(vec![make_member(
            "pkg-a",
            "packages/pkg-a",
            "1.0.0",
            vec![],
        )]);
        // "./../../../" should fail to find a member
        let err = protocol
            .resolve(
                "pkg-a",
                &WorkspaceSpecifier::RelativePath(PathBuf::from("./../../..")),
            )
            .unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidRelativePath(_)));
    }
}
