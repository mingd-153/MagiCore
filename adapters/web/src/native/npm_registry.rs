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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub dependencies: Option<std::collections::HashMap<String, String>>,
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

fn global_http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .pool_max_idle_per_host(50)
            .pool_idle_timeout(Duration::from_secs(120))
            .tcp_keepalive(Duration::from_secs(30))
            .timeout(Duration::from_secs(60))
            .user_agent("MegaGate/0.1.0")
            .build()
            .expect("failed to build HTTP client")
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

        self.with_retry(move || {
            let resp_future = client.get(&url);
            let metadata_future = async move {
                let resp = resp_future.send().await?.error_for_status()?;
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

        self.with_retry(move || {
            let mut req = global_http_client().get(&url);
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

    pub async fn download_tarball(&self, url: &str) -> Result<Vec<u8>> {
        self.with_retry(|| async {
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

    pub fn registry_url(&self) -> &str {
        &self.registry_url
    }

    async fn with_retry<F, Fut, T>(&self, mut f: F) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut last_error = None;
        for attempt in 0..4u32 {
            match f().await {
                Ok(value) => return Ok(value),
                Err(err) => {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_fetch_metadata_retries_after_transient_failure() {
        let hits = Arc::new(AtomicUsize::new(0));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
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
}
