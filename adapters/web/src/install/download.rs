//! `install/download.rs` — Tarball downloading and prefetching pipeline.
//! Integrity verification đã tách sang install/integrity.rs.

use std::sync::Arc;

use futures_util::stream::{self, StreamExt};
use mgc_store::{ContentStore, Layout, PackageCache};
use mgc_types::adapter::ResolvedPackage;
use mgc_types::{MgError, MgResult, PackageId};

use crate::audit::{allow_insecure_loopback_url, is_tarball_url_trusted};
use crate::cache::{download_concurrency_limit, SharedWebCache};
use crate::install::extract::{
    ensure_extracted_package_root, ensure_extracted_package_root_from_bytes, tarball_prefetch_lock,
};
use crate::lockfile::compute_tarball_integrity;
use crate::native;
use crate::profile::{PipelineProfile, TarballFetchResult, TarballPayload};

// Re-export integrity helpers để caller/test cũ không đổi import
// Re-export integrity helpers so old callers/tests don't need to change imports
pub use crate::install::integrity::{
    compute_sha256_b64_str, prepare_verified_tarball_for_cache, verify_sri_integrity,
    verify_tarball_integrity,
};

pub fn pipeline_task_concurrency_limit(extract_concurrency: usize) -> usize {
    std::env::var("MAGICORE_WEB_PIPELINE_TASK_CONCURRENCY")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|limit| *limit > 0)
        .unwrap_or_else(|| (download_concurrency_limit() + extract_concurrency).max(1))
}

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
    if !pkg.tarball_url.is_empty() {
        if !pkg.tarball_url.starts_with("https://")
            && !allow_insecure_loopback_url(&pkg.tarball_url)
        {
            eprintln!(
                "WARNING: Tarball URL for '{}' is not HTTPS, using registry fallback",
                pkg.id.name_str()
            );
            return fallback(registry_url, pkg);
        }

        if !is_tarball_url_trusted(&pkg.tarball_url, registry_url) {
            eprintln!(
                "WARNING: Tarball URL for '{}' domain mismatch with registry, using registry fallback",
                pkg.id.name_str()
            );
            return fallback(registry_url, pkg);
        }

        return pkg.tarball_url.clone();
    }

    fallback(registry_url, pkg)
}

