//! Package identity types: PackageName, PackageId, VersionRange, DependencySpec

use std::fmt;
use std::hash::Hash;
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use crate::version::Version;

// ─── PackageName ──────────────────────────────────────────────────────────────

/// A validated package name (e.g. "react", "@types/node", "torch")
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct PackageName(String);

impl PackageName {
    pub fn new(name: impl Into<String>) -> Result<Self, PackageNameError> {
        let name = name.into();
        if name.is_empty() {
            return Err(PackageNameError::Empty);
        }
        if name.len() > 214 {
            return Err(PackageNameError::TooLong);
        }
        if name.starts_with('@') {
            // Scoped: @scope/name
            if name.matches('/').count() != 1 {
                return Err(PackageNameError::InvalidScopedFormat);
            }
            let slash = name.find('/').unwrap();
            if name[1..slash].is_empty() || name[slash + 1..].is_empty() {
                return Err(PackageNameError::InvalidScopedFormat);
            }
        } else {
            // Must be lowercase alphanumeric + - _ .
            if !name.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit()
                    || c == '-' || c == '_' || c == '.'
            }) {
                return Err(PackageNameError::InvalidCharacters);
            }
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str { &self.0 }
    pub fn is_scoped(&self) -> bool { self.0.starts_with('@') }

    pub fn scope(&self) -> Option<&str> {
        if self.is_scoped() {
            self.0.find('/').map(|p| &self.0[1..p])
        } else {
            None
        }
    }

    pub fn unscoped(&self) -> &str {
        if self.is_scoped() {
            self.0.find('/').map_or(&self.0, |p| &self.0[p + 1..])
        } else {
            &self.0
        }
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}
impl AsRef<str> for PackageName {
    fn as_ref(&self) -> &str { &self.0 }
}
impl FromStr for PackageName {
    type Err = PackageNameError;
    fn from_str(s: &str) -> Result<Self, Self::Err> { Self::new(s) }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PackageNameError {
    #[error("package name cannot be empty")]
    Empty,
    #[error("package name too long (max 214 chars)")]
    TooLong,
    #[error("invalid scoped package format (expected @scope/name)")]
    InvalidScopedFormat,
    #[error("package name contains invalid characters")]
    InvalidCharacters,
}

// ─── PackageId ────────────────────────────────────────────────────────────────

/// Unique package identifier: name + resolved version
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageId {
    pub name: PackageName,
    pub version: Version,
}

impl PackageId {
    pub fn new(name: PackageName, version: Version) -> Self { Self { name, version } }

    pub fn parse(s: &str) -> Result<Self, PackageIdError> {
        // Handle scoped packages: @scope/name@version
        let (name_part, ver_part) = if s.starts_with('@') {
            // "@scope/name@1.0.0" — last '@' is version separator
            let last_at = s.rfind('@').ok_or_else(|| PackageIdError::MissingVersion(s.into()))?;
            if last_at == 0 {
                return Err(PackageIdError::MissingVersion(s.into()));
            }
            (&s[..last_at], &s[last_at + 1..])
        } else {
            s.split_once('@').ok_or_else(|| PackageIdError::MissingVersion(s.into()))?
        };
        let name = PackageName::new(name_part).map_err(PackageIdError::InvalidName)?;
        let version = Version::parse(ver_part).map_err(PackageIdError::InvalidVersion)?;
        Ok(Self { name, version })
    }

