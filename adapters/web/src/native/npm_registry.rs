use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub name: String,
    pub description: Option<String>,
    pub versions: std::collections::HashMap<String, VersionInfo>,
    #[serde(rename = "dist-tags")]
    pub dist_tags: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub time: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(rename = "devDependencies")]
    pub dev_dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(rename = "peerDependencies")]
    pub peer_dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(rename = "optionalDependencies")]
    pub optional_dependencies: Option<std::collections::HashMap<String, String>>,
    pub os: Option<Vec<String>>,
    pub cpu: Option<Vec<String>>,
    pub dist: Option<DistInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistInfo {
    pub tarball: String,
    #[serde(rename = "integrity")]
    pub integrity: Option<String>,
}

pub struct NpmRegistry {
    registry_url: String,
}

pub enum DownloadedTarball {
    Bytes(Vec<u8>),
    Streamed {
        computed_integrity: String,
        bytes_len: u64,
    },
}

fn network_profile_enabled() -> bool {
    std::env::var("MEGAGATE_WEB_PROFILE_NETWORK")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn network_profile_log(kind: &str, target: &str, message: &str) {
    if network_profile_enabled() {
        eprintln!(
            "[megagate:web:network-profile] kind={} target={} {}",
            kind, target, message
        );
    }
}

/// Primary HTTP client: used for metadata fetches. Optimized for many concurrent
/// short-lived requests against the same host (registry.npmjs.org).
fn global_http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            // Increased pool: 256 concurrent connections per registry host.
            .pool_max_idle_per_host(256)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(30))
            // Metadata responses are small; 30s is plenty.
            .timeout(Duration::from_secs(30))
            .user_agent(format!("MegaGate/{}", env!("CARGO_PKG_VERSION")))
            // H2 stream window: 4 MiB — allows multiple concurrent streams without
            // stalling when one response is slow.
            .http2_initial_stream_window_size(4 * 1024 * 1024)
            // H2 connection window: 32 MiB
            .http2_initial_connection_window_size(32 * 1024 * 1024)
            .build()
            .expect("failed to build HTTP client")
    })
}

/// Batch HTTP client: used for tarball streaming. Optimized for large bodies.
pub fn batch_http_client() -> &'static reqwest::Client {
    static BATCH_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    BATCH_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .pool_max_idle_per_host(256)
            .pool_idle_timeout(Duration::from_secs(300))
            .tcp_keepalive(Duration::from_secs(60))
            // Long timeout for massive tarballs like @next/swc (>200 MB).
            .timeout(Duration::from_secs(300))
            .user_agent(format!("MegaGate/{}/batch", env!("CARGO_PKG_VERSION")))
            .http2_initial_stream_window_size(16 * 1024 * 1024)
            .http2_initial_connection_window_size(64 * 1024 * 1024)
            .build()
            .expect("failed to build batch HTTP client")
    })
}

fn jitter_ms(attempt: u32) -> u64 {
    let state = (attempt as u64)
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (state >> 33) % 100
}

impl NpmRegistry {
    pub fn new(registry_url: &str) -> Self {
        Self {
            registry_url: registry_url.to_string(),
        }
    }

    pub async fn fetch_metadata(&self, package: &str) -> Result<PackageMetadata> {
        let (metadata, _) = self.fetch_metadata_with_etag(package).await?;
        Ok(metadata)
    }

    pub async fn fetch_metadata_with_etag(
        &self,
        package: &str,
    ) -> Result<(PackageMetadata, Option<String>)> {
        let url = format!("{}/{}", self.registry_url, package);
        let client = global_http_client();

        with_retry("metadata", package, move || {
            let resp_future = client.get(&url);
            let metadata_future = async move {
                let resp = resp_future
                    .header("Accept", "application/vnd.npm.install-v1+json")
                    .send()
                    .await?
                    .error_for_status()?;
                let etag = resp
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                let metadata: PackageMetadata = resp.json().await?;
                Ok((metadata, etag))
            };
            metadata_future
        })
        .await
    }