pub async fn get_tarball_bytes(
    pkg: &ResolvedPackage,
    cache: &PackageCache,
    shared_package_cache: Option<&PackageCache>,
    registry_url: Option<&str>,
    registry_token: Option<&str>,
    download_sem: &tokio::sync::Semaphore,
) -> MgResult<TarballFetchResult> {
    let prefer_shared_cache = shared_package_cache.is_some();
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
        let _ = std::fs::remove_file(cache.tarball_path(&pkg.id));
    }

    if let Some(pc) = shared_package_cache {
        if let Some(bytes) = pc
            .get_tarball(&pkg.id)
            .map_err(|e| MgError::Store(e.to_string()))?
        {
            if verify_tarball_integrity(pkg, &bytes).is_ok() {
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
            let _ = std::fs::remove_file(pc.tarball_path(&pkg.id));
        }
    }

    let Some(url) = registry_url else {
        return Err(MgError::Other(format!(
            "tarball '{}' not in cache and no registry available",
            pkg.id
        )));
    };

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
        native::npm_registry::DownloadedTarball::Streamed {
            computed_integrity,
            bytes_len,
        } => {
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

pub async fn prefetch_tarballs(
    graph: &mgc_types::adapter::ResolvedGraph,
    skip: &std::collections::HashSet<PackageId>,
    cache: &PackageCache,
    shared_cache: Option<&SharedWebCache>,
    registry: &native::npm_registry::NpmRegistry,
) -> MgResult<u64> {
    use native::npm_registry::LARGE_PKG_THRESHOLD_BYTES;

    enum PrefetchOutcome {
        CacheHit(u64),
        Downloaded(ResolvedPackage, Vec<u8>),
        StreamedToTemp {
            pkg: ResolvedPackage,
            temp_path: std::path::PathBuf,
            computed_integrity: String,
        },
    }

    let mut bytes_from_cache = 0u64;
    let shared_package_cache = shared_cache
        .map(|shared| shared.package_cache())
        .transpose()
        .map_err(|e| MgError::Store(e.to_string()))?;
    let download_semaphore = Arc::new(tokio::sync::Semaphore::new(download_concurrency_limit()));
    let mut downloads = tokio::task::JoinSet::new();

    for pkg in &graph.packages {
        if skip.contains(&pkg.id) {
            continue;
        }
        let pkg_clone = pkg.clone();
        let local_cache = cache.clone();
        let shared_package_cache = shared_package_cache.clone();
        let download_semaphore = Arc::clone(&download_semaphore);
        let registry = native::npm_registry::NpmRegistry::new_with_token(
            registry.registry_url(),
            registry.auth_token().map(str::to_string),
        );
        downloads.spawn(async move {
            let prefetch_lock = tarball_prefetch_lock(&pkg_clone.id);
            let _guard = prefetch_lock.lock().await;

            if let Some(bytes) = local_cache
                .get_tarball(&pkg_clone.id)
                .map_err(|e| MgError::Store(e.to_string()))?
            {
                if verify_tarball_integrity(&pkg_clone, &bytes).is_ok() {
                    return Ok::<_, MgError>(PrefetchOutcome::CacheHit(bytes.len() as u64));
                }
                let _ = std::fs::remove_file(local_cache.tarball_path(&pkg_clone.id));
            }

            if let Some(shared_package_cache) = shared_package_cache.as_ref() {
                if let Some(bytes) = shared_package_cache
                    .get_tarball(&pkg_clone.id)
                    .map_err(|e| MgError::Store(e.to_string()))?
                {
                    if verify_tarball_integrity(&pkg_clone, &bytes).is_ok() {
                        local_cache
                            .cache_tarball_from_path(
                                &pkg_clone.id,
                                &shared_package_cache.tarball_path(&pkg_clone.id),
                            )
                            .map_err(|e| MgError::Store(e.to_string()))?;
                        return Ok::<_, MgError>(PrefetchOutcome::CacheHit(bytes.len() as u64));
                    }
                    let _ = std::fs::remove_file(shared_package_cache.tarball_path(&pkg_clone.id));
                }
            }

            let url = package_tarball_url(registry.registry_url(), &pkg_clone);
            let _permit = download_semaphore
                .acquire_owned()
                .await
                .map_err(|e| MgError::Other(format!("download semaphore closed: {e}")))?;

            let content_length = {
                let client = native::npm_registry::batch_http_client();
                client
                    .head(&url)
                    .send()
                    .await
                    .ok()
                    .and_then(|r| {
                        r.headers()
                            .get(reqwest::header::CONTENT_LENGTH)
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.parse::<u64>().ok())
                    })
                    .unwrap_or(0)
            };

            if content_length > LARGE_PKG_THRESHOLD_BYTES {
                let temp_path = local_cache
                    .tarball_path(&pkg_clone.id)
                    .with_extension("tmp");
                let computed_integrity = registry
                    .download_tarball_to_file(&url, &temp_path)
                    .await
                    .map_err(|e| {
                        MgError::Network(format!(
                            "stream download failed for '{}': {}",
                            pkg_clone.id.name_str(),
                            e
                        ))
                    })?;
                Ok::<_, MgError>(PrefetchOutcome::StreamedToTemp {
                    pkg: pkg_clone,
                    temp_path,
                    computed_integrity,
                })
            } else {
                let bytes = registry.download_tarball(&url).await.map_err(|e| {
                    MgError::Network(format!(
                        "download failed for '{}': {}",
                        pkg_clone.id.name_str(),
                        e
                    ))
                })?;
                Ok::<_, MgError>(PrefetchOutcome::Downloaded(pkg_clone, bytes))
            }
        });
    }

    while let Some(joined) = downloads.join_next().await {
        match joined.map_err(|e| MgError::Other(format!("download task failed: {e}")))?? {
            PrefetchOutcome::CacheHit(bytes) => {
                bytes_from_cache += bytes;
            }
            PrefetchOutcome::Downloaded(mut pkg, bytes) => {
                if pkg.integrity.is_empty() {
                    pkg.integrity = compute_tarball_integrity(&bytes);
                }
                verify_tarball_integrity(&pkg, &bytes)?;
                cache
                    .cache_tarball(&pkg.id, &bytes)
                    .map_err(|e| MgError::Store(e.to_string()))?;
                if let Some(shared_package_cache) = shared_package_cache.as_ref() {
                    let _ = shared_package_cache
                        .cache_tarball_from_path(&pkg.id, &cache.tarball_path(&pkg.id));
                }
            }
            PrefetchOutcome::StreamedToTemp {
                mut pkg,
                temp_path,
                computed_integrity,
            } => {
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
                let final_path = cache.tarball_path(&pkg.id);
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
                if let Some(shared_package_cache) = shared_package_cache.as_ref() {
                    let _ = shared_package_cache.cache_tarball_from_path(&pkg.id, &final_path);
                }
            }
        }
    }

    Ok(bytes_from_cache)
}

pub async fn pipeline_download_and_extract(
    graph: &mgc_types::adapter::ResolvedGraph,
    skip: &std::collections::HashSet<PackageId>,
    cache: &PackageCache,
    shared_cache: Option<&SharedWebCache>,
    registry: Option<&native::npm_registry::NpmRegistry>,
    layout: &Layout,
    store: &ContentStore,
) -> MgResult<(
    u64,
    std::collections::HashMap<PackageId, std::path::PathBuf>,
    Vec<tokio::task::JoinHandle<()>>,
)> {
    let download_sem = Arc::new(tokio::sync::Semaphore::new(download_concurrency_limit()));
    let pipeline_profile = Arc::new(PipelineProfile::from_env());
    let shared_package_cache = shared_cache
        .map(|shared| shared.package_cache())
        .transpose()
        .map_err(|e| MgError::Store(e.to_string()))?;
    let extract_concurrency = std::env::var("MAGICORE_WEB_EXTRACT_CONCURRENCY")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(32)
        });
    let extract_sem = Arc::new(tokio::sync::Semaphore::new(extract_concurrency));
    let task_concurrency = pipeline_task_concurrency_limit(extract_concurrency);
    let scheduled_packages: Vec<ResolvedPackage> = graph
        .packages
        .iter()
        .filter(|pkg| !skip.contains(&pkg.id))
        .cloned()
        .collect();
    let tasks = scheduled_packages.into_iter().map(|pkg| {
        let cache = cache.clone();
        let shared_cache = shared_cache.cloned();
        let shared_package_cache = shared_package_cache.clone();
        let registry_url = registry.map(|r| r.registry_url().to_string());
        let registry_token = registry.and_then(|r| r.auth_token().map(str::to_string));
        let layout = layout.clone();
        let store = store.clone();
        let download_sem = Arc::clone(&download_sem);
        let extract_sem = Arc::clone(&extract_sem);
        let pipeline_profile = Arc::clone(&pipeline_profile);

        async move {
            let lock = tarball_prefetch_lock(&pkg.id);
            let _guard = lock.lock().await;

            let download_started_at = std::time::Instant::now();
            let fetch = get_tarball_bytes(
                &pkg,
                &cache,
                shared_package_cache.as_ref(),
                registry_url.as_deref(),
                registry_token.as_deref(),
                &download_sem,
            )
            .await?;
            pipeline_profile.record_download(
                &pkg.id,
                fetch.payload.len(),
                download_started_at.elapsed().as_millis() as u64,
                fetch.queue_wait_ms,
                fetch.io_ms,
            );

            let shared_cache_persist = if fetch.persist_to_shared_cache {
                shared_package_cache.clone().map(|pc| {
                    let pkg_id = pkg.id.clone();
                    match &fetch.payload {
                        TarballPayload::Bytes(bytes) => {
                            let bytes = Arc::clone(bytes);
                            tokio::task::spawn_blocking(move || {
                                let _ = pc.cache_tarball(&pkg_id, bytes.as_ref());
                            })
                        }
                        TarballPayload::CachedPath(path, _) => {
                            let path = path.clone();
                            tokio::task::spawn_blocking(move || {
                                let _ = pc.cache_tarball_from_path(&pkg_id, &path);
                            })
                        }
                    }
                })
            } else {
                None
            };

            let _permit = extract_sem
                .acquire_owned()
                .await
                .map_err(|e| MgError::Other(format!("extract semaphore closed: {e}")))?;
            let id = pkg.id.clone();
            let tarball_len = fetch.payload.len();
            let extract_started_at = std::time::Instant::now();
            let root = tokio::task::spawn_blocking(move || match fetch.payload {
                TarballPayload::Bytes(bytes) => ensure_extracted_package_root_from_bytes(
                    &layout,
                    &store,
                    shared_cache.as_ref(),
                    &pkg,
                    bytes.as_ref(),
                ),
                TarballPayload::CachedPath(path, _) => ensure_extracted_package_root(
                    &layout,
                    &store,
                    shared_cache.as_ref(),
                    &pkg,
                    &path,
                ),
            })
            .await
            .map_err(|e| MgError::Other(format!("extract task panicked: {e}")))??;
            pipeline_profile.record_extract(&id, extract_started_at.elapsed().as_millis() as u64);

            Ok::<_, MgError>((tarball_len, id, root, shared_cache_persist))
        }
    });

    let finished = stream::iter(tasks)
        .buffer_unordered(task_concurrency)
        .collect::<Vec<_>>()
        .await;
    let mut total_bytes = 0u64;
    let mut results = std::collections::HashMap::new();
    let mut persist_handles = Vec::new();
    let mut pipeline_errors = Vec::new();
    for joined in finished {
        match joined {
            Ok((bytes, id, root, persist)) => {
                total_bytes += bytes;
                results.insert(id, root);
                if let Some(persist) = persist {
                    persist_handles.push(persist);
                }
            }
            Err(e) => pipeline_errors.push(e),
        }
    }
    if let Some(e) = pipeline_errors.into_iter().next() {
        return Err(e);
    }

    pipeline_profile.flush();

    Ok((total_bytes, results, persist_handles))
}
