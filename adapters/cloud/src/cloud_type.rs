//! Cloud type detection for mgc-cloud-adapter.
//! Tách nhận diện cloud provider/runtime khỏi adapter chính.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudType {
    Cdk,
    Pulumi,
    Terraform,
    Cloudflare,
}

impl CloudType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CloudType::Cdk => "cdk",
            CloudType::Pulumi => "pulumi",
            CloudType::Terraform => "terraform",
            CloudType::Cloudflare => "cloudflare",
        }
    }
}

pub fn detect_type(root: &Path) -> Option<CloudType> {
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(t) = v
                .get("cloud")
                .and_then(|c| c.get("type"))
                .and_then(|t| t.as_str())
            {
                return match t {
                    "cdk" => Some(CloudType::Cdk),
                    "pulumi" => Some(CloudType::Pulumi),
                    "terraform" => Some(CloudType::Terraform),
                    "cloudflare" => Some(CloudType::Cloudflare),
                    _ => None,
                };
            }
        }
    }
    if root.join("wrangler.toml").exists() {
        return Some(CloudType::Cloudflare);
    }
    if root.join("Pulumi.yaml").exists() {
        return Some(CloudType::Pulumi);
    }
    if has_tf_files(root) {
        return Some(CloudType::Terraform);
    }
    if let Ok(content) = std::fs::read_to_string(root.join("package.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            let has_cdk = v
                .get("dependencies")
                .and_then(|d| d.as_object())
                .map(|deps| deps.keys().any(|k| k.starts_with("aws-cdk") || k == "cdk"))
                .unwrap_or(false);
            if has_cdk {
                return Some(CloudType::Cdk);
            }
        }
    }
    None
}

fn has_tf_files(root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries
        .flatten()
        .any(|e| e.path().extension().is_some_and(|ext| ext == "tf"))
}

pub(crate) fn manifest_is_cloud(root: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco == "cloud" {
                    return true;
                }
            }
            if v.get("cloud").is_some() {
                return true;
            }
        }
    }
    detect_type(root).is_some()
}
