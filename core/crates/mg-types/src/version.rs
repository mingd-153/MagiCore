//! Semantic versioning types

use std::fmt;
use std::str::FromStr;
use std::cmp::Ordering;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A parsed semantic version (major.minor.patch[-prerelease][+build])
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prerelease: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self { major, minor, patch, prerelease: None, build: None }
    }

    pub fn parse(s: &str) -> Result<Self, SemVerError> {
        let s = s.trim().trim_start_matches('v');
        if s.is_empty() {
            return Err(SemVerError::Empty);
        }

        // Split build metadata first
        let (ver_pre, build) = if let Some(pos) = s.find('+') {
            (&s[..pos], Some(s[pos + 1..].to_string()))
        } else {
            (s, None)
        };

        // Split prerelease
        let (ver, prerelease) = if let Some(pos) = ver_pre.find('-') {
            (&ver_pre[..pos], Some(ver_pre[pos + 1..].to_string()))
        } else {
            (ver_pre, None)
        };

        let parts: Vec<&str> = ver.split('.').collect();
        if parts.len() < 3 {
            return Err(SemVerError::InvalidFormat(s.to_string()));
        }

        let major = parts[0].parse::<u64>()
            .map_err(|_| SemVerError::InvalidNumber(parts[0].to_string()))?;
        let minor = parts[1].parse::<u64>()
            .map_err(|_| SemVerError::InvalidNumber(parts[1].to_string()))?;
        let patch = parts[2].parse::<u64>()
            .map_err(|_| SemVerError::InvalidNumber(parts[2].to_string()))?;

        Ok(Self { major, minor, patch, prerelease, build })
    }

    /// Returns true if this is a prerelease version
    pub fn is_prerelease(&self) -> bool {
        self.prerelease.is_some()
    }

    /// Returns base version without prerelease/build
    pub fn base(&self) -> Self {
        Self::new(self.major, self.minor, self.patch)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref pre) = self.prerelease {
            write!(f, "-{}", pre)?;
        }
        if let Some(ref build) = self.build {
            write!(f, "+{}", build)?;
        }
        Ok(())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare major.minor.patch
        let cmp = self.major.cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch));

        if cmp != Ordering::Equal {
            return cmp;
        }

        // Pre-release: no pre-release > pre-release (1.0.0 > 1.0.0-beta)
        match (&self.prerelease, &other.prerelease) {
            (None, None)    => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => compare_prerelease(a, b),
        }
    }
}

/// Compare two pre-release strings using semver rules.
/// Numeric identifiers are compared numerically; others lexically.
fn compare_prerelease(a: &str, b: &str) -> Ordering {
    let a_parts = a.split('.');
    let b_parts = b.split('.');

    for (a_id, b_id) in a_parts.zip(b_parts.clone()) {
        let cmp = match (a_id.parse::<u64>(), b_id.parse::<u64>()) {
            (Ok(an), Ok(bn)) => an.cmp(&bn),
            (Ok(_), Err(_))  => Ordering::Less,    // numeric < alphanumeric
            (Err(_), Ok(_))  => Ordering::Greater, // alphanumeric > numeric
            (Err(_), Err(_)) => a_id.cmp(b_id),
        };
        if cmp != Ordering::Equal {
            return cmp;
        }
    }

    // Fewer identifiers < more identifiers
    let a_len = a.split('.').count();
    let b_len = b.split('.').count();
    a_len.cmp(&b_len)
}

impl FromStr for Version {
    type Err = SemVerError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Default for Version {
    fn default() -> Self {
        Self::new(0, 0, 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SemVerError {
    #[error("version string is empty")]
    Empty,
    #[error("invalid version format: '{0}' (expected major.minor.patch)")]
    InvalidFormat(String),
    #[error("invalid version number: '{0}'")]
    InvalidNumber(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.prerelease.is_none());
    }

    #[test]
    fn test_parse_prerelease() {
        let v = Version::parse("1.0.0-beta.1").unwrap();
        assert_eq!(v.prerelease.as_deref(), Some("beta.1"));
    }

    #[test]
    fn test_parse_v_prefix() {
        let v = Version::parse("v2.0.0").unwrap();
        assert_eq!(v.major, 2);
    }

    #[test]
    fn test_ordering() {
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("2.0.0").unwrap();
        let pre = Version::parse("1.0.0-beta").unwrap();
        assert!(v1 < v2);
        assert!(pre < v1); // prerelease < release
    }

    #[test]
    fn test_prerelease_numeric_ordering() {
        let v9  = Version::parse("1.0.0-next.9").unwrap();
        let v24 = Version::parse("1.0.0-next.24").unwrap();
        assert!(v9 < v24, "next.9 should be less than next.24 (numeric comparison)");
    }

    #[test]
    fn test_display() {
        assert_eq!(Version::parse("1.2.3").unwrap().to_string(), "1.2.3");
        assert_eq!(Version::parse("1.0.0-alpha.1").unwrap().to_string(), "1.0.0-alpha.1");
    }
}
