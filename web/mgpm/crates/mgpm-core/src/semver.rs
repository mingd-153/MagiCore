//! Semantic versioning types and operations
//!
//! Implements a subset of Semantic Versioning 2.0.0

use std::fmt;
use std::str::FromStr;
use serde::{Deserialize, Serialize};

/// A semantic version number (MAJOR.MINOR.PATCH[-prerelease][+build])
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Option<String>,
    pub build: Option<String>,
}

impl Version {
    /// Creates a new version from components.
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: None,
            build: None,
        }
    }

    /// Creates a new version with pre-release suffix.
    pub fn with_prerelease(mut self, prerelease: &str) -> Self {
        self.prerelease = Some(prerelease.to_string());
        self
    }

    /// Creates a new version with build metadata.
    pub fn with_build(mut self, build: &str) -> Self {
        self.build = Some(build.to_string());
        self
    }

    /// Parses a version string (e.g., "1.2.3", "1.0.0-alpha.1", "2.0.0+build.123")
    pub fn parse(s: &str) -> Result<Self, SemVerError> {
        let (version_str, meta) = if let Some((v, m)) = s.split_once('+') {
            (v, Some(m))
        } else {
            (s, None)
        };

        let (version_str, prerelease) = if let Some((v, p)) = version_str.split_once('-') {
            (v, Some(p))
        } else {
            (version_str, None)
        };

        let parts: Vec<&str> = version_str.split('.').collect();
        if parts.len() != 3 {
            return Err(SemVerError::InvalidFormat(s.to_string()));
        }

        let major = parts[0].parse().map_err(|_| SemVerError::InvalidNumber("major".to_string()))?;
        let minor = parts[1].parse().map_err(|_| SemVerError::InvalidNumber("minor".to_string()))?;
        let patch = parts[2].parse().map_err(|_| SemVerError::InvalidNumber("patch".to_string()))?;

        Ok(Self {
            major,
            minor,
            patch,
            prerelease: prerelease.map(|s| s.to_string()),
            build: meta.map(|s| s.to_string()),
        })
    }

    /// Returns the major version number.
    pub fn major(&self) -> u64 {
        self.major
    }

    /// Returns the minor version number.
    pub fn minor(&self) -> u64 {
        self.minor
    }

    /// Returns the patch version number.
    pub fn patch(&self) -> u64 {
        self.patch
    }

    /// Returns the prerelease suffix, if any.
    pub fn prerelease(&self) -> Option<&str> {
        self.prerelease.as_deref()
    }

    /// Returns the build metadata, if any.
    pub fn build_meta(&self) -> Option<&str> {
        self.build.as_deref()
    }

    /// Returns true if this is a prerelease version.
    pub fn is_prerelease(&self) -> bool {
        self.prerelease.is_some()
    }

    /// Compares versions according to semver rules.
    /// 
    /// Pre-release versions are always less than release versions.
    /// Build metadata is ignored in comparisons.
    pub fn cmp_version(&self, other: &Version) -> std::cmp::Ordering {
        // Compare major.minor.patch
        let cmp = self.major.cmp(&other.major);
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }
        let cmp = self.minor.cmp(&other.minor);
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }
        let cmp = self.patch.cmp(&other.patch);
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }

        // A version without prerelease > version with prerelease
        match (&self.prerelease, &other.prerelease) {
            (None, None) => std::cmp::Ordering::Equal,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(a), Some(b)) => a.cmp(b),
        }
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

impl FromStr for Version {
    type Err = SemVerError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp_version(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.cmp_version(other)
    }
}

/// Errors that can occur when parsing a semantic version.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SemVerError {
    #[error("invalid semver format: '{0}'")]
    InvalidFormat(String),
    #[error("invalid {0} number")]
    InvalidNumber(String),
}

/// Integrity hash for package verification (SSPI-like)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntegrityHash {
    algorithm: String, // "sha512", "sha384", "sha256"
    hash: String,
    bytes: Vec<u8>,
}

