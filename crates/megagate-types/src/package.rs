use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use semver::Version;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRef {
    pub name: String,
    pub version: Version,
}

impl PackageRef {
    pub fn new(name: String, version: Version) -> Self {
        Self { name, version }
    }

    pub fn key(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManifest {
    pub name: String,
    pub version: Version,
    pub description: Option<String>,
    pub dependencies: HashMap<String, String>,
    #[serde(rename = "devDependencies", default)]
    pub dev_dependencies: HashMap<String, String>,
    #[serde(rename = "optionalDependencies", default)]
    pub optional_dependencies: HashMap<String, String>,
    #[serde(rename = "peerDependencies", default)]
    pub peer_dependencies: HashMap<String, String>,
    #[serde(rename = "peerDependenciesMeta", default)]
    pub peer_dependencies_meta: HashMap<String, PeerDepMeta>,
    #[serde(default)]
    pub bin: HashMap<String, String>,
    #[serde(default)]
    pub scripts: HashMap<String, String>,
    #[serde(default)]
    pub engines: HashMap<String, String>,
    #[serde(default)]
    pub files: Vec<String>,
    pub main: Option<String>,
    pub module: Option<String>,
    pub types: Option<String>,
    pub exports: Option<serde_json::Value>,
    pub side_effects: Option<serde_json::Value>,
    pub megagate: Option<MegagatePackageConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerDepMeta {
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MegagatePackageConfig {
    pub lockdown: Option<bool>,
    pub entry_points: Option<Vec<String>>,
    pub test_entry_points: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPackage {
    pub name: String,
    pub version: Version,
    pub integrity: String,
    pub resolved: String,
    pub size: u64,
    pub dependencies: HashMap<String, String>,
    pub optional_dependencies: HashMap<String, String>,
    pub peer_dependencies: HashMap<String, String>,
    pub bin: HashMap<String, String>,
    pub engines: HashMap<String, String>,
    pub provenance: Option<ProvenanceInfo>,
    pub approved_builds: Vec<String>,
    pub publish_time: Option<DateTime<Utc>>,
}

impl LockedPackage {
    pub fn key(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceInfo {
    pub repository_url: Option<String>,
    pub commit_hash: Option<String>,
    pub builder_id: Option<String>,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedDependency {
    pub name: String,
    pub version: Version,
    pub integrity: String,
    pub resolved: String,
    pub size: u64,
    pub dependencies: HashMap<String, String>,
    pub optional_dependencies: HashMap<String, String>,
    pub peer_dependencies: HashMap<String, String>,
    pub bin: HashMap<String, String>,
    pub engines: HashMap<String, String>,
    pub publish_time: Option<DateTime<Utc>>,
}

impl From<LockedPackage> for ResolvedDependency {
    fn from(pkg: LockedPackage) -> Self {
        Self {
            name: pkg.name,
            version: pkg.version,
            integrity: pkg.integrity,
            resolved: pkg.resolved,
            size: pkg.size,
            dependencies: pkg.dependencies,
            optional_dependencies: pkg.optional_dependencies,
            peer_dependencies: pkg.peer_dependencies,
            bin: pkg.bin,
            engines: pkg.engines,
            publish_time: pkg.publish_time,
        }
    }
}