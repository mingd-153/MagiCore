//! Resolution Pipeline: wanted deps -> PubGrub -> Lockfile

use std::path::{Path, PathBuf};

use mgpm_core::{PackageId, PackageName, Version, protocol::Protocol};
use mgpm_resolver::{Resolver, VersionSet, SolveError, SolveResult, Resolution as ResolverResolution};

use crate::lockfile::{Lockfile, LockfilePackage, PackageResolution};
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
    pub fn resolve_and_lock(
        &self,
        wanted: &[WantedDependency],
        project_root: &Path,
    ) -> Result<Lockfile, PipelineError> {
        // Convert wanted deps to (PackageName, String) for resolver
        let wanted_strs: Vec<(mgpm_core::PackageName, String)> = wanted
            .iter()
            .map(|w| (w.name.clone(), w.version_req.clone()))
            .collect();
        
        // Run resolver
        let result = self.resolver.solve(&wanted_strs)
            .map_err(|e| PipelineError::ResolveError(e.to_string()))?;
        
        // Build lockfile from resolutions
        let mut lockfile = Lockfile::new(self.config.config_version, &self.config.registry);
        
        for res in result.resolutions {
            let pkg = crate::lockfile::LockfilePackage::from_resolver_resolution(&res);
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
}
