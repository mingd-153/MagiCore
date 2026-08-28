/// Download utilities with streaming support and progress tracking.
use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::path::Path;
use tokio::io::AsyncWriteExt;

/// Download progress callback
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Download file with streaming and progress tracking without loading entire payload into RAM
pub async fn download_with_progress(
    url: &str,
    dest: &Path,
    progress: Option<ProgressCallback>,
) -> Result<()> {
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to send GET request to {url}"))?;

    let total_size = response.content_length().unwrap_or(0);

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut file = tokio::fs::File::create(dest)
        .await
        .with_context(|| format!("failed to create output file '{}'", dest.display()))?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.with_context(|| "error reading chunk from stream")?;
        file.write_all(&chunk)
            .await
            .with_context(|| "failed to write chunk to disk")?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if let Some(ref cb) = progress {
            cb(downloaded, total_size);
        }
    }

    file.flush().await?;
    Ok(())
}
