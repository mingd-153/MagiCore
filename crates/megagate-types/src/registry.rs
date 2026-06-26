use crate::package::PeerDepMeta;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPackageMetadata {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub dist: RegistryDist,
    pub dependencies: Option<HashMap<String, String>>,
    pub dev_dependencies: Option<HashMap<String, String>>,
    pub optional_dependencies: Option<HashMap<String, String>>,
    pub peer_dependencies: Option<HashMap<String, String>>,
    pub peer_dependencies_meta: Option<HashMap<String, PeerDepMeta>>,
    pub bin: Option<HashMap<String, String>>,
    pub engines: Option<HashMap<String, String>>,
    pub publish_time: Option<String>,
    pub versions: Option<HashMap<String, RegistryPackageVersion>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryDist {
    pub integrity: String,
    pub shasum: String,
    pub tarball: String,
    pub file_count: Option<u32>,
    pub unpacked_size: Option<u64>,
    pub signatures: Option<Vec<RegistrySignature>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrySignature {
    pub keyid: String,
    pub sig: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPackageVersion {
    pub name: String,
    pub version: String,
    pub dist: RegistryDist,
    pub dependencies: Option<HashMap<String, String>>,
    pub peer_dependencies: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryVersionsResponse {
    pub versions: HashMap<String, RegistryPackageVersion>,
    pub dist_tags: HashMap<String, String>,
    pub time: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreInitRequest {
    pub store_dir: String,
}