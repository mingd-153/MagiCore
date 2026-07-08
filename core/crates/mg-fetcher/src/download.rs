/// Download utilities with progress tracking
use anyhow::Result;
use std::path::Path;

/// Download progress callback
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Download file with progress tracking
pub async fn download_with_progress(
    url: &str,
    dest: &Path,
    _progress: Option<ProgressCallback>,
) -> Result<()> {
    // Simple implementation - full progress tracking would use reqwest streaming
    let client = reqwest::Client::new();
    let response = client.get(url).send().await?;
    let bytes = response.bytes().await?;

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::write(dest, bytes).await?;
    Ok(())
}
