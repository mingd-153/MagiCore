use crate::error::{MgError, MgResult};
use crate::version::Version;
use std::fmt;

const MAX_PACKAGE_NAME_LEN: usize = 214;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PackageName(String);

impl PackageName {
    pub fn new(name: impl Into<String>) -> MgResult<Self> {
        let name = name.into();
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(MgError::InvalidPackageName(name));
        }
        if trimmed.len() > MAX_PACKAGE_NAME_LEN {
            return Err(MgError::InvalidPackageName(name));
        }
        if trimmed.contains('\0') || trimmed.contains('\n') || trimmed.contains('\r') {
            return Err(MgError::InvalidPackageName(name));
        }
        if trimmed.contains("..") || trimmed.starts_with('/') || trimmed.starts_with('.') {
            return Err(MgError::InvalidPackageName(name));
        }
        if trimmed.contains('\\') || trimmed.contains('~') {
            return Err(MgError::InvalidPackageName(name));
        }
        let slash_count = trimmed.chars().filter(|&c| c == '/').count();
        if slash_count > 1 {
            return Err(MgError::InvalidPackageName(name));
        }
        if slash_count == 1 {
            let parts: Vec<&str> = trimmed.splitn(2, '/').collect();
            if !parts[0].starts_with('@') || parts[0].len() < 2 || parts[1].is_empty() {
                return Err(MgError::InvalidPackageName(name));
            }
        }
        if trimmed.chars().any(|c| {
            !c.is_ascii_alphanumeric() && !matches!(c, '@' | '/' | '-' | '_' | '.' | '!' | '~')
        }) {
            return Err(MgError::InvalidPackageName(name));
        }
        Ok(Self(trimmed.to_string()))
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
        for alt in self.as_str().split("||").map(str::trim) {
            if alt.is_empty() {
                continue;
            }
            if alt == "*" {
                return Some(Version::new(0, 0, 0));
            }
            if let Some(v) = wildcard_satisfying_version(alt) {
                return Some(v);
            }
            let normalized = alt
                .trim_start_matches('=')
                .trim_start_matches('^')
                .trim_start_matches('~')
                .trim_start_matches(">=")
                .trim_start_matches("<=")
                .trim_start_matches('>')
                .trim_start_matches('<')
                .trim();
            if let Ok(v) = Version::parse(normalized) {
                return Some(v);
            }
        }
        None
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    // Enhanced: Handle wildcard ranges with comparison operators (>=22.x, <=24.x)
    if range.starts_with(">=")
        || range.starts_with("<=")
        || range.starts_with('>')
        || range.starts_with('<')
    {
        if let Some((low, high)) = parse_wildcard_bounds(range) {
            // For >= and >, check lower bound; for <= and <, check upper bound
            if range.starts_with(">=") {
                return version >= &low;
            } else if range.starts_with(">") {
                return version > &low;
            } else if range.starts_with("<=") {
                return version < &high;
            } else if range.starts_with("<") {
                return version < &low;
            }
        }
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
    // x-range wildcard: 12.x, 12.3.x, x
    if let Some((low, high)) = parse_wildcard_bounds(range) {
        return version >= &low && version < &high;
    }
    Version::parse(range)
        .map(|target| version == &target)
        .unwrap_or(false)
}

fn parse_wildcard_bounds(range: &str) -> Option<(Version, Version)> {
    let trimmed = range
        .trim_start_matches('=')
        .trim_start_matches('^')
        .trim_start_matches('~')
        .trim_start_matches(">=")
        .trim_start_matches("<=")
        .trim_start_matches('>')
        .trim_start_matches('<')
        .trim();

    if !trimmed.contains('x') && !trimmed.contains('X') {
        return None;
    }
    let parts: Vec<&str> = trimmed.split('.').collect();
    match parts.len() {
        1 if parts[0] == "x" || parts[0] == "X" => {
            Some((Version::new(0, 0, 0), Version::new(u64::MAX, 0, 0)))
        }
        2 => {
            let major: u64 = parts[0].parse().ok()?;
            if parts[1] == "x" || parts[1] == "X" {
                Some((Version::new(major, 0, 0), Version::new(major + 1, 0, 0)))
            } else {
                None
            }
        }
        3 => {
            let major: u64 = parts[0].parse().ok()?;
            let minor: u64 = parts[1].parse().ok()?;
            if parts[2] == "x" || parts[2] == "X" {
                Some((
                    Version::new(major, minor, 0),
                    Version::new(major, minor + 1, 0),
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_hyphen_range(range: &str) -> Option<(Version, Version)> {
    let trimmed = range.trim();
    let parts: Vec<&str> = trimmed.splitn(3, |c: char| c.is_whitespace()).collect();
    if parts.len() != 3 || parts[1] != "-" {
        return None;
    }
    let low = Version::parse(parts[0]).ok()?;
    let high = Version::parse(parts[2]).ok()?;
    Some((low, high))
}

fn wildcard_satisfying_version(range: &str) -> Option<Version> {
    let trimmed = range
        .trim_start_matches('=')
        .trim_start_matches('^')
        .trim_start_matches('~')
        .trim_start_matches(">=")
        .trim_start_matches("<=")
        .trim_start_matches('>')
        .trim_start_matches('<')
        .trim();

    if !trimmed.contains('x') && !trimmed.contains('X') {
        return None;
    }
    let parts: Vec<&str> = trimmed.split('.').collect();
    match parts.len() {
        1 if parts[0] == "x" || parts[0] == "X" => Some(Version::new(0, 0, 0)),
        2 => {
            let major: u64 = parts[0].parse().ok()?;
            if parts[1] == "x" || parts[1] == "X" {
                Some(Version::new(major, 0, 0))
            } else {
                None
            }
        }
        3 => {
            let major: u64 = parts[0].parse().ok()?;
            let minor: u64 = parts[1].parse().ok()?;
            if parts[2] == "x" || parts[2] == "X" {
                Some(Version::new(major, minor, 0))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn match_compound_range(range: &str, version: &Version) -> bool {
    let trimmed = range.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed == "*" {
        return true;
    }

    // Hyphen range: "1 - 2" → >=1.0.0 <=2.0.0, "1.2.3 - 2.3.4" → >=1.2.3 <=2.3.4
    if let Some((low, high)) = parse_hyphen_range(trimmed) {
        return version >= &low && version <= &high;
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
