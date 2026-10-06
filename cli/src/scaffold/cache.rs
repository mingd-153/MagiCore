//! Versioned scaffold cache (~/.mgc/scaffolds/{core}/{name}/{version}/).

use anyhow::{Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

use super::spec::ScaffoldSpec;

fn cache_root_with_override(override_root: Option<PathBuf>, default_root: PathBuf) -> PathBuf {
    override_root.unwrap_or(default_root)
}

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
        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        if let Ok(entries) = fs::read_dir(&base) {
            for entry in entries.flatten() {
                if entry.path().is_dir()
                    && let Some(name) = entry.file_name().to_str()
                {
                    versions.push(name.to_string());
                }
            }
        }

        // Sort versions (reverse semver order: newest first)
        // Using lexicographic sort as acceptable approximation for v1.1.0
        // (semver crate adds 50KB+ to binary; deferred to v1.2.0 if needed)
        versions.sort_by(|a, b| b.cmp(a));

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

    /// Root of the versioned scaffold cache; MGC_SCAFFOLDS_DIR overrides the platform default.
    /// Gốc cache template có version; MGC_SCAFFOLDS_DIR ghi đè vị trí mặc định của hệ điều hành.
    fn cache_root() -> PathBuf {
        let default_root = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mgc")
            .join("scaffolds");
        let override_root = std::env::var_os("MGC_SCAFFOLDS_DIR").map(PathBuf::from);
        cache_root_with_override(override_root, default_root)
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

    #[test]
    fn explicit_cache_root_overrides_the_platform_default() {
        let explicit = tempfile::tempdir().unwrap();
        let default = tempfile::tempdir().unwrap();
        assert_eq!(
            cache_root_with_override(
                Some(explicit.path().to_path_buf()),
                default.path().to_path_buf()
            ),
            explicit.path()
        );
    }

    #[test]
    fn missing_cache_override_keeps_the_platform_default() {
        let default = tempfile::tempdir().unwrap();
        assert_eq!(
            cache_root_with_override(None, default.path().to_path_buf()),
            default.path()
        );
    }
}
