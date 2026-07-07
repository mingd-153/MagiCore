//! Resolution Pipeline: wanted deps -> PubGrub -> Lockfile

use std::path::Path;

use base64::Engine;
use mg_core::PackageName;
use mg_registry::RegistryClient;
use mg_resolver::Resolver;

use crate::lockfile::Lockfile;
use crate::LockfileError;

/// Input for resolution: packages we want to install
#[derive(Debug, Clone)]
pub struct WantedDependency {
    pub name: PackageName,
    pub version_req: String, // e.g., "^1.0.0"
    pub dev: bool,
    pub optional: bool,
}

/// Configuration for resolution
#[derive(Debug, Clone)]
pub struct ResolutionConfig {
    pub registry: String,
    pub config_version: u32,
    pub offline: bool,
    pub prefer_offline: bool,
}

impl Default for ResolutionConfig {
    fn default() -> Self {
        Self {
            registry: "https://registry.npmjs.org".to_string(),
            config_version: 1,
            offline: false,
            prefer_offline: false,
        }
    }
}

/// Compute real SHA-256 integrity hash from tarball bytes (SRI format: sha256-<base64>)
pub fn compute_package_integrity(tarball_bytes: &[u8]) -> String {
    use mg_core::cffi::sha256::Hasher;
    let mut hasher = Hasher::new();
    hasher.update(tarball_bytes);
    let hash = hasher.final_raw();
    let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(hash);
    format!("sha256-{}", b64)
}

/// Resolution pipeline that coordinates resolver + lockfile generation
pub struct ResolutionPipeline {
    resolver: Resolver,
    config: ResolutionConfig,
}

impl ResolutionPipeline {
    pub fn new(resolver: Resolver, config: ResolutionConfig) -> Self {
        Self { resolver, config }
    }

    /// Run full resolution: wanted -> resolved -> lockfile
    ///
    /// When `registry_client` is `Some`, it downloads tarballs to compute real SRI hashes.
    /// When `None`, integrity values from the resolver are kept as-is (useful for testing).
    pub async fn resolve_and_lock(
        &self,
        wanted: &[WantedDependency],
        project_root: &Path,
        registry_client: Option<&RegistryClient>,
    ) -> Result<Lockfile, PipelineError> {
        // Convert wanted deps to (PackageName, String) for resolver
        let wanted_strs: Vec<(mg_core::PackageName, String)> = wanted
            .iter()
            .map(|w| (w.name.clone(), w.version_req.clone()))
            .collect();

        let resolver = self.resolver.clone();
        let result = resolver.solve(&wanted_strs)
            .await
            .map_err(|e| PipelineError::ResolveError(e.to_string()))?;

        // Build lockfile from resolutions with real integrity hashes
        let mut lockfile = Lockfile::new(self.config.config_version, &self.config.registry);

        // Download tarballs in parallel for integrity computation
        let integrity_map: std::collections::HashMap<String, Option<String>> = if self.config.offline {
            // Offline mode: skip tarball downloads, use placeholder integrity from resolver
            result.resolutions.iter().map(|res| {
                (res.package_id.to_string(), None)
            }).collect()
        } else if let Some(client) = registry_client {
            use futures_util::future::try_join_all;

            let futures: Vec<_> = result.resolutions.iter().map(|res| {
                let id = res.package_id.clone();
                async move {
                    let bytes = client
                        .download_tarball(&id)
                        .await
                        .map_err(|e| PipelineError::DownloadError(e.to_string()))?;
                    let integrity = compute_package_integrity(&bytes);
                    Ok::<(String, String), PipelineError>((id.to_string(), integrity))
                }
            }).collect();
            let results: Vec<(String, String)> = try_join_all(futures)
                .await?;
            results.into_iter().map(|(k, v)| (k, Some(v))).collect()
        } else {
            std::collections::HashMap::new()
        };

        for res in result.resolutions {
            let mut pkg = crate::lockfile::LockfilePackage::from_resolver_resolution(
                &res,
                &self.config.registry,
            );

            if let Some(integrity) = integrity_map.get(&res.package_id.to_string()) {
                pkg.integrity = integrity.clone();
            }

            lockfile.add_package(pkg);
        }

        lockfile.sort_packages();
        lockfile.compute_content_hash();
        lockfile.update_timestamp();

        // Write lockfile
        let lockfile_path = project_root.join("mg.lock");
        crate::text::write_text(&lockfile, &lockfile_path)
            .map_err(|e| PipelineError::LockfileWrite(e.to_string()))?;

        let binary_path = project_root.join("mg.lockb");
        crate::binary::write_binary(&lockfile, &binary_path)
            .map_err(|e| PipelineError::LockfileWrite(e.to_string()))?;

        Ok(lockfile)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("resolve error: {0}")]
    ResolveError(String),
    #[error("download error: {0}")]
    DownloadError(String),
    #[error("lockfile write error: {0}")]
    LockfileWrite(String),
    #[error("lockfile error: {0}")]
    LockfileError(#[from] LockfileError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_config_default() {
        let config = ResolutionConfig::default();
        assert_eq!(config.registry, "https://registry.npmjs.org");
        assert!(!config.offline);
    }

    #[test]
    fn test_compute_package_integrity() {
        let data = b"test tarball content";
        let integrity = compute_package_integrity(data);
        assert!(integrity.starts_with("sha256-"));
        assert_eq!(integrity.len(), "sha256-".len() + 43); // base64 URL-safe no-pad: 32 bytes = 43 chars
    }
}