    pub fn name(&self) -> &PackageName { &self.name }
    pub fn version(&self) -> &Version { &self.version }
    pub fn name_str(&self) -> &str { self.name.as_str() }
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

impl FromStr for PackageId {
    type Err = PackageIdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> { Self::parse(s) }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PackageIdError {
    #[error("invalid package name: {0}")]
    InvalidName(#[from] PackageNameError),
    #[error("invalid version: {0}")]
    InvalidVersion(#[from] crate::version::SemVerError),
    #[error("missing version in '{0}' (expected name@version)")]
    MissingVersion(String),
}

// ─── VersionRange ─────────────────────────────────────────────────────────────

/// A version constraint: ^1.0.0, ~2.3, >=3.0.0 <4.0.0, *, etc.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VersionRange(pub String);

/// Alias for backward compatibility
pub type VersionReq = VersionRange;

impl VersionRange {
    pub fn parse(s: &str) -> Result<Self, VersionRangeError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(VersionRangeError::Empty);
        }
        Ok(Self(s.to_string()))
    }

    pub fn star() -> Self { Self("*".into()) }
    pub fn as_str(&self) -> &str { &self.0 }
    pub fn is_star(&self) -> bool { self.0 == "*" || self.0 == ">=0.0.0" }

    /// Returns a heuristic satisfying version for this range.
    /// Used by PubGrub solver for candidate selection.
    pub fn satisfying_version(&self) -> Option<Version> {
        let range = self.0.trim();
        if self.is_star() { return Some(Version::new(0, 0, 0)); }

        // Exact
        if let Ok(v) = Version::parse(range) {
            return Some(v);
        }

        // Caret: ^1.2.3 -> 1.99.9999
        if let Some(min) = range.strip_prefix('^') {
            if let Ok(v) = Version::parse(min.trim()) {
                return Some(Version::new(v.major, 99, 9999));
            }
        }

        // Tilde: ~1.2.3 -> 1.2.9999
        if let Some(min) = range.strip_prefix('~') {
            if let Ok(v) = Version::parse(min.trim()) {
                return Some(Version::new(v.major, v.minor, 9999));
            }
        }

        if let Some(v) = range.strip_prefix(">=") {
            if let Ok(v) = Version::parse(v.trim()) { return Some(v); }
        }

        if let Some(v) = range.strip_prefix('>') {
            if let Ok(v) = Version::parse(v.trim()) { return Some(v); }
        }

        if let Some(v) = range.strip_prefix("<=") {
            if let Ok(v) = Version::parse(v.trim()) { return Some(v); }
        }

        if let Some(v) = range.strip_prefix('<') {
            if let Ok(v) = Version::parse(v.trim()) {
                return Some(Version::new(v.major, v.minor, v.patch.saturating_sub(1)));
            }
        }

        None
    }

    /// Returns true if `version` satisfies this range.
    pub fn matches(&self, version: &Version) -> bool {
        if self.is_star() { return true; }

        let range = self.0.trim();

        // Exact
        if let Ok(v) = Version::parse(range) {
            return *version == v;
        }

        // OR
        if range.contains("||") {
            return range.split("||").any(|part| {
                let part = part.trim();
                VersionRange(part.into()).matches(version)
            });
        }

        // AND (space-separated operators)
        let parts: Vec<&str> = range.split_whitespace().collect();
        if parts.len() > 1 {
            return parts.iter().all(|p| VersionRange(p.to_string()).matches(version));
        }

        // Caret
        if let Some(min) = range.strip_prefix('^') {
            if let Ok(min_v) = Version::parse(min.trim()) {
                let max_v = Version::new(min_v.major + 1, 0, 0);
                if version.is_prerelease() {
                    let base = version.base();
                    if !(base >= min_v && base < max_v) { return false; }
                }
                return *version >= min_v && *version < max_v;
            }
        }

        // Tilde
        if let Some(min) = range.strip_prefix('~') {
            if let Ok(min_v) = Version::parse(min.trim()) {
                let max_v = Version::new(min_v.major, min_v.minor + 1, 0);
                if version.is_prerelease() {
                    let base = version.base();
                    if !(base >= min_v && base < max_v) { return false; }
                }
                return *version >= min_v && *version < max_v;
            }
        }

        if let Some(v) = range.strip_prefix(">=") {
            if let Ok(v) = Version::parse(v.trim()) { return *version >= v; }
        }
        if let Some(v) = range.strip_prefix("<=") {
            if let Ok(v) = Version::parse(v.trim()) { return *version <= v; }
        }
        if let Some(v) = range.strip_prefix('>') {
            if let Ok(v) = Version::parse(v.trim()) { return *version > v; }
        }
        if let Some(v) = range.strip_prefix('<') {
            if let Ok(v) = Version::parse(v.trim()) { return *version < v; }
        }

        false
    }
}

impl Default for VersionRange {
    fn default() -> Self { Self::star() }
}

impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

impl FromStr for VersionRange {
    type Err = VersionRangeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> { Self::parse(s) }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VersionRangeError {
    #[error("version range cannot be empty")]
    Empty,
}

// ─── DependencySpec ───────────────────────────────────────────────────────────

/// A named dependency with a version constraint
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencySpec {
    pub name: PackageName,
    pub range: VersionRange,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub dev: bool,
    #[serde(default)]
    pub peer: bool,
}

impl DependencySpec {
    pub fn new(name: PackageName, range: VersionRange) -> Self {
        Self { name, range, optional: false, dev: false, peer: false }
    }

