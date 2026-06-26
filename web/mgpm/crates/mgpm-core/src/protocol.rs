//! Package protocols and resolution sources
//!
//! Defines how packages are fetched and resolved.

use std::fmt;
use serde::{Deserialize, Serialize};

/// Protocol types for package sources
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Protocol {
    /// Standard npm registry (default)
    #[default]
    Registry,
    /// JSR registry (Deno's JavaScript registry)
    Jsr,
    /// Git repository (git+https:// or git+ssh://)
    Git {
        url: String,
        rev: Option<String>,
        subpath: Option<String>,
    },
    /// Direct tarball URL
    Http {
        url: String,
        integrity: Option<String>,
    },
    /// Local file path
    File {
        path: String,
    },
    /// Local link/path
    Link {
        path: String,
    },
    /// Workspace package (monorepo internal)
    Workspace {
        path: String,
    },
    /// Catalog reference (version pinning)
    Catalog {
        name: String,
    },
    /// GitHub shorthand (user/repo)
    Github {
        user: String,
        repo: String,
    },
}

impl Protocol {
    /// Returns true if this protocol requires network access.
    pub fn requires_network(&self) -> bool {
        matches!(
            self,
            Self::Registry 
            | Self::Jsr 
            | Self::Git { .. } 
            | Self::Http { .. } 
            | Self::Github { .. }
        )
    }

    /// Returns a display-friendly name for this protocol.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Registry => "npm",
            Self::Jsr => "jsr",
            Self::Git { .. } => "git",
            Self::Http { .. } => "http",
            Self::File { .. } => "file",
            Self::Link { .. } => "link",
            Self::Workspace { .. } => "workspace",
            Self::Catalog { .. } => "catalog",
            Self::Github { .. } => "github",
        }
    }

    /// Parses a package spec string into a protocol and package name.
    /// 
    /// Examples:
    /// - `react` -> (Registry, "react")
    /// - `lodash@^4.0.0` -> (Registry, "lodash")
    /// - `git+https://github.com/user/repo.git` -> (Git, ...)
    /// - `workspace:@scope/pkg` -> (Workspace, "@scope/pkg")
    /// - `catalog:react` -> (Catalog, "react")
    pub fn parse_spec(spec: &str) -> Option<(Protocol, String)> {
        // Check for workspace: protocol
        if let Some(rest) = spec.strip_prefix("workspace:") {
            return Some((Protocol::Workspace { path: rest.to_string() }, rest.to_string()));
        }
        
        // Check for catalog: protocol
        if let Some(rest) = spec.strip_prefix("catalog:") {
            return Some((Protocol::Catalog { name: rest.to_string() }, rest.to_string()));
        }
        
        // Check for link: protocol
        if let Some(rest) = spec.strip_prefix("link:") {
            return Some((Protocol::Link { path: rest.to_string() }, rest.to_string()));
        }
        
        // Check for file: protocol
        if let Some(rest) = spec.strip_prefix("file:") {
            return Some((Protocol::File { path: rest.to_string() }, rest.to_string()));
        }
        
        // Check for git+https:// or git+ssh://
        if spec.starts_with("git+") {
            let url = spec.strip_prefix("git+").unwrap();
            return Some((Protocol::Git { url: url.to_string(), rev: None, subpath: None }, spec.to_string()));
        }
        
        // Check for http:// or https://
        if spec.starts_with("http://") || spec.starts_with("https://") {
            return Some((Protocol::Http { url: spec.to_string(), integrity: None }, spec.to_string()));
        }
        
        // Default to registry
        Some((Protocol::Registry, spec.to_string()))
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry => write!(f, "npm"),
            Self::Jsr => write!(f, "jsr"),
            Self::Git { url, rev, subpath } => {
                write!(f, "git+{}", url)?;
                if let Some(r) = rev {
                    write!(f, "?rev={}", r)?;
                }
                if let Some(s) = subpath {
                    write!(f, "#{}", s)?;
                }
                Ok(())
            }
            Self::Http { url, .. } => write!(f, "{}", url),
            Self::File { path } => write!(f, "file:{}", path),
            Self::Link { path } => write!(f, "link:{}", path),
            Self::Workspace { path } => write!(f, "workspace:{}", path),
            Self::Catalog { name } => write!(f, "catalog:{}", name),
            Self::Github { user, repo } => write!(f, "github:{}/{}", user, repo),
        }
    }
}

