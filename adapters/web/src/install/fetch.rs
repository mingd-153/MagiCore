//! `install/fetch.rs` — Tarball URL construction and HTTP fetch helpers.
//! Tách từ download.rs để tách concern fetch khỏi pipeline orchestration.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mgc_store::PackageCache;
use mgc_types::adapter::ResolvedPackage;
use mgc_types::{MgError, MgResult};

use crate::audit::{allow_insecure_loopback_url, is_tarball_url_trusted};
use crate::install::integrity::{prepare_verified_tarball_for_cache, verify_tarball_integrity};
use crate::native;
use crate::profile::{TarballFetchResult, TarballPayload};

/// Give each concurrent install its own same-filesystem download staging path.
/// A fixed `<tarball>.tmp` lets one process rename another process's in-flight
/// download, producing missing or truncated package cache entries.
/// Mỗi install đồng thời có staging riêng cùng filesystem; tên `.tmp` cố định
/// khiến process khác có thể rename nhầm download đang chạy.
pub(crate) fn unique_tarball_staging_path(final_path: &Path) -> PathBuf {
    let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
    let name = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("package.tgz");
    parent.join(format!(
        ".{name}.download-{}-{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4()
    ))
}

pub(crate) struct DownloadStagingGuard(PathBuf);

impl DownloadStagingGuard {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for DownloadStagingGuard {
    fn drop(&mut self) {
        // The file can be absent after successful publication; cleanup is
        // best-effort so cancellation/failure never leaves owned staging.
        // File đã được chuyển khi thành công; drop chỉ dọn staging còn sót.
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Publish a fully downloaded tarball without replacing another writer's
/// cache entry. A losing writer accepts only a winner with the same SHA-512.
/// Publish file đã tải xong theo kiểu first-writer-wins; writer thua chỉ nhận
/// cache thắng nếu SHA-512 khớp.
pub(crate) fn publish_downloaded_tarball(
    staging_path: &Path,
    final_path: &Path,
    expected_integrity: &str,
) -> MgResult<()> {
    if let Some(parent) = final_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| MgError::Store(err.to_string()))?;
    }

    let publish_result = match std::fs::hard_link(staging_path, final_path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_cached_tarball_integrity(final_path, expected_integrity)
        }
        Err(err) => Err(MgError::Store(format!(
            "cannot atomically publish tarball cache entry '{}': {err}",
            final_path.display()
        ))),
    };

    if publish_result.is_err() {
        let _ = std::fs::remove_file(staging_path);
        return publish_result;
    }
    std::fs::remove_file(staging_path).map_err(|err| {
        MgError::Store(format!(
            "published tarball cache entry but failed to remove staging file '{}': {err}",
            staging_path.display()
        ))
    })
}

