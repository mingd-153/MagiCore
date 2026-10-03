//! OCI types — manifest, blobs, descriptors (OCI Distribution Spec subset)
//! (OCI types: manifest, blobs, descriptors — OCI Distribution Spec subset)

use serde::{Deserialize, Serialize};

/// OCI Image Manifest v1 (schemaVersion 2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciManifest {
    #[serde(rename = "schemaVersion", alias = "schema_version")]
    pub schema_version: i32,
    #[serde(rename = "mediaType", alias = "media_type")]
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
    #[serde(rename = "mediaType", alias = "media_type")]
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
    #[serde(
        rename = "User",
        alias = "user",
        skip_serializing_if = "Option::is_none"
    )]
    pub user: Option<String>,
    #[serde(
        rename = "ExposedPorts",
        alias = "exposed_ports",
        skip_serializing_if = "Option::is_none"
    )]
    pub exposed_ports: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(rename = "Env", alias = "env", skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<String>>,
    #[serde(
        rename = "Entrypoint",
        alias = "entrypoint",
        skip_serializing_if = "Option::is_none"
    )]
    pub entrypoint: Option<Vec<String>>,
    #[serde(rename = "Cmd", alias = "cmd", skip_serializing_if = "Option::is_none")]
    pub cmd: Option<Vec<String>>,
    #[serde(
        rename = "Volumes",
        alias = "volumes",
        skip_serializing_if = "Option::is_none"
    )]
    pub volumes: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(
        rename = "WorkingDir",
        alias = "working_dir",
        skip_serializing_if = "Option::is_none"
    )]
    pub working_dir: Option<String>,
    #[serde(
        rename = "Labels",
        alias = "labels",
        skip_serializing_if = "Option::is_none"
    )]
    pub labels: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciRootFs {
    pub r#type: String, // "layers"
    #[serde(alias = "diffIds")]
    pub diff_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciHistory {
    pub created: String,
    #[serde(alias = "createdBy", skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(alias = "emptyLayer", skip_serializing_if = "Option::is_none")]
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::{OciConfig, OciDescriptor, OciHistory, OciImageConfig, OciManifest, OciRootFs};

    #[test]
    fn oci_wire_names_are_canonical_and_legacy_manifests_still_deserialize() {
        let manifest = OciManifest::new(
            OciDescriptor::new(
                "application/vnd.oci.image.config.v1+json".into(),
                2,
                "sha256:config".into(),
            ),
            vec![],
        );
        let canonical = serde_json::to_value(&manifest).unwrap();
        assert_eq!(canonical["schemaVersion"], 2);
        assert_eq!(
            canonical["mediaType"],
            "application/vnd.oci.image.manifest.v1+json"
        );
        assert_eq!(
            canonical["config"]["mediaType"],
            "application/vnd.oci.image.config.v1+json"
        );
        assert!(canonical.get("schema_version").is_none());
        assert!(canonical["config"].get("media_type").is_none());

        let legacy: OciManifest = serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "media_type": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "media_type": "application/vnd.oci.image.config.v1+json",
                "size": 2,
                "digest": "sha256:config"
            },
            "layers": []
        }))
        .unwrap();
        assert_eq!(legacy.schema_version, 2);
        assert_eq!(
            legacy.config.media_type,
            "application/vnd.oci.image.config.v1+json"
        );
    }

    #[test]
    fn oci_image_config_uses_schema_casing_and_reads_legacy_names() {
        let image_config = OciImageConfig {
            created: "2026-10-03T00:00:00Z".into(),
            architecture: "amd64".into(),
            os: "linux".into(),
            config: Some(OciConfig {
                user: Some("1000:1000".into()),
                exposed_ports: Some(std::collections::HashMap::from([(
                    "8080/tcp".into(),
                    serde_json::json!({}),
                )])),
                env: Some(vec!["MODE=prod".into()]),
                entrypoint: Some(vec!["/init".into()]),
                cmd: Some(vec!["serve".into()]),
                volumes: Some(std::collections::HashMap::from([(
                    "/data".into(),
                    serde_json::json!({}),
                )])),
                working_dir: Some("/app".into()),
                labels: Some(std::collections::HashMap::from([(
                    "org.example.name".into(),
                    "image".into(),
                )])),
            }),
            rootfs: Some(OciRootFs {
                r#type: "layers".into(),
                diff_ids: vec!["sha256:layer".into()],
            }),
            history: Some(vec![OciHistory {
                created: "2026-10-03T00:00:00Z".into(),
                created_by: Some("mgc".into()),
                empty_layer: Some(false),
                comment: None,
            }]),
            annotations: None,
        };
        let value = serde_json::to_value(&image_config).unwrap();
        assert_eq!(value["config"]["User"], "1000:1000");
        assert!(value["config"].get("user").is_none());
        for key in [
            "ExposedPorts",
            "Env",
            "Entrypoint",
            "Cmd",
            "Volumes",
            "WorkingDir",
            "Labels",
        ] {
            assert!(value["config"].get(key).is_some(), "missing {key}");
        }
        assert_eq!(value["rootfs"]["diff_ids"][0], "sha256:layer");
        assert!(value["rootfs"].get("diffIds").is_none());
        assert_eq!(value["history"][0]["created_by"], "mgc");
        assert_eq!(value["history"][0]["empty_layer"], false);

        let legacy: OciImageConfig = serde_json::from_value(serde_json::json!({
            "created": "2026-10-03T00:00:00Z",
            "architecture": "amd64",
            "os": "linux",
            "config": {
                "user": "1000:1000",
                "exposed_ports": {"8080/tcp": {}},
                "env": ["MODE=prod"],
                "entrypoint": ["/init"],
                "cmd": ["serve"],
                "volumes": {"/data": {}},
                "working_dir": "/app",
                "labels": {"org.example.name": "image"}
            },
            "rootfs": {"type": "layers", "diffIds": ["sha256:layer"]},
            "history": [{
                "created": "2026-10-03T00:00:00Z",
                "createdBy": "mgc",
                "emptyLayer": false
            }]
        }))
        .unwrap();
        assert_eq!(legacy.config.unwrap().working_dir.as_deref(), Some("/app"));
        assert_eq!(legacy.rootfs.unwrap().diff_ids, ["sha256:layer"]);
        assert_eq!(
            legacy.history.unwrap()[0].created_by.as_deref(),
            Some("mgc")
        );
    }
}
