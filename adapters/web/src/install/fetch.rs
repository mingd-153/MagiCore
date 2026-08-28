//! `install/fetch.rs` — Tarball URL construction and HTTP fetch helpers.
//! Tách từ download.rs để tách concern fetch khỏi pipeline orchestration.

use std::sync::Arc;

use mgc_store::PackageCache;
use mgc_types::adapter::ResolvedPackage;
use mgc_types::{MgError, MgResult};

use crate::audit::{allow_insecure_loopback_url, is_tarball_url_trusted};
use crate::install::integrity::{prepare_verified_tarball_for_cache, verify_tarball_integrity};
use crate::native;
use crate::profile::{TarballFetchResult, TarballPayload};

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
    if let Some(pc) = shared_package_cache {
        if let Some(bytes) = pc
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
    let temp_path = final_path.with_extension("tmp");

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
                pkg.integrity = computed_integrity;
            }

            // Promote temp file to final cache location
            // Promote file temp vào cache location cuối cùng
            if let Some(parent) = final_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| MgError::Store(e.to_string()))?;
            }
            std::fs::rename(&temp_path, &final_path).map_err(|e| {
                MgError::Store(format!(
                    "failed to promote streamed tarball for '{}': {}",
                    pkg.id.name_str(),
                    e
                ))
            })?;

            Ok(TarballFetchResult {
                payload: TarballPayload::CachedPath(final_path, bytes_len),
                queue_wait_ms,
                io_ms,
                persist_to_shared_cache: false,
            })
        }
    }
}
