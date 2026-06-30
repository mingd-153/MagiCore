//! Resolution Pipeline: wanted deps -> PubGrub -> Lockfile

use std::path::Path;

use base64::Engine;
use mgpm_core::PackageName;
use mgpm_registry::RegistryClient;
use mgpm_resolver::Resolver;
use sha2::{Digest, Sha256};

use crate::lockfile::Lockfile;
use crate::LockfileError;

/// Input for resolution: packages we want to install
#[derive(Debug, Clone)]
pub struct WantedDependency {
    pub name: PackageName,
    pub version_req: String,  // e.g., "^1.0.0"
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
    let hash = Sha256::digest(tarball_bytes);
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
        let wanted_strs: Vec<(mgpm_core::PackageName, String)> = wanted
            .iter()
            .map(|w| (w.name.clone(), w.version_req.clone()))
            .collect();
        
        // Run resolver
        let result = self.resolver.solve(&wanted_strs)
            .map_err(|e| PipelineError::ResolveError(e.to_string()))?;
        
        // Build lockfile from resolutions with real integrity hashes
        let mut lockfile = Lockfile::new(self.config.config_version, &self.config.registry);
        
        for res in result.resolutions {
            let mut pkg = crate::lockfile::LockfilePackage::from_resolver_resolution(&res, &self.config.registry);
            
            if let Some(client) = registry_client {
                // Download tarball to compute real SRI hash
                let tarball_bytes = client
                    .download_tarball(&res.package_id)
                    .await
                    .map_err(|e| PipelineError::DownloadError(e.to_string()))?;
                pkg.integrity = Some(compute_package_integrity(&tarball_bytes));
            }
            // When no registry client, keep integrity from resolver (may be empty)
            
            lockfile.add_package(pkg);
        }
        
        lockfile.sort_packages();
        lockfile.compute_content_hash();
        lockfile.update_timestamp();
        
        // Write lockfile
        let lockfile_path = project_root.join("mgpm.lock");
        crate::text::write_text(&lockfile, &lockfile_path)
            .map_err(|e| PipelineError::LockfileWrite(e.to_string()))?;
        
        let binary_path = project_root.join("mgpm.lockb");
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
    use mgpm_core::PackageName;
    
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