    pub async fn fetch_metadata_conditional(
        &self,
        package: &str,
        etag: Option<&str>,
    ) -> Result<Option<(PackageMetadata, String)>> {
        let url = format!("{}/{}", self.registry_url, package);
        let etag_owned = etag.map(|s| s.to_string());

        with_retry("metadata-conditional", package, move || {
            let mut req = global_http_client()
                .get(&url)
                .header("Accept", "application/vnd.npm.install-v1+json");
            if let Some(ref etag_val) = etag_owned {
                req = req.header("If-None-Match", etag_val);
            }
            async move {
                let resp = req.send().await?;

                if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
                    return Ok(None);
                }

                let resp = resp.error_for_status()?;
                let new_etag = resp
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();

                let metadata: PackageMetadata = resp.json().await?;
                Ok(Some((metadata, new_etag)))
            }
        })
        .await
    }

    /// Classic buffered download — kept for compatibility with callers that need
    /// the raw bytes (e.g., integrity verification before store import).
    pub async fn download_tarball(&self, url: &str) -> Result<Vec<u8>> {
        with_retry("tarball", url, || async {
            let resp = global_http_client()
                .get(url)
                .send()
                .await?
                .error_for_status()?;
            let bytes = resp.bytes().await?;
            Ok(bytes.to_vec())
        })
        .await
    }

    /// **Fast-lane streaming download** — writes directly to `dest` without
    /// holding the full tarball in RAM. Returns the SHA-512 integrity hash
    /// (`sha512-<base64>`) computed on-the-fly as the stream is written.
    ///
    /// This is the primary path for large packages (> LARGE_PKG_THRESHOLD_BYTES).
    pub async fn download_tarball_to_file(
        &self,
        url: &str,
        dest: &std::path::Path,
    ) -> Result<String> {
        use futures_util::StreamExt;
        use sha2::Digest;
        use tokio::io::AsyncWriteExt;

        with_retry("tarball-stream", url, || async {
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let resp = batch_http_client()
                .get(url)
                .send()
                .await?
                .error_for_status()?;

            let mut file = tokio::fs::File::create(dest).await?;
            let mut stream = resp.bytes_stream();
            let mut hasher = sha2::Sha512::new();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                hasher.update(&chunk);
                file.write_all(&chunk).await?;
            }
            file.flush().await?;

            let digest = hasher.finalize();
            let b64 = base64_encode(&digest);
            Ok(format!("sha512-{b64}"))
        })
        .await
    }

    pub async fn download_tarball_auto(
        &self,
        url: &str,
        dest: &std::path::Path,
    ) -> Result<DownloadedTarball> {
        use futures_util::StreamExt;
        use sha2::Digest;
        use tokio::io::AsyncWriteExt;

        with_retry("tarball-auto", url, || async {
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            let resp = batch_http_client()
                .get(url)
                .send()
                .await?
                .error_for_status()?;

            let content_length = resp.content_length().unwrap_or(0);
            if content_length <= LARGE_PKG_THRESHOLD_BYTES {
                let bytes = resp.bytes().await?;
                return Ok(DownloadedTarball::Bytes(bytes.to_vec()));
            }

            let mut file = tokio::fs::File::create(dest).await?;
            let mut stream = resp.bytes_stream();
            let mut hasher = sha2::Sha512::new();
            let mut bytes_len = 0u64;

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                bytes_len += chunk.len() as u64;
                hasher.update(&chunk);
                file.write_all(&chunk).await?;
            }
            file.flush().await?;

            let digest = hasher.finalize();
            let b64 = base64_encode(&digest);
            Ok(DownloadedTarball::Streamed {
                computed_integrity: format!("sha512-{b64}"),
                bytes_len,
            })
        })
        .await
    }

    pub fn registry_url(&self) -> &str {
        &self.registry_url
    }
}

/// Parallel metadata fetcher — fires all requests concurrently and collects results.
/// Returns errors individually so a single bad package does not abort the batch.
pub async fn batch_fetch_metadata(
    registry_url: &str,
    packages: &[&str],
) -> Vec<(String, Result<PackageMetadata>)> {
    use futures_util::future::join_all;

    let futures: Vec<_> = packages
        .iter()
        .map(|name| {
            let reg = NpmRegistry::new(registry_url);
            let name = name.to_string();
            async move {
                let result = reg.fetch_metadata(&name).await;
                (name, result)
            }
        })
        .collect();

    join_all(futures).await
}

pub async fn batch_download_tarball(url: &str) -> Result<Vec<u8>> {
    let client = batch_http_client();
    with_retry("batch-tarball", url, || async {
        let resp = client.get(url).send().await?.error_for_status()?;
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    })
    .await
}

/// Security guard: returns `Err` if the given package version was published within
/// the last 24 hours (Supply-chain quarantine period). Returns `Ok(true)` if the
/// package passes the check, `Ok(false)` if publish time is unknown/unparseable
/// (treated as safe to avoid blocking packages without a timestamp).
pub fn check_publish_age(
    metadata: &PackageMetadata,
    version: &str,
) -> Result<bool, String> {
    let Some(ts_str) = metadata.time.get(version) else {
        return Ok(false); // no timestamp — not quarantined
    };

    let published = chrono::DateTime::parse_from_rfc3339(ts_str)
        .map_err(|e| format!("failed to parse publish time for {}: {}", version, e))?;
    let now = chrono::Utc::now();
    let age = now.signed_duration_since(published.with_timezone(&chrono::Utc));

    if age < chrono::Duration::hours(24) {
        let hours = age.num_minutes() as f64 / 60.0;
        Err(format!(
            "🚨 SECURITY: Package '{}@{}' was published only {:.1}h ago (< 24h quarantine).\n   \
             This may be a supply-chain attack. Use --allow-untrusted to override.",
            metadata.name, version, hours
        ))
    } else {
        Ok(true)
    }
}

