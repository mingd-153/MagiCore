//! Chunked upload resumable — PATCH + PUT finalize (12 §11, 17 §3)
//! (Upload engine cho OCI /v2 blob upload: resumable, digest verify)

use crate::methods::HttpClient;
use anyhow::{bail, Result};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

/// Upload session state
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UploadSession {
    pub upload_id: String,
    pub offset: u64,
    pub total_size: Option<u64>,
    pub digest: String,          // sha256 của toàn bộ blob
    pub uploaded_digest: String, // sha256 của phần đã upload
    pub started_at: String,      // ISO8601
    pub updated_at: String,
    pub chunk_size: u64,
}

/// Chunked upload engine cho OCI /v2
pub struct ChunkedUploader {
    client: HttpClient,
    base_url: String,
    chunk_size: u64, // mặc định 10 MiB
    timeout_per_chunk: Duration,
}

impl ChunkedUploader {
    pub fn new(client: HttpClient, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            chunk_size: 10 * 1024 * 1024, // 10 MiB
            timeout_per_chunk: Duration::from_secs(60),
        }
    }

    pub fn with_chunk_size(mut self, size: u64) -> Self {
        self.chunk_size = size;
        self
    }

    /// Khởi tạo upload session → nhận upload_id
    pub async fn start_upload(&self, repo: &str) -> Result<String> {
        let url = format!(
            "{}/v2/{}/blobs/uploads/",
            self.base_url.trim_end_matches('/'),
            repo
        );
        let resp = self.client.post(&url, Vec::new()).await?;
        if !resp.status().is_success() {
            bail!("start upload failed: {}", resp.status());
        }
        // Server trả Location header với upload_id
        let location = resp
            .headers()
            .get("location")
            .ok_or_else(|| anyhow::anyhow!("missing Location header"))?
            .to_str()?;
        // Extract upload_id từ URL: .../blobs/uploads/{id}
        let upload_id = location
            .split('/')
            .last()
            .ok_or_else(|| anyhow::anyhow!("invalid Location: {}", location))?
            .to_string();
        Ok(upload_id)
    }

    /// Upload chunk (PATCH) — resumable
    pub async fn upload_chunk(
        &self,
        repo: &str,
        upload_id: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<u64> {
        let url = format!(
            "{}/v2/{}/blobs/uploads/{}",
            self.base_url.trim_end_matches('/'),
            repo,
            upload_id
        );
        let end = offset + data.len() as u64 - 1;
        let _range = format!("bytes={}-{}", offset, end);

        let resp = self
            .client
            .patch_with_timeout(&url, data.to_vec(), self.timeout_per_chunk)
            .await?;

        if resp.status().as_u16() == 308 {
            // Resume incomplete - server trả Range header
            let range_header = resp
                .headers()
                .get("range")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("bytes=0-");
            // Parse next offset từ Range: bytes=0-{next}
            let next_offset = range_header
                .strip_prefix("bytes=0-")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(offset + data.len() as u64);
            return Ok(next_offset);
        }

        if !resp.status().is_success() {
            bail!("upload chunk failed: {}", resp.status());
        }
        Ok(offset + data.len() as u64)
    }

    /// Finalize upload — PUT với digest query param
    pub async fn finalize_upload(
        &self,
        repo: &str,
        upload_id: &str,
        expected_digest: &str, // "sha256:..."
    ) -> Result<String> {
        let url = format!(
            "{}/v2/{}/blobs/uploads/{}?digest={}",
            self.base_url.trim_end_matches('/'),
            repo,
            upload_id,
            expected_digest
        );
        let resp = self.client.put(&url, Vec::new()).await?;
        if !resp.status().is_success() {
            bail!("finalize upload failed: {}", resp.status());
        }
        // Server trả Location với digest
        let location = resp
            .headers()
            .get("location")
            .ok_or_else(|| anyhow::anyhow!("missing Location after finalize"))?
            .to_str()?;
        Ok(location.to_string())
    }

    /// Upload file hoàn chỉnh từ đường dẫn — auto chunk + resume
    pub async fn upload_file(&self, repo: &str, file_path: &Path) -> Result<String> {
        let file = File::open(file_path).map_err(|e| anyhow::anyhow!("open file: {}", e))?;
        let file_size = file.metadata()?.len();

        // Compute full SHA256
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 8192];
        let mut file_clone = File::open(file_path)?;
        loop {
            let n = file_clone.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let digest = format!("sha256:{}", hex::encode(hasher.finalize()));

        // Start session
        let upload_id: String = self.start_upload(repo).await?;

        // Check for existing session (resume)
        let mut offset = 0u64;
        let mut file = file;

        loop {
            let mut chunk = vec![0u8; self.chunk_size as usize];
            let n = file.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            chunk.truncate(n);

            offset = self.upload_chunk(repo, &upload_id, offset, &chunk).await?;
            tracing::info!("Uploaded {} / {} bytes", offset, file_size);
        }

        // Finalize
        let location = self.finalize_upload(repo, &upload_id, &digest).await?;
        Ok(location)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uploader_creation() {
        let client = HttpClient::new().unwrap();
        let uploader = ChunkedUploader::new(client, "http://localhost:4315");
        assert_eq!(uploader.chunk_size, 10 * 1024 * 1024);
    }
}
