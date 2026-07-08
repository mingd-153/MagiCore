//! Package identity types
//!
//! Core types for representing packages, versions, and dependencies.

use std::fmt;
use std::hash::Hash;
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use super::semver::Version;

/// A package name (e.g., "react", "@types/node", "lodash")
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct PackageName(String);

impl PackageName {
    /// Creates a new package name from a string.
    /// 
    /// # Errors
    /// Returns an error if the name is empty or contains invalid characters.
    pub fn new(name: impl Into<String>) -> Result<Self, PackageNameError> {
        let name = name.into();
        if name.is_empty() {
            return Err(PackageNameError::Empty);
        }
        if name.len() > 214 {
            return Err(PackageNameError::TooLong);
        }
        // Scoped packages start with @
        if name.starts_with('@') {
            if !name.starts_with("@") || name.matches('/').count() != 1 {
                return Err(PackageNameError::InvalidScopedFormat);
            }
            let slash_pos = name.find('/').unwrap_or(0);
            let scope = &name[1..slash_pos];
            let name_part = &name[slash_pos + 1..];
            if scope.is_empty() || name_part.is_empty() {
                return Err(PackageNameError::InvalidScopedFormat);
            }
        } else {
            // Regular package name: lowercase letters, numbers, hyphens, underscores, periods
            if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' || c == '.') {
                return Err(PackageNameError::InvalidCharacters);
            }
        }
        Ok(Self(name))
    }

    /// Returns the package name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true if this is a scoped package (starts with @).
    pub fn is_scoped(&self) -> bool {
        self.0.starts_with('@')
    }

    /// Returns the scope for scoped packages, or None for regular packages.
    pub fn scope(&self) -> Option<&str> {
        if self.is_scoped() {
            self.0.find('/').map(|pos| &self.0[1..pos])
        } else {
            None
        }
    }

    /// Returns the package name without the scope prefix.
    pub fn unscoped(&self) -> &str {
        if self.is_scoped() {
            self.0.find('/').map_or(&self.0[..], |pos| &self.0[pos + 1..])
        } else {
            &self.0
        }
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for PackageName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for PackageName {
    type Err = PackageNameError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// Errors that can occur when parsing a package name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackageNameError {
    #[error("package name cannot be empty")]
    Empty,
    #[error("package name too long (max 214 characters)")]
    TooLong,
    #[error("invalid scoped package format (must be @scope/name)")]
    InvalidScopedFormat,
    #[error("package name contains invalid characters (use lowercase letters, numbers, hyphens, underscores, periods)")]
    InvalidCharacters,
}

/// A unique package identifier including name and version
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageId {
    name: PackageName,
    version: Version,
}

impl PackageId {
    /// Creates a new package ID from name and version.
    pub fn new(name: PackageName, version: Version) -> Self {
        Self { name, version }
    }

    /// Creates a new package ID from name and version strings.
    pub fn new_str(name: &str, version: &str) -> Result<Self, PackageIdError> {
        let name = PackageName::new(name)
            .map_err(PackageIdError::InvalidName)?;
        let version = Version::parse(version)
            .map_err(PackageIdError::InvalidVersion)?;
        Ok(Self { name, version })
    }

    /// Returns the package name.
    pub fn name(&self) -> &PackageName {
        &self.name
    }

    /// Returns the package version.
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Returns the package name as a string.
    pub fn name_str(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the version as a string.
    pub fn version_str(&self) -> String {
        self.version.to_string()
    }

    /// Returns the full package specifier (e.g., "react@18.2.0" or "@types/node@5.0.0")
    pub fn as_spec(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

impl FromStr for PackageId {
    type Err = PackageIdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (name, version) = s.split_once('@')
            .ok_or_else(|| PackageIdError::MissingVersion(s.to_string()))?;
        Self::new_str(name, version)
    }
}

/// Error type for package ID operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackageIdError {
    #[error("invalid package name: {0}")]
    InvalidName(#[from] PackageNameError),
    #[error("invalid version: {0}")]
    InvalidVersion(#[from] super::semver::SemVerError),
    #[error("missing version separator '@' in '{0}'")]
    MissingVersion(String),
}

/// A package dependency specification
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DependencySpec {
    /// The package name
    pub name: PackageName,
    /// The version range (e.g., "^1.0.0", "~2.3.4", ">=3.0.0 <4.0.0")
    pub version_req: VersionRange,
    /// Optional features list
    #[serde(default)]
    pub features: Vec<String>,
    /// Optional peer dependencies
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub peer_dependencies: Vec<DependencySpec>,
    /// Whether this is an optional dependency
    #[serde(default)]
    pub optional: bool,
    /// The source/protocol of this dependency
    #[serde(default)]
    pub protocol: Protocol,
}

impl DependencySpec {
    /// Creates a new dependency specification.
    pub fn new(name: PackageName, version_req: VersionRange) -> Self {
        Self {
            name,
            version_req,
            features: Vec::new(),
            peer_dependencies: Vec::new(),
            optional: false,
            protocol: Protocol::Registry,
        }
    }

    /// Creates a dependency spec from a string like "react@^18.0.0".
    pub fn parse(spec: &str) -> Result<Self, DependencySpecError> {
        // TODO: Parse features, optional, peer deps
        let (name, version) = spec.split_once('@')
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .unwrap_or_else(|| (spec.to_string(), "*".to_string()));
        
        let name = PackageName::new(&name)
            .map_err(DependencySpecError::InvalidName)?;
        let version_req = VersionRange::parse(&version)
            .map_err(DependencySpecError::InvalidVersion)?;
        
        Ok(Self {
            name,
            version_req,
            features: Vec::new(),
            peer_dependencies: Vec::new(),
            optional: false,
            protocol: Protocol::Registry,
        })
    }

    /// Sets the optional flag.
    pub fn optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    /// Sets the protocol.
    pub fn protocol(mut self, protocol: Protocol) -> Self {
        self.protocol = protocol;
        self
    }

    /// Adds a feature.
    pub fn add_feature(mut self, feature: impl Into<String>) -> Self {
        self.features.push(feature.into());
        self
    }
}

impl fmt::Display for DependencySpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.name, self.version_req)?;
        if !self.features.is_empty() {
            write!(f, "[{}]", self.features.join(","))?;
        }
        Ok(())
    }
}

/// Error type for dependency specification parsing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DependencySpecError {
    #[error("invalid package name: {0}")]
    InvalidName(#[from] PackageNameError),
    #[error("invalid version range: {0}")]
    InvalidVersion(#[from] VersionRangeError),
}

/// Version range for dependency specification
/// 
/// Supports: `^1.0.0`, `~2.3.4`, `>=3.0.0`, `<4.0.0`, `1.2.3`, `*`, `>=1.0.0 <2.0.0`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VersionRange(String);

impl VersionRange {
    /// Parses a version range from a string.
    pub fn parse(s: &str) -> Result<Self, VersionRangeError> {
        if s.is_empty() {
            return Err(VersionRangeError::Empty);
        }
        Ok(Self(s.to_string()))
    }

    /// Returns the version range as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true if this range matches any version.
    pub fn is_star(&self) -> bool {
        self.0 == "*"
    }

    /// Returns true if this range contains the given version.
    pub fn contains(&self, version: &Version) -> bool {
        // Fast path: try C FFI first (covers all range types efficiently)
        if cfg!(not(miri)) {
            if let Some(result) = crate::cffi::semver::range_contains(&self.0, version) {
                return result;
            }
        }

        let version_str = version.to_string();
        
        if self.is_star() {
            return true;
        }

        let range_str = self.0.trim();
        
        if range_str == version_str {
            return true;
        }

        // Handle || (OR / union) — any sub-range matching is sufficient
        if range_str.contains("||") {
            return range_str.split("||").any(|part| {
                let part = part.trim();
                if part.is_empty() { return false; }
                let sub = VersionRange(part.to_string());
                sub.contains(version)
            });
        }

        if let Some(min) = range_str.strip_prefix('^') {
            if let Ok(min_v) = Version::parse(min) {
                let max_v = increment_major(&min_v);
                // npm semver: prerelease versions are only matched if their base (without prerelease)
                // also falls within the range. This prevents e.g. ^3.4.0 matching 4.0.0-beta.9.
                if version.prerelease.is_some() {
                    let base = Version { major: version.major, minor: version.minor, patch: version.patch, prerelease: None, build: None };
                    if !(base >= min_v && base < max_v) {
                        return false;
                    }
                }
                return *version >= min_v && *version < max_v;
            }
        }

        if let Some(min) = range_str.strip_prefix('~') {
            if let Ok(min_v) = Version::parse(min) {
                let max_v = increment_minor(&min_v);
                if version.prerelease.is_some() {
                    let base = Version { major: version.major, minor: version.minor, patch: version.patch, prerelease: None, build: None };
                    if !(base >= min_v && base < max_v) {
                        return false;
                    }
                }
                return *version >= min_v && *version < max_v;
            }
        }

        if let Some(ver) = range_str.strip_prefix(">=") {
            if let Ok(v) = Version::parse(ver) {
                return *version >= v;
            }
        }

        if let Some(ver) = range_str.strip_prefix('>') {
            if let Ok(v) = Version::parse(ver) {
                return *version > v;
            }
        }

        if let Some(ver) = range_str.strip_prefix("<=") {
            if let Ok(v) = Version::parse(ver) {
                return *version <= v;
            }
        }

        if let Some(ver) = range_str.strip_prefix('<') {
            if let Ok(v) = Version::parse(ver) {
                return *version < v;
            }
        }

        if range_str.contains(">=") && range_str.contains("<") {
            let parts: Vec<&str> = range_str.split("&&").collect();
            if parts.len() == 2 {
                let min_str = parts[0].trim().trim_start_matches(">=");
                let max_str = parts[1].trim().trim_start_matches('<');
                if let (Ok(min_v), Ok(max_v)) = (Version::parse(min_str), Version::parse(max_str)) {
                    if version.prerelease.is_some() {
                        let base = Version { major: version.major, minor: version.minor, patch: version.patch, prerelease: None, build: None };
                        if !(base >= min_v && base < max_v) {
                            return false;
                        }
                    }
                    return *version >= min_v && *version < max_v;
                }
            }
        }

        false
    }
    
    /// Returns a version that satisfies this range (prefers highest for open ranges)
    pub fn satisfying_version(&self) -> Option<Version> {
        let range_str = self.0.trim();
        
        if self.is_star() {
            return Some(Version::new(0, 0, 0));
        }
        
        if range_str == ">=0.0.0" || range_str == ">0.0.0" {
            return Some(Version::new(0, 0, 0));
        }
        
        // For ^x.y.z, return the minimum version in range
        if let Some(min) = range_str.strip_prefix('^') {
            if let Ok(min_v) = Version::parse(min) {
                return Some(min_v);
            }
        }
        
        // For ~x.y.z, return the minimum version in range
        if let Some(min) = range_str.strip_prefix('~') {
            if let Ok(min_v) = Version::parse(min) {
                return Some(min_v);
            }
        }
        
        // For >=x.y.z, return that version
        if let Some(ver) = range_str.strip_prefix(">=") {
            if let Ok(v) = Version::parse(ver) {
                return Some(v);
            }
        }
        
        // For exact version
        if let Ok(v) = Version::parse(range_str) {
            return Some(v);
        }
        
        Some(Version::new(0, 0, 0))
    }

    /// Returns the intersection of this range with another.
    pub fn intersection(&self, other: &VersionRange) -> VersionRange {
        if self.is_star() {
            return other.clone();
        }
        if other.is_star() {
            return self.clone();
        }
        
        if *self == *other {
            return self.clone();
        }
        
        VersionRange(format!("{} && {}", self.0, other.0))
    }
}

fn increment_major(v: &Version) -> Version {
    Version {
        major: v.major + 1,
        minor: 0,
        patch: 0,
        prerelease: None,
        build: None,
    }
}

fn increment_minor(v: &Version) -> Version {
    Version {
        major: v.major,
        minor: v.minor + 1,
        patch: 0,
        prerelease: None,
        build: None,
    }
}

impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for VersionRange {
    fn default() -> Self {
        Self("*".to_string())
    }
}

impl FromStr for VersionRange {
    type Err = VersionRangeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VersionRangeError {
    #[error("version range cannot be empty")]
    Empty,
}

use crate::protocol::Protocol;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_name_regular() {
        let name = PackageName::new("react").unwrap();
        assert_eq!(name.as_str(), "react");
        assert!(!name.is_scoped());
    }

    #[test]
    fn test_package_name_scoped() {
        let name = PackageName::new("@types/node").unwrap();
        assert!(name.is_scoped());
        assert_eq!(name.scope(), Some("types"));
        assert_eq!(name.unscoped(), "node");
    }

    #[test]
    fn test_package_id() {
        let id = PackageId::new_str("react", "18.2.0").unwrap();
        assert_eq!(id.as_spec(), "react@18.2.0");
    }

    #[test]
    fn test_dependency_spec() {
        let spec = DependencySpec::parse("react@^18.0.0").unwrap();
        assert_eq!(spec.name.as_str(), "react");
    }

    #[test]
    fn test_version_range_star() {
        let range = VersionRange::parse("*").unwrap();
        assert!(range.is_star());
    }
}
