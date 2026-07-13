use crate::error::{MgError, MgResult};
use crate::version::Version;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PackageName(String);

impl PackageName {
    pub fn new(name: impl Into<String>) -> MgResult<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(MgError::InvalidPackageName(name));
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn unscoped(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(self.as_str())
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct VersionRange(String);

impl VersionRange {
    pub fn parse(input: &str) -> MgResult<Self> {
        if input.trim().is_empty() {
            return Err(MgError::InvalidVersionRange(input.to_string()));
        }
        Ok(Self(input.trim().to_string()))
    }

    pub fn star() -> Self {
        Self("*".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_star(&self) -> bool {
        self.0 == "*"
    }

    pub fn matches(&self, version: &Version) -> bool {
        let raw = self.as_str();
        if raw == "*" {
            return true;
        }
        for part in raw.split("||").map(str::trim) {
            if match_compound_range(part, version) {
                return true;
            }
        }
        false
    }

    pub fn satisfying_version(&self) -> Option<Version> {
        let raw = self.as_str().split("||").next()?.trim();
        if raw == "*" {
            return Some(Version::new(0, 0, 0));
        }
        let normalized = raw
            .trim_start_matches('=')
            .trim_start_matches('^')
            .trim_start_matches('~')
            .trim_start_matches(">=")
            .trim_start_matches("<=")
            .trim_start_matches('>')
            .trim_start_matches('<')
            .trim();
        Version::parse(normalized).ok()
    }
}

impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PackageId {
    name: PackageName,
    version: Version,
}

impl PackageId {
    pub fn new(name: PackageName, version: Version) -> Self {
        Self { name, version }
    }

    pub fn parse(input: &str) -> MgResult<Self> {
        let Some(at) = input.rfind('@') else {
            return Err(MgError::InvalidPackageSpec(input.to_string()));
        };
        let name = PackageName::new(&input[..at])?;
        let version = Version::parse(&input[at + 1..])?;
        Ok(Self::new(name, version))
    }

    pub fn name(&self) -> &PackageName {
        &self.name
    }

    pub fn name_str(&self) -> &str {
        self.name.as_str()
    }

    pub fn version(&self) -> &Version {
        &self.version
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DependencySpec {
    pub name: PackageName,
    pub range: VersionRange,
    pub dev: bool,
    pub optional: bool,
    pub peer: bool,
}

impl DependencySpec {
    pub fn new(name: PackageName, range: VersionRange) -> Self {
        Self {
            name,
            range,
            dev: false,
            optional: false,
            peer: false,
        }
    }

    pub fn parse(input: &str) -> MgResult<Self> {
        if let Some(idx) = input.rfind('@') {
            if idx > 0 {
                let name = PackageName::new(&input[..idx])?;
                let range = VersionRange::parse(&input[idx + 1..])?;
                return Ok(Self::new(name, range));
            }
        }
        Ok(Self::new(PackageName::new(input)?, VersionRange::star()))
    }
}

fn match_single_range(range: &str, version: &Version) -> bool {
    if range == "*" {
        return true;
    }
    if let Some(target) = range
        .trim_start_matches('=')
        .split_whitespace()
        .next()
        .and_then(|s| Version::parse(s).ok())
    {
        return version == &target;
    }
    if let Some(target) = range.strip_prefix('^').and_then(|s| Version::parse(s).ok()) {
        return version.major == target.major && version >= &target;
    }
    if let Some(target) = range.strip_prefix('~').and_then(|s| Version::parse(s).ok()) {
        return version.major == target.major
            && version.minor == target.minor
            && version >= &target;
    }
    if let Some(target) = range
        .strip_prefix(">=")
        .and_then(|s| Version::parse(s).ok())
    {
        return version >= &target;
    }
    if let Some(target) = range
        .strip_prefix("<=")
        .and_then(|s| Version::parse(s).ok())
    {
        return version <= &target;
    }
    if let Some(target) = range.strip_prefix('>').and_then(|s| Version::parse(s).ok()) {
        return version > &target;
    }
    if let Some(target) = range.strip_prefix('<').and_then(|s| Version::parse(s).ok()) {
        return version < &target;
    }
    Version::parse(range)
        .map(|target| version == &target)
        .unwrap_or(false)
}

fn match_compound_range(range: &str, version: &Version) -> bool {
    let trimmed = range.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed == "*" {
        return true;
    }

    let mut parts = Vec::new();
    let mut tokens = trimmed.split_whitespace().peekable();
    while let Some(token) = tokens.next() {
        if matches!(token, ">=" | "<=" | ">" | "<" | "=" | "==" | "^" | "~") {
            if let Some(next) = tokens.next() {
                parts.push(format!("{token}{next}"));
            } else {
                return false;
            }
        } else {
            parts.push(token.to_string());
        }
    }

    parts
        .iter()
        .all(|part| match_single_range(part.as_str(), version))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(input: &str) -> Version {
        Version::parse(input).unwrap()
    }

    #[test]
    fn exact_equals_range_matches_same_version() {
        let range = VersionRange::parse("=0.139.0").unwrap();
        assert!(
            range.matches(&v("0.139.0")),
            "=0.139.0 should match 0.139.0"
        );
        assert!(!range.matches(&v("0.139.1")));
    }

    #[test]
    fn exact_equals_range_produces_satisfying_version() {
        let range = VersionRange::parse("=18.3.1").unwrap();
        assert_eq!(range.satisfying_version(), Some(v("18.3.1")));
    }

    #[test]
    fn compound_range_matches_intersection() {
        let range = VersionRange::parse(">=0.2.0 <0.5.0").unwrap();
        assert!(range.matches(&v("0.4.9")));
        assert!(!range.matches(&v("0.5.0")));
        assert!(!range.matches(&v("0.1.9")));
    }

    #[test]
    fn compound_range_matches_spaced_comparators() {
        let range = VersionRange::parse(">= 2.1.2 < 3.0.0").unwrap();
        assert!(range.matches(&v("2.8.1")));
        assert!(!range.matches(&v("2.1.1")));
        assert!(!range.matches(&v("3.0.0")));
    }

    #[test]
    fn npm_double_equals_matches_same_version() {
        let range = VersionRange::parse("==0.139.0").unwrap();
        assert!(range.matches(&v("0.139.0")));
        assert!(!range.matches(&v("0.139.1")));
    }

    #[test]
    fn npm_double_equals_resolves_as_op() {
        let range = VersionRange::parse("== 0.139.0").unwrap();
        assert!(range.matches(&v("0.139.0")));
        assert!(!range.matches(&v("0.139.1")));
    }

    #[test]
    fn npm_single_equals_matches_oxc_types() {
        let range = VersionRange::parse("=0.139.0").unwrap();
        assert!(
            range.matches(&v("0.139.0")),
            "=0.139.0 should match 0.139.0"
        );
        assert!(
            !range.matches(&v("0.138.0")),
            "=0.139.0 should NOT match 0.138.0"
        );
        assert!(
            !range.matches(&v("0.139.1")),
            "=0.139.0 should NOT match 0.139.1"
        );
    }
}
