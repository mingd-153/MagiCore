//! OCI reference — parse `repo:tag` / `repo@sha256:digest` (17 §4)
//! (Tham chiếu OCI: tên repository + tag/digest, dùng cho model push/pull)

use anyhow::{bail, Result};

/// Reference tới một artifact: tag (bền, có thể ghi đè) hoặc digest (bất biến).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OciReference {
    Tag(String),
    Digest(String),
}

/// Tham chiếu đầy đủ: repository + reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OciRef {
    pub repo: String,
    pub reference: OciReference,
}

impl OciRef {
    /// Parse `repo:tag` hoặc `repo@sha256:...` (2 segments — bỏ registry prefix).
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        if input.is_empty() {
            bail!("OCI reference is empty");
        }
        // tách phần cuối: @digest ưu tiên (digest chứa ':')
        if let Some((repo, digest)) = input.rsplit_once('@') {
            if repo.is_empty() || digest.is_empty() {
                bail!("invalid OCI reference '{input}': empty repo or digest");
            }
            return Ok(Self {
                repo: repo.to_string(),
                reference: OciReference::Digest(digest.to_string()),
            });
        }
        if let Some((repo, tag)) = input.rsplit_once(':') {
            // ponytail: registry có port ("host:5000/repo:tag") sẽ nhầm port thành tag —
            // đủ tốt cho CLI model push/pull; thêm RFC6838 registry-parsing khi cần.
            if repo.is_empty() || tag.is_empty() {
                bail!("invalid OCI reference '{input}': empty repo or tag");
            }
            return Ok(Self {
                repo: repo.to_string(),
                reference: OciReference::Tag(tag.to_string()),
            });
        }
        // không có reference → tag "latest"
        Ok(Self {
            repo: input.to_string(),
            reference: OciReference::Tag("latest".to_string()),
        })
    }

    /// Chuỗi reference thuần (không repo): tag hoặc digest.
    pub fn reference_str(&self) -> &str {
        match &self.reference {
            OciReference::Tag(t) => t,
            OciReference::Digest(d) => d,
        }
    }

    /// Dạng đầy đủ: `repo:tag` / `repo@sha256:...`
    pub fn to_string_full(&self) -> String {
        match &self.reference {
            OciReference::Tag(t) => format!("{}:{}", self.repo, t),
            OciReference::Digest(d) => format!("{}@{}", self.repo, d),
        }
    }
}
