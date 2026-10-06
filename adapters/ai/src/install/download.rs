//! Model download từ HuggingFace, local, hoặc URL; remote phải được pin bằng digest.
// (Model downloads from HuggingFace, local, or URL; remote sources require a pinned digest.)

use mgc_types::{MgError, MgResult};
use std::path::{Path, PathBuf};

const DEFAULT_MAX_REMOTE_MODEL_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_REMOTE_MODEL_BYTES_ENV: &str = "MGC_MAX_MODEL_DOWNLOAD_BYTES";

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

    /// Pin a Hugging Face source to an immutable 40-hex commit revision.
    /// Pin nguồn Hugging Face vào revision commit bất biến 40 ký tự hex.
    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        if let ModelSource::HuggingFace {
            revision: current, ..
        } = &mut self
        {
            *current = Some(revision.into());
        }
        self
    }
}

/// Download to same-directory staging, verify a declared checksum, then atomically publish.
/// Download vào staging cùng thư mục, verify checksum nếu có, rồi publish nguyên tử.
pub async fn download_model(
    _model_id: &str,
    source: &ModelSource,
    target_dir: &Path,
) -> MgResult<(PathBuf, u64)> {
    validate_remote_integrity(source)?;

    let (relative_path, remote_url, local_source) = match source {
        ModelSource::Local(src) => {
            let filename = src
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| MgError::Other("local model source has no valid filename".into()))?;
            (validate_model_relative_path(filename)?, None, Some(src))
        }
        ModelSource::HuggingFace {
            repo_id,
            revision,
            filename,
            ..
        } => {
            let relative = validate_model_relative_path(filename)?;
            let repo = validate_model_relative_path(repo_id)?;
            let rev = validate_model_relative_path(revision.as_deref().ok_or_else(|| {
                MgError::Integrity(
                    "Hugging Face model source requires an immutable commit revision".into(),
                )
            })?)?;
            let url = format!(
                "https://huggingface.co/{}/resolve/{}/{}",
                repo.display(),
                rev.display(),
                relative.display()
            );
            (relative, Some(url), None)
        }
        ModelSource::Url { url, .. } => {
            let filename = filename_from_url(url);
            (
                validate_model_relative_path(&filename)?,
                Some(url.clone()),
                None,
            )
        }
    };

    ensure_real_directory(target_dir)?;
    let dest = ensure_safe_model_destination(target_dir, &relative_path)?;
    let staging = unique_staging_path(&dest);
    let transfer = async {
        let bytes = if let Some(source) = local_source {
            copy_local_to_staging(source, &staging).await?
        } else {
            let url = remote_url
                .as_deref()
                .ok_or_else(|| MgError::Other("remote model source is missing its URL".into()))?;
            http_download_to_staging(url, &staging).await?
        };

        if let Some(expected) = source.checksum() {
            verify_file_checksum(&staging, expected)?;
        }

        mgc_lockfile::atomic::atomic_replace_file(&staging, &dest).map_err(|error| {
            MgError::Other(format!("publish model artifact atomically: {error}"))
        })?;
        #[cfg(unix)]
        std::fs::File::open(dest.parent().unwrap_or(target_dir))
            .and_then(|directory| directory.sync_all())
            .map_err(|error| MgError::Other(format!("sync model artifact directory: {error}")))?;
        Ok((dest.clone(), bytes))
    }
    .await;
    if transfer.is_err() {
        let _ = tokio::fs::remove_file(&staging).await;
    }
    transfer
}

fn validate_remote_integrity(source: &ModelSource) -> MgResult<()> {
    if matches!(source, ModelSource::Local(_)) {
        return Ok(());
    }

    let checksum = source.checksum().ok_or_else(|| {
        MgError::Integrity("remote model sources require a trusted sha256/blake3 checksum".into())
    })?;
    validate_checksum_spec(checksum)?;

    if let ModelSource::HuggingFace { revision, .. } = source {
        let revision = revision.as_deref().ok_or_else(|| {
            MgError::Integrity(
                "Hugging Face model sources require an immutable 40-hex commit revision".into(),
            )
        })?;
        if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(MgError::Integrity(
                "Hugging Face revision must be an immutable 40-hex commit SHA".into(),
            ));
        }
    }
    Ok(())
}

