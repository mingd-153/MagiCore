//! Versioned scaffold cache (~/.mgc/scaffolds/{core}/{name}/{version}/).

use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};

use super::spec::ScaffoldSpec;

/// Scaffold cache manager (versioned storage).
pub struct ScaffoldCache;

impl ScaffoldCache {
    /// Cache path for a specific scaffold version.
    ///
    /// Example: `~/.mgc/scaffolds/web/nextjs/15.5.0/`
    pub fn path(spec: &ScaffoldSpec, version: &str) -> PathBuf {
        Self::cache_root()
            .join(spec.core.as_str())
            .join(&spec.normalized_name)
            .join(version)
    }

    /// Check if a specific version is cached.
    pub fn has(spec: &ScaffoldSpec, version: &str) -> bool {
        let path = Self::path(spec, version);
        path.is_dir() && Self::has_template_contract(&path)
    }

    /// Write scaffold data to versioned cache.
    ///
    /// `data` is expected to be a tarball or zip that will be extracted.
    pub fn write(spec: &ScaffoldSpec, version: &str, tarball: &[u8]) -> Result<()> {
        let target = Self::path(spec, version);
        if target.exists() {
            // Already cached - skip
            return Ok(());
        }

        fs::create_dir_all(&target)?;

        // Extract tarball
        let decoder = flate2::read::GzDecoder::new(tarball);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(&target)?;

        // Write version metadata
        let version_file = target.join(".mgc-version");
        fs::write(version_file, version)?;

        Ok(())
    }

    /// Read cached scaffold path (returns directory path for layer resolution).
    pub fn read(spec: &ScaffoldSpec, version: &str) -> Result<PathBuf> {
        let path = Self::path(spec, version);
        if !path.is_dir() {
            bail!(
                "Scaffold cache miss: {}/{} version {}",
                spec.core.as_str(),
                spec.normalized_name,
                version
            );
        }
        Ok(path)
    }

    /// List all cached versions for a scaffold (sorted newest first).
    pub fn list_versions(spec: &ScaffoldSpec) -> Vec<String> {
        let base = Self::cache_root()
            .join(spec.core.as_str())
            .join(&spec.normalized_name);

        if !base.is_dir() {
            return vec![];
        }

        let mut versions = vec![];
        if let Ok(entries) = fs::read_dir(&base) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        versions.push(name.to_string());
                    }
                }
            }
        }

        // Sort versions (semver-aware if possible)
        versions.sort_by(|a, b| {
            // Simple string comparison for now (TODO: semver crate)
            b.cmp(a)
        });

        versions
    }

    /// Clear cache for a specific scaffold version.
    pub fn clear(spec: &ScaffoldSpec, version: &str) -> Result<()> {
        let path = Self::path(spec, version);
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        Ok(())
    }

    /// Clear all cached versions for a scaffold.
    pub fn clear_all(spec: &ScaffoldSpec) -> Result<()> {
        let base = Self::cache_root()
            .join(spec.core.as_str())
            .join(&spec.normalized_name);
        if base.exists() {
            fs::remove_dir_all(&base)?;
        }
        Ok(())
    }

    /// Root cache directory (~/.mgc/scaffolds/).
    fn cache_root() -> PathBuf {
        dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mgc")
            .join("scaffolds")
    }

    /// Check if directory contains template contract (template.toml).
    fn has_template_contract(dir: &Path) -> bool {
        if dir.join("template.toml").is_file() {
            return true;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && Self::has_template_contract(&path) {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scaffold::spec::{CoreKind, ScaffoldRef};

    #[test]
    fn test_cache_path_structure() {
        let spec = ScaffoldSpec {
            core: CoreKind::Web,
            name: "nextjs".to_string(),
            normalized_name: "nextjs".to_string(),
            requested_ref: ScaffoldRef::DistTag("latest".to_string()),
        };

        let path = ScaffoldCache::path(&spec, "15.5.0");
        assert!(path.ends_with("mgc/scaffolds/web/nextjs/15.5.0"));
    }

    #[test]
    fn test_list_versions_empty() {
        let spec = ScaffoldSpec {
            core: CoreKind::Web,
            name: "nonexistent".to_string(),
            normalized_name: "nonexistent".to_string(),
            requested_ref: ScaffoldRef::DistTag("latest".to_string()),
        };

        let versions = ScaffoldCache::list_versions(&spec);
        assert!(versions.is_empty());
    }
}
