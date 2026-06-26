use crate::error::Result;
use crate::package::{LockedPackage};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockfileV1 {
    pub version: u32,
    pub lockfile_version: u32,
    pub packages: HashMap<String, LockedPackage>,
    pub importers: HashMap<String, ImporterDeps>,
    pub store: StoreInfo,
    pub metadata: LockfileMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreInfo {
    pub dir: String,
    pub layout_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockfileMetadata {
    pub created_at: DateTime<Utc>,
    pub megagate_version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImporterDeps {
    pub dependencies: HashMap<String, String>,
    pub dev_dependencies: HashMap<String, String>,
    pub optional_dependencies: HashMap<String, String>,
}

impl LockfileV1 {
    pub fn new(megagate_version: String) -> Self {
        let now = Utc::now();
        let empty_hash = Sha256::digest(b"").to_vec();
        Self {
            version: 1,
            lockfile_version: 1,
            packages: HashMap::new(),
            importers: HashMap::new(),
            store: StoreInfo {
                dir: "~/.megagate/store".to_string(),
                layout_version: 1,
            },
            metadata: LockfileMetadata {
                created_at: now,
                megagate_version,
                content_hash: hex::encode(empty_hash),
            },
        }
    }

    pub fn compute_content_hash(&self) -> String {
        let mut hasher = Sha256::new();
        let mut keys: Vec<_> = self.packages.keys().collect();
        keys.sort();
        for key in keys {
            let pkg = &self.packages[key];
            hasher.update(key.as_bytes());
            hasher.update(pkg.integrity.as_bytes());
            hasher.update(pkg.version.to_string().as_bytes());
        }
        let mut importer_keys: Vec<_> = self.importers.keys().collect();
        importer_keys.sort();
        for key in importer_keys {
            let importer = &self.importers[key];
            let mut dep_keys: Vec<_> = importer.dependencies.keys().collect();
            dep_keys.sort();
            for dep in dep_keys {
                hasher.update(dep.as_bytes());
                hasher.update(importer.dependencies[dep].as_bytes());
            }
        }
        hex::encode(hasher.finalize())
    }

    pub fn verify_content_hash(&self) -> Result<bool> {
        Ok(self.metadata.content_hash == self.compute_content_hash())
    }

    pub fn get_package(&self, name: &str, version: &semver::Version) -> Option<&LockedPackage> {
        self.packages.get(&format!("{}@{}", name, version))
    }

    pub fn add_package(&mut self, pkg: LockedPackage) {
        self.packages.insert(pkg.key(), pkg);
    }

    pub fn add_importer(&mut self, path: String, deps: ImporterDeps) {
        self.importers.insert(path, deps);
    }
}

impl Default for LockfileV1 {
    fn default() -> Self {
        Self::new(env!("CARGO_PKG_VERSION").to_string())
    }
}