impl IntegrityHash {
    /// Parses an SRI format string (e.g., "sha256-<base64>", "sha512-<base64>").
    pub fn from_sri(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        if parts.len() != 2 {
            return Err("invalid SRI format: missing algorithm-base64 separator".to_string());
        }
        let algorithm = parts[0];
        let b64 = parts[1];

        match algorithm {
            "sha256" | "sha384" | "sha512" => {}
            _ => return Err(format!("unknown algorithm: {}", algorithm)),
        }

        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
            .map_err(|e| format!("base64 decode error: {}", e))?;
        let hash = hex::encode(&bytes);

        Ok(Self {
            algorithm: algorithm.to_string(),
            hash,
            bytes,
        })
    }

    /// Creates a new SHA-512 integrity hash.
    pub fn sha512(hash: &str) -> Self {
        let bytes = hex::decode(hash).unwrap_or_default();
        Self {
            algorithm: "sha512".to_string(),
            hash: hash.to_string(),
            bytes,
        }
    }

    /// Creates a new SHA-384 integrity hash.
    pub fn sha384(hash: &str) -> Self {
        let bytes = hex::decode(hash).unwrap_or_default();
        Self {
            algorithm: "sha384".to_string(),
            hash: hash.to_string(),
            bytes,
        }
    }

    /// Creates a new SHA-256 integrity hash.
    pub fn sha256(hash: &str) -> Self {
        let bytes = hex::decode(hash).unwrap_or_default();
        Self {
            algorithm: "sha256".to_string(),
            hash: hash.to_string(),
            bytes,
        }
    }

    /// Returns the hash as a base64-encoded string.
    pub fn as_base64(&self) -> String {
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &self.bytes)
    }

    /// Returns the SSPI format string (algorithm-base64).
    pub fn as_sspi(&self) -> String {
        format!("{}-{}", self.algorithm, self.as_base64())
    }

    /// Returns the raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the hex-encoded hash.
    pub fn as_hex(&self) -> &str {
        &self.hash
    }

    /// Returns the algorithm name.
    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }
}

impl fmt::Display for IntegrityHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_sspi())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_version_parse() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.major(), 1);
        assert_eq!(v.minor(), 2);
        assert_eq!(v.patch(), 3);
    }

    #[test]
    fn test_version_prerelease() {
        let v = Version::parse("1.0.0-alpha.1").unwrap();
        assert!(v.is_prerelease());
        assert_eq!(v.prerelease(), Some("alpha.1"));
    }

    #[test]
    fn test_version_ordering() {
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("2.0.0").unwrap();
        let v3 = Version::parse("1.0.0-alpha").unwrap();

        assert!(v1 < v2);
        assert!(v1 > v3); // Release > prerelease
    }

    #[test]
    fn test_integrity_hash() {
        let hash = IntegrityHash::sha512("abc123");
        assert_eq!(hash.algorithm(), "sha512");
    }

    fn valid_version_string() -> impl Strategy<Value = String> {
        "[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}"
    }

    proptest! {
        #[test]
        fn proptest_version_parse_roundtrip(ref s in valid_version_string()) {
            if let Ok(v) = Version::parse(s) {
                let displayed = v.to_string();
                // Re-parse the displayed version and verify it matches
                if let Ok(v2) = Version::parse(&displayed) {
                    assert_eq!(v, v2);
                }
            }
        }

        #[test]
        fn proptest_version_comparison_antisymmetry(a_major in 0u64..10, a_minor in 0u64..10, a_patch in 0u64..10,
                                                      b_major in 0u64..10, b_minor in 0u64..10, b_patch in 0u64..10) {
            let a = Version::new(a_major, a_minor, a_patch);
            let b = Version::new(b_major, b_minor, b_patch);
            let cmp_ab = a.cmp(&b);
            let cmp_ba = b.cmp(&a);

            if a == b {
                assert_eq!(cmp_ab, std::cmp::Ordering::Equal);
                assert_eq!(cmp_ba, std::cmp::Ordering::Equal);
            } else {
                assert_eq!(cmp_ab, cmp_ba.reverse());
                assert_ne!(cmp_ab, std::cmp::Ordering::Equal);
            }
        }
    }
}
