//! `manifest.rs` — `package.json` reading, writing and manifest conversion.
//!
//! Provides `PackageJson` structure, atomic file write utilities, and manifest parse/serialization.

use mg_types::{DependencySpec, Manifest, MgError, MgResult, PackageName, Version, VersionRange};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

/// Ghi file nguyên tử (Atomic write) — tránh lỗi hỏng file khi crash giữa chừng
pub fn atomic_write(path: &Path, data: &[u8]) -> MgResult<()> {
    let dir = path.parent().unwrap_or(Path::new("."));

    let tmp_path = dir.join(format!(
        ".mg-tmp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));

    if path.exists() {
        let backup_path = path.with_extension("bak");
        let _ = std::fs::copy(path, &backup_path);
    }

    std::fs::write(&tmp_path, data).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        MgError::Other(format!("failed to write temp file: {e}"))
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp_path)
            .map(|m| m.permissions())
            .unwrap_or_else(|_| std::fs::Permissions::from_mode(0o644));
        perms.set_mode(0o644);
        let _ = std::fs::set_permissions(&tmp_path, perms);
    }

    std::fs::rename(&tmp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        MgError::Other(format!("failed to rename temp file: {e}"))
    })?;

    Ok(())
}

/// Chỉ ghi file nếu nội dung thay đổi (Atomic write if changed)
pub fn atomic_write_if_changed(path: &Path, data: &[u8]) -> MgResult<bool> {
    if let Ok(existing) = std::fs::read(path) {
        if existing == data {
            return Ok(false);
        }
    }

    atomic_write(path, data)?;
    Ok(true)
}

/// Cấu trúc `package.json` của hệ sinh thái Node/Web
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageJson {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "devDependencies")]
    pub dev_dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "peerDependencies")]
    pub peer_dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "optionalDependencies"
    )]
    pub optional_dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl PackageJson {
    pub fn new(name: String, version: String) -> Self {
        Self {
            name,
            version,
            description: None,
            dependencies: None,
            dev_dependencies: None,
            peer_dependencies: None,
            optional_dependencies: None,
            extra: Map::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self, anyhow::Error> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), anyhow::Error> {
        let content = serde_json::to_string_pretty(self)?;
        atomic_write_if_changed(path, content.as_bytes())?;
        Ok(())
    }
}

pub fn is_workspace_protocol_range(range: &str) -> bool {
    let r = range.trim();
    r.starts_with("workspace:") || r == "workspace:*" || r == "workspace:^" || r == "workspace:~"
}

pub fn parse_manifest(project_root: &Path) -> MgResult<Manifest> {
    let pkg_path = project_root.join("package.json");
    if !pkg_path.exists() {
        return Err(MgError::Other(format!(
            "No package.json in '{}'. Run 'mg init --template web' first.",
            project_root.display()
        )));
    }
    const MAX_MANIFEST_SIZE: u64 = 10 * 1024 * 1024; // 10MB
    let metadata = std::fs::metadata(&pkg_path)?;
    if metadata.len() > MAX_MANIFEST_SIZE {
        return Err(MgError::Other(format!(
            "package.json is too large ({} bytes, max {})",
            metadata.len(),
            MAX_MANIFEST_SIZE
        )));
    }
    let pkg_json: PackageJson = serde_json::from_str(&std::fs::read_to_string(&pkg_path)?)?;
    let mut manifest = Manifest::new(&pkg_json.name, mg_types::ecosystem::Ecosystem::Web);
    manifest.version = Some(Version::parse(&pkg_json.version).map_err(|_| {
        MgError::Other(format!(
            "invalid version '{}' in package.json",
            pkg_json.version
        ))
    })?);
    let parse_deps =
        |map: Option<std::collections::HashMap<String, String>>| -> MgResult<Vec<DependencySpec>> {
            match map {
                Some(deps) => {
                    let mut out = Vec::with_capacity(deps.len());
                    for (name, range) in deps {
                        if is_workspace_protocol_range(&range) {
                            continue;
                        }
                        let pn = PackageName::new(name)?;
                        let vr = VersionRange::parse(&range)?;
                        out.push(DependencySpec::new(pn, vr));
                    }
                    Ok(out)
                }
                None => Ok(vec![]),
            }
        };
    manifest.dependencies = parse_deps(pkg_json.dependencies)?;
    manifest.dev_dependencies = parse_deps(pkg_json.dev_dependencies)?;
    manifest.peer_dependencies = parse_deps(pkg_json.peer_dependencies)?;
    manifest.optional_dependencies = parse_deps(pkg_json.optional_dependencies)?;
    Ok(manifest)
}

pub fn write_manifest(project_root: &Path, manifest: &Manifest) -> MgResult<()> {
    let to_map = |deps: &[DependencySpec]| -> std::collections::HashMap<String, String> {
        deps.iter()
            .map(|d| (d.name.as_str().to_string(), d.range.to_string()))
            .collect()
    };
    let pkg_path = project_root.join("package.json");
    let fallback_version = manifest
        .version
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "0.1.0".to_string());
    let existing = PackageJson::load(&pkg_path)
        .unwrap_or_else(|_| PackageJson::new(manifest.name.clone(), fallback_version));
    let pkg = PackageJson {
        name: manifest.name.clone(),
        version: manifest
            .version
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or(existing.version),
        description: existing.description,
        dependencies: if manifest.dependencies.is_empty() {
            None
        } else {
            Some(to_map(&manifest.dependencies))
        },
        dev_dependencies: if manifest.dev_dependencies.is_empty() {
            None
        } else {
            Some(to_map(&manifest.dev_dependencies))
        },
        peer_dependencies: if manifest.peer_dependencies.is_empty() {
            None
        } else {
            Some(to_map(&manifest.peer_dependencies))
        },
        optional_dependencies: if manifest.optional_dependencies.is_empty() {
            None
        } else {
            Some(to_map(&manifest.optional_dependencies))
        },
        extra: existing.extra,
    };
    pkg.save(&pkg_path)?;
    Ok(())
}