/// Threshold in bytes above which `download_tarball_to_file` is used instead of
/// buffered `download_tarball`. Set to 2 MiB.
pub const LARGE_PKG_THRESHOLD_BYTES: u64 = 2 * 1024 * 1024;

fn base64_encode(bytes: &[u8]) -> String {
    // Manual base64 to avoid new dep — use alphabet from RFC 4648.
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 { chunk[1] as usize } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as usize } else { 0 };
        out.push(ALPHABET[b0 >> 2] as char);
        out.push(ALPHABET[((b0 & 0x3) << 4) | (b1 >> 4)] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((b1 & 0xF) << 2) | (b2 >> 6)] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[b2 & 0x3F] as char);
        } else {
            out.push('=');
        }
    }
    out
}

async fn with_retry<F, Fut, T>(kind: &str, target: &str, mut f: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut last_error = None;
    for attempt in 0..4u32 {
        match f().await {
            Ok(value) => {
                if attempt > 0 {
                    network_profile_log(
                        kind,
                        target,
                        &format!("success_after_retry attempt={}", attempt + 1),
                    );
                }
                return Ok(value);
            }
            Err(err) => {
                network_profile_log(
                    kind,
                    target,
                    &format!("retry attempt={} error={}", attempt + 1, err),
                );
                last_error = Some(err);
                if attempt < 3 {
                    let base_ms = 50u64 * (2u64.pow(attempt));
                    let jitter = jitter_ms(attempt);
                    tokio::time::sleep(Duration::from_millis(base_ms + jitter)).await;
                }
            }
        }
    }
    Err(last_error.expect("retry loop should capture an error"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn bind_test_listener() -> Option<TcpListener> {
        match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => Some(listener),
            Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                eprintln!("skipping socket-backed test in sandbox: {err}");
                None
            }
            Err(err) => panic!("failed to bind socket-backed test listener: {err}"),
        }
    }

    #[tokio::test]
    async fn test_fetch_metadata_retries_after_transient_failure() {
        let hits = Arc::new(AtomicUsize::new(0));
        let Some(listener) = bind_test_listener().await else {
            return;
        };
        let addr = listener.local_addr().unwrap();
        let hits_for_server = hits.clone();

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let hit = hits_for_server.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                if hit == 0 || hit == 1 {
                    let _ = stream
                        .write_all(
                            b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 3\r\n\r\nbad",
                        )
                        .await;
                } else {
                    let body = r#"{"name":"react","description":null,"versions":{"18.2.0":{"version":"18.2.0","dependencies":null,"optionalDependencies":null,"os":null,"cpu":null,"dist":{"tarball":"http://example.test/react.tgz","integrity":null}}},"dist-tags":{"latest":"18.2.0"}}"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            }
        });

        let registry = NpmRegistry::new(&format!("http://{}", addr));
        let metadata = registry.fetch_metadata("react").await.unwrap();
        assert_eq!(metadata.name, "react");
        assert!(hits.load(Ordering::SeqCst) >= 3);
    }

    #[test]
    fn test_check_publish_age_blocks_new_package() {
        let mut meta = PackageMetadata {
            name: "evil-pkg".to_string(),
            description: None,
            versions: Default::default(),
            dist_tags: Default::default(),
            time: Default::default(),
        };
        // 1 hour ago
        let published_at = (chrono::Utc::now() - chrono::Duration::hours(1))
            .to_rfc3339();
        meta.time.insert("1.0.0".to_string(), published_at);

        let result = check_publish_age(&meta, "1.0.0");
        assert!(result.is_err(), "should block packages published < 24h ago");
        assert!(result.unwrap_err().contains("quarantine"));
    }

    #[test]
    fn test_check_publish_age_allows_old_package() {
        let mut meta = PackageMetadata {
            name: "safe-pkg".to_string(),
            description: None,
            versions: Default::default(),
            dist_tags: Default::default(),
            time: Default::default(),
        };
        // 48 hours ago
        let published_at = (chrono::Utc::now() - chrono::Duration::hours(48))
            .to_rfc3339();
        meta.time.insert("2.0.0".to_string(), published_at);

        let result = check_publish_age(&meta, "2.0.0");
        assert!(result.is_ok());
    }

    #[test]
    fn test_base64_encode_hello() {
        // "Hello" in base64 is "SGVsbG8="
        assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
    }
}