    /// Parse "react@^18.0.0" or "react" (defaults to "*")
    pub fn parse(s: &str) -> Result<Self, DependencySpecError> {
        let (name_s, range_s) = if s.starts_with('@') {
            // Scoped: @scope/name@range
            match s.rfind('@') {
                Some(pos) if pos > 0 => (&s[..pos], &s[pos + 1..]),
                _ => (s, "*"),
            }
        } else {
            s.split_once('@').unwrap_or((s, "*"))
        };

        let name  = PackageName::new(name_s).map_err(DependencySpecError::Name)?;
        let range = VersionRange::parse(range_s).map_err(DependencySpecError::Range)?;
        Ok(Self::new(name, range))
    }
}

impl fmt::Display for DependencySpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.range)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DependencySpecError {
    #[error("invalid name: {0}")]
    Name(#[from] PackageNameError),
    #[error("invalid range: {0}")]
    Range(#[from] VersionRangeError),
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_name_regular() {
        let n = PackageName::new("react").unwrap();
        assert!(!n.is_scoped());
        assert_eq!(n.as_str(), "react");
    }

    #[test]
    fn test_package_name_scoped() {
        let n = PackageName::new("@types/node").unwrap();
        assert!(n.is_scoped());
        assert_eq!(n.scope(), Some("types"));
        assert_eq!(n.unscoped(), "node");
    }

    #[test]
    fn test_package_id_parse() {
        let id = PackageId::parse("react@18.2.0").unwrap();
        assert_eq!(id.name_str(), "react");
        assert_eq!(id.version().to_string(), "18.2.0");
    }

    #[test]
    fn test_package_id_scoped() {
        let id = PackageId::parse("@types/node@20.0.0").unwrap();
        assert_eq!(id.name_str(), "@types/node");
    }

    #[test]
    fn test_version_range_star() {
        let r = VersionRange::parse("*").unwrap();
        assert!(r.is_star());
        assert!(r.matches(&Version::parse("999.0.0").unwrap()));
    }

    #[test]
    fn test_version_range_caret() {
        let r = VersionRange::parse("^1.2.3").unwrap();
        assert!(r.matches(&Version::parse("1.2.3").unwrap()));
        assert!(r.matches(&Version::parse("1.9.9").unwrap()));
        assert!(!r.matches(&Version::parse("2.0.0").unwrap()));
        assert!(!r.matches(&Version::parse("0.9.9").unwrap()));
    }

    #[test]
    fn test_version_range_tilde() {
        let r = VersionRange::parse("~1.2.0").unwrap();
        assert!(r.matches(&Version::parse("1.2.5").unwrap()));
        assert!(!r.matches(&Version::parse("1.3.0").unwrap()));
    }

    #[test]
    fn test_version_range_or() {
        let r = VersionRange::parse("^1.0.0 || ^2.0.0").unwrap();
        assert!(r.matches(&Version::parse("1.5.0").unwrap()));
        assert!(r.matches(&Version::parse("2.5.0").unwrap()));
        assert!(!r.matches(&Version::parse("3.0.0").unwrap()));
    }

    #[test]
    fn test_dependency_spec_parse() {
        let d = DependencySpec::parse("react@^18.0.0").unwrap();
        assert_eq!(d.name.as_str(), "react");
        assert_eq!(d.range.as_str(), "^18.0.0");
    }

    #[test]
    fn test_dependency_spec_no_version() {
        let d = DependencySpec::parse("lodash").unwrap();
        assert!(d.range.is_star());
    }
}
