//! Data models for registry server
//! (Model: packages, versions, dist-tags, blobs, OCI manifests)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// npmjs đưa `"deprecated": false` (không null) — chấp nhận bool/string/null
fn de_optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct OptStr;
    impl<'de> serde::Deserialize<'de> for OptStr {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            #[derive(serde::Deserialize)]
            #[serde(untagged)]
            enum V {
                S(String),
                B(bool),
            }
            match V::deserialize(deserializer)? {
                V::S(_) | V::B(_) => Ok(OptStr),
            }
        }
    }
    Ok(Option::<OptStr>::deserialize(deserializer)?.map(|_| String::new()))
}

/// Package metadata in registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub description: Option<String>,
    pub versions: HashMap<String, PackageVersion>,
    #[serde(rename = "dist-tags", default)]
    pub dist_tags: HashMap<String, String>, // tag -> version
    #[serde(default)]
    pub maintainers: Vec<Maintainer>,
    /// Flat map chuẩn npm: {"created": ..., "modified": ..., "<version>": "<iso>"}
    #[serde(default)]
    pub time: HashMap<String, String>,
    #[serde(default)]
    pub private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageVersion {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub dist: DistInfo,
    pub dependencies: Option<HashMap<String, String>>,
    pub dev_dependencies: Option<HashMap<String, String>>,
    pub peer_dependencies: Option<HashMap<String, String>>,
    pub optional_dependencies: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scripts: Option<HashMap<String, String>>,
    pub main: Option<String>,
    pub module: Option<String>,
    pub types: Option<String>,
    pub exports: Option<serde_json::Value>,
    pub publish_config: Option<PublishConfig>,
    #[serde(default, deserialize_with = "de_optional_string")]
    pub deprecated: Option<String>,
    pub license: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub author: Option<Author>,
    pub repository: Option<Repository>,
    pub bugs: Option<Bugs>,
    pub homepage: Option<String>,
    pub readme: Option<String>,
    #[serde(rename = "_id", default)]
    pub _id: String,
    #[serde(rename = "_rev", default)]
    pub _rev: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistInfo {
    pub integrity: String, // sha512-...
    pub shasum: String,    // sha1
    pub tarball: String,   // URL to tarball
    #[serde(rename = "fileCount", default)]
    pub file_count: Option<u32>,
    #[serde(rename = "unpackedSize", default)]
    pub unpacked_size: Option<u64>,
    #[serde(default)]
    pub signatures: Option<Vec<Signature>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub keyid: String,
    pub sig: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishConfig {
    pub access: Option<String>, // "public" | "restricted"
    pub registry: Option<String>,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Maintainer {
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    pub email: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub r#type: String,
    pub url: String,
    pub directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bugs {
    pub url: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub objects: Vec<SearchResultItem>,
    pub total: u64,
    pub time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub package: SearchPackage,
    pub score: f64,
    #[serde(rename = "searchScore")]
    pub search_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPackage {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub date: String,
    pub links: SearchLinks,
    pub publisher: SearchPublisher,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchLinks {
    pub npm: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub bugs: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPublisher {
    pub username: String,
    pub email: Option<String>,
}

// OCI models
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OciManifest {
    pub schema_version: i32,
    pub media_type: String,
    pub config: OciDescriptor,
    pub layers: Vec<OciDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<Box<OciDescriptor>>,
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty", default)]
    pub annotations: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OciDescriptor {
    pub media_type: String,
    pub size: i64,
    pub digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urls: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciBlobUploadResponse {
    pub location: String,
    pub range: Option<String>,
    pub docker_upload_uuid: String,
}

// PyPI models (PEP 691 simple API)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PypiFile {
    pub name: String,
    pub version: String,
    pub filename: String,
    pub digest: String, // sha256:<hex>
    pub size: i64,
    pub requires_python: Option<String>,
}

// Error responses
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    pub fn new(error: &str, message: &str) -> Self {
        Self {
            error: error.to_string(),
            message: message.to_string(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}
