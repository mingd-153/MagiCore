//! OCI Client — pull/push blobs & manifests (OCI Distribution Spec)
//! (OCI client: blob pull/push, manifest pull/push — OCI Distribution Spec subset)

use crate::manifest::{OciDescriptor, OciManifest};
use anyhow::{bail, Result};
use mg_http::{timeout::TimeoutConfig, HttpClient, TlsConfig};
use serde_json;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// OCI Client for registry operations
pub struct OciClient {
    client: HttpClient,
    base_url: String,
}

impl OciClient {
    pub fn new(registry_url: impl Into<String>, tls: Option<TlsConfig>) -> Result<Self> {
        let url = registry_url.into();
        let tls = tls.unwrap_or_default();
        let timeout = TimeoutConfig::default();
        let http = HttpClient::with_security(&timeout, &tls)?.with_retry(
            mg_http::retry::RetryStrategy::Exponential {
                base: std::time::Duration::from_secs(1),
                max: std::time::Duration::from_secs(30),
            },
        );
        Ok(Self {
            client: http,
            base_url: url,
        })
    }

    /// Bearer token cho registry private (fail-closed khi server cấu hình admin token)
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.client = self
            .client
            .with_auth("authorization", format!("Bearer {}", token.into()));
        self
    }

    /// List repositories (GET /v2/_catalog)
    pub async fn list_repositories(&self) -> Result<Vec<String>> {
        let url = format!("{}/v2/_catalog", self.base_url.trim_end_matches('/'));
        let resp = self.client.get(&url).await?;
        if !resp.status().is_success() {
            bail!("Catalog failed: {}", resp.status());
        }
        let json: serde_json::Value = resp.json().await?;
        Ok(json["repositories"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// List tags (GET /v2/<repo>/tags/list)
    pub async fn list_tags(&self, repo: &str) -> Result<Vec<String>> {
        let url = format!(
            "{}/v2/{}/tags/list",
            self.base_url.trim_end_matches('/'),
            repo
        );
        let resp = self.client.get(&url).await?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let json: serde_json::Value = resp.json().await?;
        Ok(json["tags"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Get blob by digest (HEAD first to check existence, then GET)
    pub async fn pull_blob(&self, repo: &str, digest: &str) -> Result<Vec<u8>> {
        let url = format!(
            "{}/v2/{}/blobs/{}",
            self.base_url.trim_end_matches('/'),
            repo,
            digest
        );

        // First check if blob exists (HEAD)
        let head = self.client.get(&url).await?;
        if head.status().as_u16() == 404 {
            bail!("Blob not found: {}", digest);
        }
        if !head.status().is_success() {
            bail!("HEAD blob failed: {}", head.status());
        }

        // GET the blob
        let resp = self.client.get(&url).await?;
        if !resp.status().is_success() {
            bail!("GET blob failed: {}", resp.status());
        }

        let data = resp.bytes().await?.to_vec();

        // Verify digest
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let computed = format!("sha256:{}", hex::encode(hasher.finalize()));
        if computed != digest {
            bail!("Digest mismatch: expected {}, got {}", digest, computed);
        }

        Ok(data)
    }

    /// Push blob (initiate upload, upload chunks, finalize)
    pub async fn push_blob(&self, repo: &str, data: &[u8]) -> Result<String> {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let digest = format!("sha256:{}", hex::encode(hasher.finalize()));

        // Check if already exists
        if self.blob_exists(repo, &digest).await? {
            return Ok(digest);
        }

        // Write data to temp file for upload
        let temp_dir = tempfile::tempdir()?;
        let temp_file = temp_dir.path().join("blob.bin");
        std::fs::write(&temp_file, data)?;

        // Start upload
        let uploader =
            mg_http::upload::ChunkedUploader::new(self.client.clone(), self.base_url.clone());
        uploader.upload_file(repo, &temp_file).await?;

        Ok(digest)
    }

    async fn blob_exists(&self, repo: &str, digest: &str) -> Result<bool> {
        let url = format!(
            "{}/v2/{}/blobs/{}",
            self.base_url.trim_end_matches('/'),
            repo,
            digest
        );
        match self.client.get(&url).await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Pull manifest
    pub async fn pull_manifest(&self, repo: &str, reference: &str) -> Result<OciManifest> {
        let url = format!(
            "{}/v2/{}/manifests/{}",
            self.base_url.trim_end_matches('/'),
            repo,
            reference
        );

        let resp = self.client.get(&url).await?;
        if !resp.status().is_success() {
            bail!("Pull manifest failed: {}", resp.status());
        }

        let manifest: OciManifest = resp.json().await?;
        Ok(manifest)
    }

    /// Push manifest
    pub async fn push_manifest(
        &self,
        repo: &str,
        reference: &str,
        manifest: &OciManifest,
    ) -> Result<()> {
        let url = format!(
            "{}/v2/{}/manifests/{}",
            self.base_url.trim_end_matches('/'),
            repo,
            reference
        );

        let body = serde_json::to_vec(manifest)?;

        let resp = self.client.put(&url, body).await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await?;
            bail!("Push manifest failed: {} - {}", status, text);
        }
        Ok(())
    }

    /// Push model with config + layers (convenience method for AI models)
    pub async fn push_model(
        &self,
        repo: &str,
        tag: &str,
        config: &crate::manifest::OciImageConfig,
        layers: &[(PathBuf, String)], // (file_path, media_type)
    ) -> Result<String> {
        // Push layers
        let mut layer_descriptors = Vec::new();
        for (path, media_type) in layers {
            let data = std::fs::read(path)?;
            let digest = self.push_blob(repo, &data).await?;
            let size = data.len() as i64;
            let desc = OciDescriptor::new(media_type.to_string(), size, digest);
            layer_descriptors.push(desc);
        }

        // Create config
        let config_data = serde_json::to_vec(config)?;
        let config_digest = self.push_blob(repo, &config_data).await?;
        let config_desc = OciDescriptor::new(
            "application/vnd.oci.image.config.v1+json".to_string(),
            config_data.len() as i64,
            config_digest,
        );

        // Create manifest
        let manifest = OciManifest::new(config_desc, layer_descriptors);

        // Push manifest
        let tag = tag.to_string();
        // Push manifest with repo and tag
        self.push_manifest(repo, &tag, &manifest).await?;

        Ok(tag)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn manifest_creation() {
        let config = crate::manifest::OciDescriptor::new(
            "application/vnd.oci.image.config.v1+json".to_string(),
            100,
            "sha256:config123".to_string(),
        );
        let layer = crate::manifest::OciDescriptor::new(
            "application/vnd.oci.image.layer.v1.tar+gzip".to_string(),
            1000,
            "sha256:layer456".to_string(),
        );
        let manifest = crate::manifest::OciManifest::new(config, vec![layer]);
        assert_eq!(manifest.config.digest, "sha256:config123");
        assert_eq!(manifest.layers.len(), 1);
    }
}
