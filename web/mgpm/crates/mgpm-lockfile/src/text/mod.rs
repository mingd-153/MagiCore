//! Text lockfile serialization (mgpm.lock)
//!
//! TOML format, human-readable, git-diffable

use std::fs;
use std::path::Path;

use serde::Serialize;

use super::{Lockfile, LockfilePackage};
use crate::{LockfileError, lockfile::LOCKFILE_VERSION_V1};

#[derive(Serialize)]
struct TomlLockfile {
    version: u32,
    metadata: TomlMetadata,
    packages: Vec<TomlPackage>,
}

#[derive(Serialize)]
struct TomlMetadata {
    config_version: u32,
    created_at: u64,
    updated_at: u64,
    content_hash: String,
    registry: String,
}

#[derive(Serialize)]
struct TomlPackage {
    id: String,
    name: String,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    integrity: Option<String>,
    resolution: TomlResolution,
}

#[derive(Serialize)]
struct TomlResolution {
    #[serde(rename = "type")]
    res_type: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    registry: Option<String>,
}

impl From<&Lockfile> for TomlLockfile {
    fn from(lf: &Lockfile) -> Self {
        Self {
            version: lf.version,
            metadata: TomlMetadata {
                config_version: lf.metadata.config_version,
                created_at: lf.metadata.created_at,
                updated_at: lf.metadata.updated_at,
                content_hash: lf.metadata.content_hash.clone(),
                registry: lf.metadata.registry.clone(),
            },
            packages: lf.packages.iter().map(TomlPackage::from).collect(),
        }
    }
}

impl From<&LockfilePackage> for TomlPackage {
    fn from(pkg: &LockfilePackage) -> Self {
        Self {
            id: pkg.id.clone(),
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            integrity: pkg.integrity.clone(),
            resolution: TomlResolution {
                res_type: pkg.resolution.r#type.clone(),
                url: pkg.resolution.url.clone(),
                registry: pkg.resolution.registry.clone(),
            },
        }
    }
}

pub fn write_text(lockfile: &Lockfile, path: &Path) -> Result<(), LockfileError> {
    let toml_lockfile = TomlLockfile::from(lockfile);
    let content = toml::to_string_pretty(&toml_lockfile)
        .map_err(|e: toml::ser::Error| LockfileError::Serialization(e.to_string()))?;
    
    fs::write(path, content)
        .map_err(|e: std::io::Error| LockfileError::Io(e.to_string()))?;
    Ok(())
}

pub fn read_text(path: &Path) -> Result<Lockfile, LockfileError> {
    let content = fs::read_to_string(path)
        .map_err(|e: std::io::Error| LockfileError::Io(e.to_string()))?;
    
    let value: toml::Value = content.parse()
        .map_err(|e: toml::de::Error| LockfileError::Deserialization(e.to_string()))?;
    
    let table = value.as_table()
        .ok_or_else(|| LockfileError::Corrupted("root is not a table".to_string()))?;
    
let version = table.get("version")
        .and_then(|v| v.as_integer())
        .unwrap_or(1) as u32;
    
    // Check if this is a v1 lockfile that needs migration
    let is_v1 = version == LOCKFILE_VERSION_V1;

    let metadata_table = table.get("metadata")
        .and_then(|m| m.as_table())
        .ok_or_else(|| LockfileError::Corrupted("missing metadata".to_string()))?;
    
    let metadata = super::LockfileMetadata {
        config_version: metadata_table.get("config_version")
            .and_then(|v| v.as_integer())
            .unwrap_or(1) as u32,
        created_at: metadata_table.get("created_at")
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as u64,
        updated_at: metadata_table.get("updated_at")
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as u64,
        content_hash: metadata_table.get("content_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        registry: metadata_table.get("registry")
            .and_then(|v| v.as_str())
            .unwrap_or("npm")
            .to_string(),
    };
    
    let packages_array = table.get("packages")
        .and_then(|p| p.as_array())
        .ok_or_else(|| LockfileError::Corrupted("missing packages array".to_string()))?;
    
    let mut packages = Vec::new();
    for pkg_value in packages_array {
        let pkg_table = pkg_value.as_table()
            .ok_or_else(|| LockfileError::Corrupted("package is not a table".to_string()))?;
        
        let res_table = pkg_table.get("resolution")
            .and_then(|r| r.as_table())
            .ok_or_else(|| LockfileError::Corrupted("missing resolution".to_string()))?;
        
        let resolution = super::PackageResolution {
            r#type: res_table.get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("registry")
                .to_string(),
            url: res_table.get("url")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string(),
            registry: res_table.get("registry").and_then(|r| r.as_str()).map(String::from),
        };
        
        packages.push(LockfilePackage {
            id: pkg_table.get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string(),
            name: pkg_table.get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string(),
            version: pkg_table.get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            resolution,
            integrity: pkg_table.get("integrity").and_then(|i| i.as_str()).map(String::from),
        });
    }
    
    let mut lockfile = Lockfile {
        version,
        metadata,
        packages,
    };
    
    // Auto-migrate v1 lockfiles to v2
    if is_v1 {
        lockfile.migrate_v1_to_v2()
            .map_err(|e| LockfileError::Corrupted(format!("v1 lockfile migration failed: {}", e)))?;
    }
    
    Ok(lockfile)
}

pub fn exists(path: &Path) -> bool {
    path.exists()
}

pub fn get_preferred_path(base: &Path) -> std::path::PathBuf {
    let text_path = base.with_extension(super::LOCKFILE_TEXT_EXT);
    let binary_path = base.with_extension(super::LOCKFILE_BINARY_EXT);
    
    if text_path.exists() && !binary_path.exists() {
        return text_path;
    }
    
    if binary_path.exists() && !text_path.exists() {
        return binary_path;
    }
    
    if text_path.exists() && binary_path.exists() {
        let text_modified = fs::metadata(&text_path)
            .and_then(|m| m.modified())
            .ok();
        let binary_modified = fs::metadata(&binary_path)
            .and_then(|m| m.modified())
            .ok();
        
        if let (Some(text_time), Some(binary_time)) = (text_modified, binary_modified) {
            if text_time > binary_time {
                return text_path;
            }
        }
    }
    
    text_path
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_text_roundtrip() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("mgpm.lock");
        
        let mut lock = Lockfile::new(1, "npm");
        lock.add_package(LockfilePackage {
            id: "react@18.0.0".to_string(),
            name: "react".to_string(),
            version: "18.0.0".to_string(),
            resolution: super::super::PackageResolution {
                r#type: "registry".to_string(),
                url: "https://registry.npmjs.org/react/-/react-18.0.0.tgz".to_string(),
                registry: Some("npm".to_string()),
            },
            integrity: Some("sha512-...".to_string()),
        });
        
        write_text(&lock, &path).unwrap();
        let loaded = read_text(&path).unwrap();
        
        assert_eq!(loaded.packages.len(), 1);
        assert_eq!(loaded.packages[0].name, "react");
        assert_eq!(loaded.packages[0].version, "18.0.0");
    }
}