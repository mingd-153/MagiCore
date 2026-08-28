//! Model installation for AI adapter.
//! Download models từ HuggingFace Hub, local paths, hoặc URLs.

use mgc_types::MgResult;
use std::path::{Path, PathBuf};

pub mod download;
pub mod verify;

use download::ModelSource;
use verify::verify_model_checksum;

/// Model install summary
#[derive(Debug, Clone)]
pub struct InstallSummary {
    pub model_id: String,
    pub source: ModelSource,
    pub local_path: PathBuf,
    pub verified: bool,
    pub bytes_downloaded: u64,
    pub duration_ms: u64,
}

/// Install model from source (HuggingFace, local, URL)
pub async fn install_model(
    model_id: &str,
    source: ModelSource,
    target_dir: &Path,
) -> MgResult<InstallSummary> {
    let start = std::time::Instant::now();

    // Download model
    let (local_path, bytes_downloaded) =
        download::download_model(model_id, &source, target_dir).await?;

    // Verify checksum if provided
    let verified = if let Some(checksum) = source.checksum() {
        verify_model_checksum(&local_path, checksum)?
    } else {
        false
    };

    let duration_ms = start.elapsed().as_millis() as u64;

    Ok(InstallSummary {
        model_id: model_id.to_string(),
        source,
        local_path,
        verified,
        bytes_downloaded,
        duration_ms,
    })
}

#[cfg(test)]
#[path = "test/install_tests.rs"]
mod tests;
