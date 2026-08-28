// Tarball prefetch orchestration for core-web — warms shared cache after resolve.
// Điều phối prefetch tarball cho core-web — tách khỏi adapter root để dễ maintain.
use std::sync::Arc;

use mgc_types::adapter::ResolvedPackage;
use mgc_types::MgResult;

use crate::cache::{download_concurrency_limit, SharedWebCache};
use crate::install::download::package_tarball_url;
use crate::install::extract::tarball_prefetch_lock;
use crate::native;

pub fn spawn_tarball_download(
    shared_cache: SharedWebCache,
    packages: Vec<ResolvedPackage>,
    registry_url: String,
) -> tokio::task::JoinHandle<MgResult<u64>> {
    tokio::spawn(async move {
        let cache = shared_cache
            .package_cache()
            .map_err(|e| mgc_types::MgError::Store(e.to_string()))?;
        let download_sem = Arc::new(tokio::sync::Semaphore::new(download_concurrency_limit()));
        let mut set: tokio::task::JoinSet<MgResult<u64>> = tokio::task::JoinSet::new();
        for pkg in packages {
            let cache = cache.clone();
            let reg = native::npm_registry::NpmRegistry::new(registry_url.as_str());
            let download_sem = Arc::clone(&download_sem);
            set.spawn(async move {
                let id = pkg.id.clone();
                let lock = tarball_prefetch_lock(&id);
                let _guard = lock.lock().await;
                if let Some(bytes) = cache
                    .get_tarball(&id)
                    .map_err(|e| mgc_types::MgError::Store(e.to_string()))?
                {
                    return Ok(bytes.len() as u64);
                }
                let _permit = download_sem.acquire_owned().await.map_err(|e| {
                    mgc_types::MgError::Other(format!("download semaphore closed: {e}"))
                })?;
                let url = package_tarball_url(reg.registry_url(), &pkg);
                let bytes =
                    native::npm_registry::batch_download_tarball_with_auth(&url, reg.auth_token())
                        .await
                        .map_err(|e| {
                            mgc_types::MgError::Network(format!("prefetch dl failed: {e}"))
                        })?;
                let id = pkg.id.clone();
                let len = bytes.len() as u64;
                let cache2 = cache.clone();
                match tokio::task::spawn_blocking(move || {
                    let mut pkg = pkg;
                    if let Err(e) = crate::install::download::prepare_verified_tarball_for_cache(
                        &mut pkg, &bytes,
                    ) {
                        eprintln!("[magicore] prefetch integrity failed for {id}: {e}");
                    } else if let Err(e) = cache2.cache_tarball(&pkg.id, &bytes) {
                        eprintln!("[magicore] prefetch cache write failed for {id}: {e}");
                    }
                })
                .await
                {
                    Ok(()) => {}
                    Err(e) => {
                        eprintln!("[magicore] prefetch spawn_blocking panicked: {e}");
                    }
                }
                Ok(len)
            });
        }
        let mut total = 0u64;
        while let Some(r) = set.join_next().await {
            total +=
                r.map_err(|e| mgc_types::MgError::Other(format!("prefetch task failed: {e}")))??;
        }
        Ok::<_, mgc_types::MgError>(total)
    })
}