/// A resolved package with download location
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Resolution {
    /// The package ID
    pub id: super::PackageId,
    /// The source protocol
    pub protocol: Protocol,
    /// The resolved tarball URL (if applicable)
    pub tarball: Option<String>,
    /// Integrity hash for verification
    pub integrity: Option<String>,
    /// The registry this was resolved from
    pub registry: Option<String>,
    /// Workspace package local path (if applicable)
    pub local_path: Option<String>,
}

impl Resolution {
    /// Creates a new resolution.
    pub fn new(id: super::PackageId, protocol: Protocol) -> Self {
        Self {
            id,
            protocol,
            tarball: None,
            integrity: None,
            registry: None,
            local_path: None,
        }
    }

    /// Sets the tarball URL.
    pub fn with_tarball(mut self, tarball: &str) -> Self {
        self.tarball = Some(tarball.to_string());
        self
    }

    /// Sets the integrity hash.
    pub fn with_integrity(mut self, integrity: &str) -> Self {
        self.integrity = Some(integrity.to_string());
        self
    }

    /// Sets the registry.
    pub fn with_registry(mut self, registry: &str) -> Self {
        self.registry = Some(registry.to_string());
        self
    }

    /// Sets the local path (for workspace packages).
    pub fn with_local_path(mut self, path: &str) -> Self {
        self.local_path = Some(path.to_string());
        self
    }
}

/// Package metadata from a registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    /// Package name
    pub name: super::PackageName,
    /// Latest version
    pub version: super::Version,
    /// Description
    pub description: Option<String>,
    /// Author
    pub author: Option<Person>,
    /// License
    pub license: Option<String>,
    /// Repository URL
    pub repository: Option<String>,
    /// Homepage URL
    pub homepage: Option<String>,
    /// Keywords
    pub keywords: Vec<String>,
    /// Dependencies
    pub dependencies: Vec<super::DependencySpec>,
    /// Dev dependencies
    pub dev_dependencies: Vec<super::DependencySpec>,
    /// Peer dependencies
    pub peer_dependencies: Vec<super::DependencySpec>,
    /// Optional dependencies
    pub optional_dependencies: Vec<super::DependencySpec>,
    /// Available versions
    pub versions: Vec<super::Version>,
    /// Created timestamp
    pub created: Option<String>,
    /// Modified timestamp
    pub modified: Option<String>,
}

/// A person (author/maintainer)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
    pub email: Option<String>,
    pub url: Option<String>,
}

/// Registry configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Registry URL (e.g., "https://registry.npmjs.org")
    pub url: String,
    /// Authentication token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Always authenticate
    #[serde(default)]
    pub always_auth: bool,
    /// Scope for this registry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: "https://registry.npmjs.org".to_string(),
            token: None,
            always_auth: false,
            scope: None,
        }
    }
}

impl RegistryConfig {
    /// Creates a new registry config with the default npm registry.
    pub fn npm() -> Self {
        Self::default()
    }

    /// Creates a new registry config with JSR.
    pub fn jsr() -> Self {
        Self {
            url: "https://registry.jsr.io".to_string(),
            ..Default::default()
        }
    }

    /// Sets the authentication token.
    pub fn with_token(mut self, token: &str) -> Self {
        self.token = Some(token.to_string());
        self
    }

    /// Sets the scope.
    pub fn with_scope(mut self, scope: &str) -> Self {
        self.scope = Some(scope.to_string());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_parse_workspace() {
        let (proto, name) = Protocol::parse_spec("workspace:@scope/pkg").unwrap();
        assert!(matches!(proto, Protocol::Workspace { .. }));
        assert_eq!(name, "@scope/pkg");
    }

    #[test]
    fn test_protocol_parse_catalog() {
        let (proto, name) = Protocol::parse_spec("catalog:react").unwrap();
        assert!(matches!(proto, Protocol::Catalog { .. }));
        assert_eq!(name, "react");
    }

    #[test]
    fn test_protocol_network() {
        assert!(Protocol::Registry.requires_network());
        assert!(!Protocol::Workspace { path: ".".to_string() }.requires_network());
    }
}