fn verify_cached_tarball_integrity(path: &Path, expected_integrity: &str) -> MgResult<()> {
    use base64::Engine;
    use sha2::{Digest, Sha512};
    use std::io::Read;

    let metadata = std::fs::symlink_metadata(path).map_err(|err| {
        MgError::Store(format!(
            "cannot inspect concurrent tarball cache winner: {err}"
        ))
    })?;
    if !metadata.file_type().is_file() {
        return Err(MgError::Store(format!(
            "concurrent tarball cache winner is not a regular file: '{}'",
            path.display()
        )));
    }

    let mut file = std::fs::File::open(path)
        .map_err(|err| MgError::Store(format!("cannot verify concurrent cache winner: {err}")))?;
    let mut hasher = Sha512::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| MgError::Store(format!("cannot hash concurrent cache winner: {err}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = format!(
        "sha512-{}",
        base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
    );
    if actual != expected_integrity {
        return Err(MgError::Store(format!(
            "concurrent tarball cache conflict for '{}': winner integrity does not match downloaded artifact",
            path.display()
        )));
    }
    Ok(())
}

/// Construct tarball download URL for a package.
/// Xây dựng URL tải tarball cho package — ưu tiên pkg.tarball_url, fallback registry chuẩn.
///
/// Validates HTTPS (except loopback) and domain trust before using pkg.tarball_url.
/// Kiểm tra HTTPS (trừ loopback) và trust domain trước khi dùng pkg.tarball_url.
pub fn package_tarball_url(registry_url: &str, pkg: &ResolvedPackage) -> String {
    let fallback = |_registry: &str, _pkg: &ResolvedPackage| {
        format!(
            "{}/{}/-/{}-{}.tgz",
            _registry.trim_end_matches('/'),
            _pkg.id.name_str(),
            _pkg.id.name().unscoped(),
            _pkg.id.version()
        )
    };

    // If package has explicit tarball URL, validate before using
    // Nếu package có tarball URL rõ ràng, validate trước khi dùng
    if !pkg.tarball_url.is_empty() {
        // Reject non-HTTPS (except localhost/loopback for testing)
        // Reject không-HTTPS (trừ localhost/loopback cho testing)
        if !pkg.tarball_url.starts_with("https://")
            && !allow_insecure_loopback_url(&pkg.tarball_url)
        {
            eprintln!(
                "WARNING: Tarball URL for '{}' is not HTTPS, using registry fallback",
                pkg.id.name_str()
            );
            return fallback(registry_url, pkg);
        }

        // Check domain matches registry (prevent tarball hijack)
        // Kiểm tra domain khớp registry (chống hijack tarball)
        if !is_tarball_url_trusted(&pkg.tarball_url, registry_url) {
            eprintln!(
                "WARNING: Tarball URL for '{}' domain mismatch with registry, using registry fallback",
                pkg.id.name_str()
            );
            return fallback(registry_url, pkg);
        }

        return pkg.tarball_url.clone();
    }

    // No explicit URL: use registry default pattern
    // Không có URL rõ: dùng pattern registry mặc định
    fallback(registry_url, pkg)
}

/// Fetch tarball bytes for a package (cache → shared cache → network).
/// Tải tarball bytes cho package (cache → shared cache → network).
///
/// Tries local cache first, then shared cache, then downloads from registry.
/// Thử cache local trước, rồi shared cache, rồi download từ registry.
pub async fn get_tarball_bytes(
    pkg: &ResolvedPackage,
    cache: &PackageCache,
    shared_package_cache: Option<&PackageCache>,
    registry_url: Option<&str>,
    registry_token: Option<&str>,
    download_sem: &tokio::sync::Semaphore,
) -> MgResult<TarballFetchResult> {
    let prefer_shared_cache = shared_package_cache.is_some();

    // Try local cache first
    // Thử cache local trước
    if let Some(bytes) = cache
        .get_tarball(&pkg.id)
        .map_err(|e| MgError::Store(e.to_string()))?
    {
        if verify_tarball_integrity(pkg, &bytes).is_ok() {
            return Ok(TarballFetchResult {
                payload: TarballPayload::Bytes(Arc::<[u8]>::from(bytes)),
                queue_wait_ms: 0,
                io_ms: 0,
                persist_to_shared_cache: false,
            });
        }
        // Corrupted local cache: remove
        // Cache local hỏng: xóa
        let _ = std::fs::remove_file(cache.tarball_path(&pkg.id));
    }

    // Try shared cache
    // Thử shared cache
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Some(pc) = shared_package_cache
        && let Some(bytes) = pc
            .get_tarball(&pkg.id)
            .map_err(|e| MgError::Store(e.to_string()))?
    {
        if verify_tarball_integrity(pkg, &bytes).is_ok() {
            // Copy to local cache if needed
            // Copy vào local cache nếu cần
            if !prefer_shared_cache {
                let _ = cache.cache_tarball_from_path(&pkg.id, &pc.tarball_path(&pkg.id));
            }
            return Ok(TarballFetchResult {
                payload: TarballPayload::Bytes(Arc::<[u8]>::from(bytes)),
                queue_wait_ms: 0,
                io_ms: 0,
                persist_to_shared_cache: false,
            });
        }
        // Corrupted shared cache: remove
        // Shared cache hỏng: xóa
        let _ = std::fs::remove_file(pc.tarball_path(&pkg.id));
    }

    // Cache miss: download from registry
    // Cache miss: download từ registry
    let url = registry_url.ok_or_else(|| {
        MgError::Network(format!(
            "no registry URL provided for '{}' and no cache hit",
            pkg.id.name_str()
        ))
    })?;

    let queue_started_at = std::time::Instant::now();
    let _permit = download_sem
        .acquire()
        .await
        .map_err(|e| MgError::Other(format!("download semaphore closed: {e}")))?;
    let queue_wait_ms = queue_started_at.elapsed().as_millis() as u64;

    let tarball_url = package_tarball_url(url, pkg);
    let io_started_at = std::time::Instant::now();
    let mut pkg = pkg.clone();

    let final_path = shared_package_cache
        .map(|pc| pc.tarball_path(&pkg.id))
        .unwrap_or_else(|| cache.tarball_path(&pkg.id));
    let temp_path = unique_tarball_staging_path(&final_path);
    let staging = DownloadStagingGuard::new(temp_path.clone());

    // Download via NpmRegistry (supports zero-buffer streaming)
    // Download qua NpmRegistry (hỗ trợ zero-buffer streaming)
    let downloaded =
        native::npm_registry::NpmRegistry::new_with_token(url, registry_token.map(str::to_string))
            .download_tarball_auto(&tarball_url, &temp_path)
            .await
            .map_err(|e| {
                MgError::Network(format!(
                    "download failed for '{}': {}",
                    pkg.id.name_str(),
                    e
                ))
            })?;

    let io_ms = io_started_at.elapsed().as_millis() as u64;

    match downloaded {
        // Small tarball: downloaded to memory
        // Tarball nhỏ: download vào memory
        native::npm_registry::DownloadedTarball::Bytes(bytes) => {
            prepare_verified_tarball_for_cache(&mut pkg, &bytes)?;

            let persist_to_shared_cache = shared_package_cache.is_some();
            if !persist_to_shared_cache {
                cache
                    .cache_tarball(&pkg.id, &bytes)
                    .map_err(|e| MgError::Store(e.to_string()))?;
            }

            Ok(TarballFetchResult {
                payload: TarballPayload::Bytes(Arc::<[u8]>::from(bytes)),
                queue_wait_ms,
                io_ms,
                persist_to_shared_cache,
            })
        }
        // Large tarball: streamed to disk
        // Tarball lớn: stream vào disk
        native::npm_registry::DownloadedTarball::Streamed {
            computed_integrity,
            bytes_len,
        } => {
            // Verify streamed tarball integrity
            // Verify integrity tarball đã stream
            if !pkg.integrity.is_empty() && pkg.integrity != computed_integrity {
                let _ = std::fs::remove_file(&temp_path);
                return Err(MgError::Other(format!(
                    "integrity mismatch for '{}': expected '{}', got '{}'",
                    pkg.id.name_str(),
                    pkg.integrity,
                    computed_integrity
                )));
            }

            if pkg.integrity.is_empty() {
                pkg.integrity = computed_integrity.clone();
            }

            publish_downloaded_tarball(staging.path(), &final_path, &computed_integrity)?;

            Ok(TarballFetchResult {
                payload: TarballPayload::CachedPath(final_path, bytes_len),
                queue_wait_ms,
                io_ms,
                persist_to_shared_cache: false,
            })
        }
    }
}
