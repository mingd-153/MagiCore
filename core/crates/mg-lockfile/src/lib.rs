/// Lockfile management for reproducible installs
/// 
/// Manages megagate.lock files to ensure reproducible package installations.

use anyhow::Result;
use mg_types::{PackageId, PackageName, Version};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub mod serialization;

/// Lockfile format version
pub const LOCKFILE_VERSION: u32 = 1;

/// Locked package entry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LockedPackage {
    pub id: PackageId,
    pub version: Version,
    pub resolved: String, // URL where package was fetched from
    pub integrity: String, // Hash for verification
    pub dependencies: Vec<PackageId>,
}

/// Lockfile structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u32,
    pub packages: HashMap<PackageId, LockedPackage>,
}

impl Lockfile {
    pub fn new() -> Self {
        Self {
            version: LOCKFILE_VERSION,
            packages: HashMap::new(),
        }
    }

    /// Load lockfile from path
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(path)?;
        let lockfile: Self = serde_json::from_str(&content)?;
        Ok(lockfile)
    }

    /// Save lockfile to path
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Add locked package
    pub fn insert(&mut self, pkg: LockedPackage) {
        self.packages.insert(pkg.id.clone(), pkg);
    }

    /// Get locked package
    pub fn get(&self, id: &PackageId) -> Option<&LockedPackage> {
        self.packages.get(id)
    }

    /// Check if package is locked
    pub fn contains(&self, id: &PackageId) -> bool {
        self.packages.contains_key(id)
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lockfile_creation() {
        let lockfile = Lockfile::new();
        assert_eq!(lockfile.version, LOCKFILE_VERSION);
        assert!(lockfile.packages.is_empty());
    }

    #[test]
    fn test_lockfile_insert() {
        let mut lockfile = Lockfile::new();
        let pkg = LockedPackage {
            id: PackageId::new(PackageName::new("test-pkg").unwrap(), Version::new(1, 0, 0)),
            version: Version::new(1, 0, 0),
            resolved: "https://example.com/test-pkg-1.0.0.tgz".to_string(),
            integrity: "sha256-abc123".to_string(),
            dependencies: vec![],
        };

        lockfile.insert(pkg.clone());
        assert!(lockfile.contains(&pkg.id));
        assert_eq!(lockfile.get(&pkg.id), Some(&pkg));
    }
}
