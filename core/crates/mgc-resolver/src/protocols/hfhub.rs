//! HuggingFace Hub model registry protocol (Phase E slice, AI models).
//! Protocol registry model HuggingFace Hub.
//!
//! Models are NOT packages: one revision holds MANY files, so this engine
//! deliberately does NOT implement `RegistryProtocol` (forcing models
//! into single-artifact entries would fake package semantics). Integrity
//! is per file: LFS `lfs.oid` (sha256) preferred, else the git blob id
//! (`sha1("blob {len}\\0{bytes}")`, recomputed at verify).
//! (Model KHÔNG phải package: một revision có NHIỀU file, nên engine này
//! cố ý KHÔNG implement `RegistryProtocol`.)
//!
//! Revision discipline: an exact 40-hex commit SHA is REQUIRED. Branch
//! and tag names are mutable refs — resolving them would pin a moving
//! target, which a lockfile must never do.
//! (Kỷ luật revision: BẮT BUỘC commit SHA 40-hex chính xác.)

use mgc_types::{MgError, MgResult};
use serde_json::Value;

const DEFAULT_API_URL: &str = "https://huggingface.co";

/// One locked model file — Một file model đã lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFile {
    /// Repo-relative path — Đường dẫn tương đối trong repo.
    pub path: String,
    /// Byte size when the registry reports it — Kích thước khi registry báo.
    pub size: Option<u64>,
    /// LFS sha256 (`lfs.oid`), when present — sha256 LFS nếu có.
    pub sha256: Option<String>,
    /// Git blob id (hex40) — always present, fallback integrity.
    pub git_blob_sha1: String,
    /// Direct download URL for the pinned revision.
    pub url: String,
}

/// A resolved model revision — Một revision model đã resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResolution {
    /// `org/name` model id — Id model.
    pub model: String,
    /// Pinned 40-hex commit — Commit đã ghim.
    pub commit: String,
    /// Locked files — Các file đã lock.
    pub files: Vec<ModelFile>,
}

pub struct HfHubProtocol {
    base_url: String,
    client: reqwest::Client,
    token: Option<String>,
}

