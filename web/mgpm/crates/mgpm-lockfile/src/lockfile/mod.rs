//! Lockfile data structures and serialization
//!
//! Dual format support:
//! - Binary (mgpm.lockb): bincode + custom header, fast load
//! - Text (mgpm.lock): TOML, human-readable, git-diffable

use serde::{Deserialize, Serialize};

use mgpm_core::{PackageId, Protocol, Resolution as CoreResolution};
use mgpm_resolver::Resolution as ResolverResolution;

pub const LOCKFILE_VERSION: u32 = 1;
pub const LOCKFILE_MAGIC: &[u8] = b"MGPMLOCK";
pub const LOCKFILE_BINARY_EXT: &str = "lockb";
pub const LOCKFILE_TEXT_EXT: &str = "lock";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u32,
    pub metadata: LockfileMetadata,
    pub packages: Vec<LockfilePackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockfileMetadata {
    pub config_version: u32,
    pub created_at: u64,
    pub updated_at: u64,
    pub content_hash: String,
    pub registry: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockfilePackage {
    pub id: String,
    pub name: String,
    pub version: String,
    pub resolution: PackageResolution,
    pub integrity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageResolution {
    pub r#type: String,
    pub url: String,
    pub registry: Option<String>,
}

impl Lockfile {
    pub fn new(config_version: u32, registry: &str) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            version: LOCKFILE_VERSION,
            metadata: LockfileMetadata {
                config_version,
                created_at: now,
                updated_at: now,
                content_hash: String::new(),
                registry: registry.to_string(),
            },
            packages: Vec::new(),
        }
    }

    pub fn add_package(&mut self, pkg: LockfilePackage) {
        self.packages.push(pkg);
    }

    pub fn find_package(&self, name: &str, version: &str) -> Option<&LockfilePackage> {
        self.packages.iter().find(|p| p.name == name && p.version == version)
    }

    pub fn sort_packages(&mut self) {
        self.packages.sort_by(|a, b| {
            a.name.cmp(&b.name).then_with(|| a.version.cmp(&b.version))
        });
    }

    pub fn compute_content_hash(&mut self) {
        let data = serde_json::to_string(&self.packages).unwrap_or_default();
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        self.metadata.content_hash = format!("{:x}", hasher.finish());
    }

    pub fn update_timestamp(&mut self) {
        self.metadata.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
}

impl LockfilePackage {
    pub fn from_resolution(id: &PackageId, resolution: &CoreResolution) -> Self {
        let (res_type, url, registry) = match &resolution.protocol {
            Protocol::Registry => (
                "registry".to_string(),
                resolution.tarball.clone().unwrap_or_default(),
                resolution.registry.clone(),
            ),
            Protocol::Workspace { path } => (
                "workspace".to_string(),
                path.clone(),
                None,
            ),
            Protocol::Git { url, .. } => (
                "git".to_string(),
                url.clone(),
                None,
            ),
            Protocol::Http { url, .. } => (
                "http".to_string(),
                url.clone(),
                None,
            ),
            Protocol::File { path } => (
                "file".to_string(),
                path.clone(),
                None,
            ),
            Protocol::Link { path } => (
                "link".to_string(),
                path.clone(),
                None,
            ),
            Protocol::Catalog { name } => (
                "catalog".to_string(),
                name.clone(),
                None,
            ),
            Protocol::Github { user, repo } => (
                "github".to_string(),
                format!("github:{}/{}", user, repo),
                None,
            ),
            Protocol::Jsr => (
                "jsr".to_string(),
                String::new(),
                None,
            ),
        };

        Self {
            id: id.as_spec(),
            name: id.name().as_str().to_string(),
            version: id.version().to_string(),
            resolution: PackageResolution {
                r#type: res_type,
                url,
                registry,
            },
            integrity: resolution.integrity.clone(),
        }
    }

    /// Create from resolver's Resolution type
    pub fn from_resolver_resolution(res: &ResolverResolution) -> Self {
        Self {
            id: res.package_id.as_spec(),
            name: res.package_id.name().as_str().to_string(),
            version: res.version.to_string(),
            resolution: PackageResolution {
                r#type: "registry".to_string(),
                url: format!("https://registry.npmjs.org/{}/-/{}-{}.tgz", 
                    res.package_id.name().as_str(), 
                    res.package_id.name().as_str(), 
                    res.version),
                registry: Some("npm".to_string()),
            },
            integrity: Some(res.integrity.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mgpm_core::PackageName;
    use proptest::prelude::*;

    #[test]
    fn test_lockfile_new() {
        let lock = Lockfile::new(1, "npm");
        assert_eq!(lock.version, LOCKFILE_VERSION);
        assert_eq!(lock.metadata.registry, "npm");
    }

    #[test]
    fn test_lockfile_package_sort() {
        let mut lock = Lockfile::new(1, "npm");
        
        lock.add_package(LockfilePackage {
            id: "react@18.0.0".to_string(),
            name: "react".to_string(),
            version: "18.0.0".to_string(),
            resolution: PackageResolution {
                r#type: "registry".to_string(),
                url: "".to_string(),
                registry: None,
            },
            integrity: None,
        });
        
        lock.add_package(LockfilePackage {
            id: "react@17.0.0".to_string(),
            name: "react".to_string(),
            version: "17.0.0".to_string(),
            resolution: PackageResolution {
                r#type: "registry".to_string(),
                url: "".to_string(),
                registry: None,
            },
            integrity: None,
        });

        lock.sort_packages();
        
        assert_eq!(lock.packages[0].version, "17.0.0");
        assert_eq!(lock.packages[1].version, "18.0.0");
    }

    fn arb_package_name() -> impl Strategy<Value = String> {
        "[a-z]{3,10}"
    }

    fn arb_version() -> impl Strategy<Value = String> {
        (0u64..100, 0u64..100, 0u64..100)
            .prop_map(|(maj, min, pat)| format!("{}.{}.{}", maj, min, pat))
    }

    proptest! {
        #[test]
        fn proptest_lockfile_roundtrip(
            config_version in 0u32..10,
            packages in proptest::collection::vec(
                (arb_package_name(), arb_version()),
                0..10
            )
        ) {
            let mut lock = Lockfile::new(config_version, "npm");
            for (name, version) in &packages {
                lock.add_package(LockfilePackage {
                    id: format!("{}@{}", name, version),
                    name: name.clone(),
                    version: version.clone(),
                    resolution: PackageResolution {
                        r#type: "registry".to_string(),
                        url: format!("https://registry.npmjs.org/{}/-/{}-{}.tgz", name, name, version),
                        registry: Some("npm".to_string()),
                    },
                    integrity: Some(format!("sha512-{}", hex::encode(name))),
                });
            }
            lock.sort_packages();
            lock.compute_content_hash();

            // Serialize to JSON and back (simulates text roundtrip)
            let json = serde_json::to_string(&lock).unwrap();
            let deserialized: Lockfile = serde_json::from_str(&json).unwrap();

            assert_eq!(deserialized.version, lock.version);
            assert_eq!(deserialized.packages.len(), lock.packages.len());
            for (a, b) in deserialized.packages.iter().zip(lock.packages.iter()) {
                assert_eq!(a.name, b.name);
                assert_eq!(a.version, b.version);
                assert_eq!(a.integrity, b.integrity);
            }
        }
    }
}