fn validate_checksum_spec(expected: &str) -> MgResult<()> {
    let expected = expected.trim();
    let (algorithm, digest) = match expected.split_once(':') {
        Some(("sha256", digest)) => ("sha256", digest),
        Some(("blake3", digest)) => ("blake3", digest),
        Some((other, _)) => {
            return Err(MgError::Integrity(format!(
                "unsupported checksum algorithm '{other}' (expected 'sha256:' or 'blake3:')"
            )));
        }
        None => ("blake3", expected),
    };
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MgError::Integrity(format!(
            "invalid {algorithm} checksum: expected exactly 64 hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_model_relative_path(value: &str) -> MgResult<PathBuf> {
    use std::path::Component;

    if value.is_empty() || value.contains(['\\', '\0', ':']) {
        return Err(MgError::Other(format!(
            "invalid model artifact path '{value}'"
        )));
    }
    let path = Path::new(value);
    let components = path.components().collect::<Vec<_>>();
    if components.is_empty()
        || components
            .iter()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(MgError::Other(format!(
            "model artifact path must be relative and contain no traversal: '{value}'"
        )));
    }
    Ok(path.to_path_buf())
}

fn ensure_real_directory(path: &Path) -> MgResult<()> {
    std::fs::create_dir_all(path)?;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(MgError::Integrity(format!(
            "model destination root must be a real directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn ensure_safe_model_destination(root: &Path, relative: &Path) -> MgResult<PathBuf> {
    use std::path::Component;

    let canonical_root = root.canonicalize()?;
    let mut parent = canonical_root.clone();
    let components = relative.components().collect::<Vec<_>>();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        let Component::Normal(segment) = component else {
            return Err(MgError::Other(
                "invalid model artifact path component".into(),
            ));
        };
        parent.push(segment);
        match std::fs::symlink_metadata(&parent) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => {
                return Err(MgError::Integrity(format!(
                    "model artifact parent is not a real directory: {}",
                    parent.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match std::fs::create_dir(&parent) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        let metadata = std::fs::symlink_metadata(&parent)?;
                        if !metadata.file_type().is_dir() {
                            return Err(MgError::Integrity(format!(
                                "model artifact parent is not a real directory: {}",
                                parent.display()
                            )));
                        }
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    let Component::Normal(file_name) = components
        .last()
        .copied()
        .ok_or_else(|| MgError::Other("empty model artifact path".into()))?
    else {
        return Err(MgError::Other("invalid model artifact filename".into()));
    };
    let dest = parent.join(file_name);
    match std::fs::symlink_metadata(&dest) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => {
            return Err(MgError::Integrity(format!(
                "model artifact destination is not a regular file: {}",
                dest.display()
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if !dest.starts_with(&canonical_root) {
        return Err(MgError::Integrity(
            "model artifact destination escaped its cache root".into(),
        ));
    }
    Ok(dest)
}

async fn copy_local_to_staging(source: &Path, staging: &Path) -> MgResult<u64> {
    let mut input = tokio::fs::File::open(source)
        .await
        .map_err(|error| MgError::Other(format!("open local model source: {error}")))?;
    let mut output = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staging)
        .await
        .map_err(|error| MgError::Other(format!("create model staging file: {error}")))?;
    let copied = tokio::io::copy(&mut input, &mut output)
        .await
        .map_err(|error| MgError::Other(format!("copy local model to staging: {error}")))?;
    output
        .sync_all()
        .await
        .map_err(|error| MgError::Other(format!("sync local model staging file: {error}")))?;
    Ok(copied)
}

fn unique_staging_path(dest: &Path) -> PathBuf {
    let file_name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("model.bin");
    dest.with_file_name(format!(
        ".{file_name}.mgc-download-{}",
        uuid::Uuid::new_v4()
    ))
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

/// Stream an HTTP artifact to a unique staging path; publish happens only
/// after checksum verification in `download_model`.
/// Stream artifact vào staging riêng; chỉ publish sau khi verify checksum.
async fn http_download_to_staging(url: &str, staging: &Path) -> MgResult<u64> {
    http_download_to_staging_with_limit(url, staging, max_remote_model_bytes()?).await
}

async fn http_download_to_staging_with_limit(
    url: &str,
    staging: &Path,
    max_bytes: u64,
) -> MgResult<u64> {
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(2 * 60 * 60))
        .build()
        .map_err(|error| MgError::Network(format!("create model HTTP client: {error}")))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| MgError::Network(format!("download failed: {e}")))?;

    if !response.status().is_success() {
        return Err(MgError::Network(format!(
            "model download returned HTTP {}",
            response.status()
        )));
    }

    let declared_length = response.content_length();
    if declared_length.is_some_and(|declared| declared > max_bytes) {
        return Err(MgError::Integrity(format!(
            "model download exceeds {MAX_REMOTE_MODEL_BYTES_ENV} limit ({max_bytes} bytes)"
        )));
    }
    let mut response = response;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staging)
        .await
        .map_err(|error| MgError::Other(format!("create model staging file: {error}")))?;
    let mut written = 0_u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| MgError::Network(format!("read model response body: {error}")))?
    {
        written = next_model_download_total(written, chunk.len(), max_bytes)?;
        file.write_all(&chunk)
            .await
            .map_err(|error| MgError::Other(format!("write model staging file: {error}")))?;
    }
    if declared_length.is_some_and(|expected| expected != written) {
        return Err(MgError::Integrity(format!(
            "model download length mismatch: expected {}, received {written}",
            declared_length.unwrap_or_default()
        )));
    }
    file.sync_all()
        .await
        .map_err(|error| MgError::Other(format!("sync model staging file: {error}")))?;
    Ok(written)
}

fn max_remote_model_bytes() -> MgResult<u64> {
    let configured = std::env::var(MAX_REMOTE_MODEL_BYTES_ENV).ok();
    let max_bytes = match configured {
        Some(value) => value.parse::<u64>().map_err(|error| {
            MgError::Integrity(format!(
                "{MAX_REMOTE_MODEL_BYTES_ENV} must be a positive byte count: {error}"
            ))
        })?,
        None => DEFAULT_MAX_REMOTE_MODEL_BYTES,
    };
    if max_bytes == 0 {
        return Err(MgError::Integrity(format!(
            "{MAX_REMOTE_MODEL_BYTES_ENV} must be greater than zero"
        )));
    }
    Ok(max_bytes)
}

fn next_model_download_total(current: u64, chunk_len: usize, max_bytes: u64) -> MgResult<u64> {
    let next = current
        .checked_add(chunk_len as u64)
        .ok_or_else(|| MgError::Integrity("model download size overflow".into()))?;
    if next > max_bytes {
        return Err(MgError::Integrity(format!(
            "model download exceeds configured size limit ({max_bytes} bytes)"
        )));
    }
    Ok(next)
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
            )));
        }
        None => ("blake3", expected),
    };

    let computed = match algo {
        "sha256" => {
            use sha2::Digest;
            use std::io::Read;
            let mut file = std::fs::File::open(path)?;
            let mut hasher = sha2::Sha256::new();
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                hasher.update(&buffer[..count]);
            }
            let digest = hasher.finalize();
            hex::encode(digest)
        }
        _ => mgc_crypto::Blake3Hasher::hash_file(path)
            .map_err(|error| MgError::Other(format!("hash model artifact: {error}")))?
            .to_hex(),
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