impl HfHubProtocol {
    /// Build with an explicit Hub base URL.
    /// Dựng với URL gốc Hub tường minh.
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
            token: std::env::var("MGC_HF_TOKEN").ok().filter(|v| !v.is_empty()),
        }
    }

    /// Build from environment (`MGC_HF_API_URL`, `MGC_HF_TOKEN`).
    /// Dựng từ môi trường.
    pub fn from_env() -> Self {
        let base_url = std::env::var("MGC_HF_API_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_API_URL.to_string());
        Self::new(&base_url)
    }

    /// Validate a `org/name` model id (fail-closed on traversal/URLs).
    /// Kiểm tra id model (fail-closed với traversal/URL).
    fn check_model_id(model: &str) -> MgResult<()> {
        let valid = !model.is_empty()
            && model.matches('/').count() == 1
            && !model.contains("..")
            && !model.contains("://")
            && model
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
        if valid {
            Ok(())
        } else {
            Err(MgError::Other(format!(
                "invalid model id '{model}' — expected org/name (fail-closed)"
            )))
        }
    }

    /// Revision must be an exact commit SHA (mutable refs never pin).
    /// Revision phải là commit SHA chính xác.
    fn check_revision(revision: &str) -> MgResult<()> {
        if revision.len() == 40 && revision.chars().all(|c| c.is_ascii_hexdigit()) {
            Ok(())
        } else {
            Err(MgError::Other(format!(
                "revision '{revision}' is not an exact 40-hex commit SHA — branch/tag names are mutable refs and cannot lock (fail-closed)"
            )))
        }
    }

    /// Resolve a model revision to its locked file list via the tree API.
    /// Resolve revision model thành danh sách file đã lock qua tree API.
    pub async fn resolve_model(&self, model: &str, revision: &str) -> MgResult<ModelResolution> {
        Self::check_model_id(model)?;
        Self::check_revision(revision)?;
        let url = format!(
            "{}/api/models/{model}/tree/{revision}?recursive=true",
            self.base_url
        );
        let mut request = self.client.get(&url);
        if let Some(token) = self.token.as_deref() {
            request = request.bearer_auth(token);
        }
        let resp = request
            .send()
            .await
            .map_err(|e| MgError::Network(format!("GET {url} failed: {e}")))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| MgError::Network(format!("read {url} body failed: {e}")))?;
        if !status.is_success() {
            return Err(MgError::Network(format!(
                "GET {url} returned {status} — model tree required (fail-closed)"
            )));
        }
        let entries: Vec<Value> = serde_json::from_str(&body)
            .map_err(|e| MgError::Other(format!("parse model tree failed: {e}")))?;
        let mut files = Vec::new();
        for entry in &entries {
            if entry.get("type").and_then(Value::as_str) != Some("file") {
                continue;
            }
            let path = entry.get("path").and_then(Value::as_str).ok_or_else(|| {
                MgError::Other("model tree file entry without path (fail-closed)".to_string())
            })?;
            if path.contains("..") {
                return Err(MgError::Other(format!(
                    "model tree path escapes the repo: '{path}' (fail-closed)"
                )));
            }
            let git_blob_sha1 = entry
                .get("oid")
                .and_then(Value::as_str)
                .filter(|oid| oid.len() == 40 && oid.chars().all(|c| c.is_ascii_hexdigit()))
                .ok_or_else(|| {
                    MgError::Other(format!(
                        "model tree file '{path}' has no hex40 blob oid (fail-closed)"
                    ))
                })?
                .to_string();
            let (sha256, size) = match entry.get("lfs") {
                Some(lfs) => {
                    let oid = lfs.get("oid").and_then(Value::as_str).ok_or_else(|| {
                        MgError::Other(format!("LFS file '{path}' without oid (fail-closed)"))
                    })?;
                    if oid.len() != 64 || !oid.chars().all(|c| c.is_ascii_hexdigit()) {
                        return Err(MgError::Other(format!(
                            "LFS oid for '{path}' is not a sha256 (fail-closed)"
                        )));
                    }
                    (
                        Some(oid.to_string()),
                        lfs.get("size").and_then(Value::as_u64),
                    )
                }
                None => (None, entry.get("size").and_then(Value::as_u64)),
            };
            files.push(ModelFile {
                // Canonical file-download shape:
                // `{base}/{model}/resolve/{revision}/{path}`.
                url: format!("{}/{model}/resolve/{revision}/{path}", self.base_url),
                path: path.to_string(),
                size,
                sha256,
                git_blob_sha1,
            });
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(ModelResolution {
            model: model.to_string(),
            commit: revision.to_string(),
            files,
        })
    }

    /// Download one locked file (auth header only when configured).
    /// Tải một file đã lock.
    pub async fn download_file(&self, file: &ModelFile) -> MgResult<Vec<u8>> {
        let mut request = self.client.get(&file.url);
        if let Some(token) = self.token.as_deref() {
            request = request.bearer_auth(token);
        }
        let resp = request
            .send()
            .await
            .map_err(|e| MgError::Network(format!("GET {} failed: {e}", file.url)))?;
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| MgError::Network(format!("read {} body failed: {e}", file.url)))?;
        if !status.is_success() {
            return Err(MgError::Network(format!(
                "GET {} returned {status} (fail-closed)",
                file.url
            )));
        }
        Ok(bytes.to_vec())
    }

    /// Verify bytes: LFS sha256 when recorded, else recomputed git blob
    /// id. Mismatch or missing integrity always fails.
    /// Xác minh byte: sha256 LFS nếu có, không thì git blob id tính lại.
    pub fn verify_file(&self, file: &ModelFile, bytes: &[u8]) -> MgResult<()> {
        if let Some(expected) = file.sha256.as_deref() {
            let actual = super::sha256_hex(bytes);
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(MgError::Integrity(format!(
                    "sha256 mismatch for '{}': expected {}, got {}",
                    file.path, expected, actual
                )));
            }
            return Ok(());
        }
        let actual = git_blob_sha1(bytes);
        if actual != file.git_blob_sha1 {
            return Err(MgError::Integrity(format!(
                "git blob mismatch for '{}': expected {}, got {}",
                file.path, file.git_blob_sha1, actual
            )));
        }
        Ok(())
    }
}

/// Git blob id: `sha1("blob {len}\\0{bytes}")`, lowercase hex.
/// (Git blob id.)
pub fn git_blob_sha1(bytes: &[u8]) -> String {
    use sha1::Digest;
    let mut hasher = sha1::Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()));
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
