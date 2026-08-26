//! Model download từ HuggingFace, local, hoặc URL — kèm kiểm tra checksum bắt buộc.
// (Model download from HuggingFace, local, or URL — with mandatory checksum gate.)

use mgc_types::{MgError, MgResult};
use std::path::{Path, PathBuf};

/// Model source variants
#[derive(Debug, Clone)]
pub enum ModelSource {
    /// HuggingFace Hub (repo_id, revision, filename)
    HuggingFace {
        repo_id: String,
        revision: Option<String>,
        filename: String,
        checksum: Option<String>,
    },
    /// Local file path
    Local(PathBuf),
    /// Remote URL
    Url {
        url: String,
        checksum: Option<String>,
    },
}

impl ModelSource {
    pub fn checksum(&self) -> Option<&str> {
        match self {
            ModelSource::HuggingFace { checksum, .. } => checksum.as_deref(),
            ModelSource::Url { checksum, .. } => checksum.as_deref(),
            ModelSource::Local(_) => None,
        }
    }

    pub fn huggingface(repo_id: &str, filename: &str) -> Self {
        ModelSource::HuggingFace {
            repo_id: repo_id.to_string(),
            revision: None,
            filename: filename.to_string(),
            checksum: None,
        }
    }

    /// Khai báo checksum để bật kiểm tra toàn vẹn sau download.
    // (Declare a checksum to enable post-download integrity verification.)
    /// Format: `"sha256:<hex>"`, `"blake3:<hex>"`, hoặc bare hex (mặc định hiểu là blake3).
    // (Format: prefixed algorithm, bare hex defaults to blake3.)
    pub fn with_checksum(mut self, checksum: impl Into<String>) -> Self {
        let value = Some(checksum.into());
        match &mut self {
            ModelSource::HuggingFace { checksum, .. } | ModelSource::Url { checksum, .. } => {
                *checksum = value;
            }
            ModelSource::Local(_) => {}
        }
        self
    }

    pub fn url(url: &str) -> Self {
        ModelSource::Url {
            url: url.to_string(),
            checksum: None,
        }
    }
}

/// Download model từ source → target_dir. Trả về (path, bytes_downloaded).
/// Checksum khai báo trong source sẽ được kiểm tra ngay sau khi ghi file —
/// sai checksum → xoá file tải về + Err (fail-closed, không để file lạ tồn dư).
// (Checksum declared on the source is verified right after write — on mismatch the
// downloaded file is removed and an error returned.)
pub async fn download_model(
    _model_id: &str,
    source: &ModelSource,
    target_dir: &Path,
) -> MgResult<(PathBuf, u64)> {
    std::fs::create_dir_all(target_dir)?;

    let (dest, bytes) = match source {
        ModelSource::Local(src) => {
            let filename = src
                .file_name()
                .ok_or_else(|| MgError::Other("No filename in local path".into()))?;
            let dest = target_dir.join(filename);

            std::fs::copy(src, &dest)?;
            let bytes = std::fs::metadata(&dest)?.len();

            Ok((dest, bytes))
        }
        ModelSource::HuggingFace {
            repo_id,
            revision,
            filename,
            ..
        } => {
            let rev = revision.as_deref().unwrap_or("main");
            let url = format!(
                "https://huggingface.co/{}/resolve/{}/{}",
                repo_id, rev, filename
            );
            http_download(&url, &target_dir.join(filename)).await
        }
        ModelSource::Url { url, .. } => {
            let filename = filename_from_url(url);
            http_download(url, &target_dir.join(filename)).await
        }
    }?;

    match source.checksum() {
        Some(expected) => {
            // Sai checksum → xoá file + bail (tamper không được giữ lại trên disk)
            // (Checksum mismatch → delete file + bail so a tampered artifact never lingers)
            if let Err(e) = verify_file_checksum(&dest, expected) {
                let _ = std::fs::remove_file(&dest);
                return Err(e);
            }
        }
        None => {
            // Escape hatch có cảnh báo (RULE §11): thiếu checksum vẫn tải được nhưng phải lên tiếng
            // (Escape hatch with warning: downloads without checksum stay possible but loud)
            eprintln!(
                "warning: downloaded '{}' without checksum verification — supply a sha256/blake3 checksum for supply-chain safety",
                dest.display()
            );
        }
    }

    Ok((dest, bytes))
}

/// Tách tên file từ URL (dùng cho nhánh Url).
// (Derive destination filename from the URL tail.)
fn filename_from_url(url: &str) -> String {
    let tail = url.rsplit('/').next().unwrap_or_default();
    let clean = tail.split(['?', '#']).next().unwrap_or_default();
    if clean.is_empty() {
        "model.bin".to_string()
    } else {
        clean.to_string()
    }
}

/// GET url → ghi file. Dùng chung cho mọi nhánh remote (1 chỗ duy nhất xử lý HTTP).
// (GET url → write to dest. Single shared HTTP download path for all remote branches.)
pub(crate) async fn http_download(url: &str, dest: &Path) -> MgResult<(PathBuf, u64)> {
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| MgError::Network(format!("download failed: {e}")))?;

    if !response.status().is_success() {
        return Err(MgError::Network(format!(
            "HTTP {}: {url}",
            response.status()
        )));
    }

    // ponytail: full-buffer .bytes() — đủ cho model < RAM; streaming resume là P3
    // (ponytail: full-buffer .bytes() — fine below RAM-size models; streaming/resume deferred to P3)
    let bytes = response
        .bytes()
        .await
        .map_err(|e| MgError::Network(format!("download body failed: {e}")))?;

    std::fs::write(dest, &bytes)?;
    Ok((dest.to_path_buf(), bytes.len() as u64))
}

/// Kiểm tra checksum file theo prefix thuật toán: `sha256:`/`blake3:`/bare hex (=blake3).
/// (Verify file checksum by algorithm prefix: `sha256:`/`blake3:`; bare hex means blake3.)
pub fn verify_file_checksum(path: &Path, expected: &str) -> MgResult<()> {
    let expected = expected.trim();
    let (algo, want) = match expected.split_once(':') {
        Some(("sha256", rest)) => ("sha256", rest),
        Some(("blake3", rest)) => ("blake3", rest),
        Some((other, _)) => {
            return Err(MgError::Other(format!(
                "unsupported checksum algorithm '{other}' (expected 'sha256:' or 'blake3:')"
            )))
        }
        None => ("blake3", expected),
    };

    let content = std::fs::read(path)?;
    let computed = match algo {
        "sha256" => {
            use sha2::Digest;
            let digest = sha2::Sha256::digest(&content);
            hex::encode(digest)
        }
        _ => mgc_crypto::Blake3Hasher::hash_bytes(&content).to_hex(),
    };

    if computed.eq_ignore_ascii_case(want) {
        Ok(())
    } else {
        Err(MgError::Other(format!(
            "checksum mismatch ({algo}): expected {want}, got {computed}"
        )))
    }
}

#[cfg(test)]
#[path = "test/download_tests.rs"]
mod tests;
