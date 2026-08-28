//! OCI types — manifest, blobs, descriptors (OCI Distribution Spec subset)
//! (OCI types: manifest, blobs, descriptors — OCI Distribution Spec subset)

use serde::{Deserialize, Serialize};

/// OCI Image Manifest v1 (schemaVersion 2)
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl OciManifest {
    pub fn new(config: OciDescriptor, layers: Vec<OciDescriptor>) -> Self {
        Self {
            schema_version: 2,
            media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
            config,
            layers,
            subject: None,
            annotations: std::collections::HashMap::new(),
        }
    }
}

/// OCI Descriptor (common for config, layers, blobs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciDescriptor {
    pub media_type: String,
    pub size: i64,
    pub digest: String, // sha256:...
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<u8>>, // optional inline data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urls: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<std::collections::HashMap<String, String>>,
}

impl OciDescriptor {
    pub fn new(media_type: String, size: i64, digest: String) -> Self {
        Self {
            media_type,
            size,
            digest,
            data: None,
            urls: None,
            annotations: None,
        }
    }

    pub fn with_data(mut self, data: Vec<u8>) -> Self {
        self.data = Some(data);
        self
    }

    pub fn with_urls(mut self, urls: Vec<String>) -> Self {
        self.urls = Some(urls);
        self
    }

    pub fn with_annotations(
        mut self,
        annotations: std::collections::HashMap<String, String>,
    ) -> Self {
        self.annotations = Some(annotations);
        self
    }
}

/// OCI Image Config (for model metadata)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciImageConfig {
    pub created: String,
    pub architecture: String,
    pub os: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<OciConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rootfs: Option<OciRootFs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<OciHistory>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposed_ports: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmd: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volumes: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciRootFs {
    pub r#type: String, // "layers"
    pub diff_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciHistory {
    pub created: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_layer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// Media types (OCI constants)
pub mod media_types {
    pub const OCI_MANIFEST_V1: &str = "application/vnd.oci.image.manifest.v1+json";
    pub const OCI_CONFIG: &str = "application/vnd.oci.image.config.v1+json";
    pub const OCI_LAYER_GZIP: &str = "application/vnd.oci.image.layer.v1.tar+gzip";
    pub const OCI_LAYER_ZSTD: &str = "application/vnd.oci.image.layer.v1.tar+zstd";
    pub const OCI_EMPTY_JSON: &str = "application/vnd.oci.empty.v1+json";
}
