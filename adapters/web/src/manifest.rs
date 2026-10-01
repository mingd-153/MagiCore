//! `manifest.rs` — `package.json` reading, writing and manifest conversion.
//!
//! Provides `PackageJson` structure, atomic file write utilities, and manifest parse/serialization.

use mgc_types::{DependencySpec, Manifest, MgError, MgResult, PackageName, Version, VersionRange};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

/// Ghi file nguyên tử (Atomic write) — tránh lỗi hỏng file khi crash giữa chừng
pub fn atomic_write(path: &Path, data: &[u8]) -> MgResult<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            mgc_adapter_base::project_file::atomic_write_regular(path, data)
        }
        Ok(_) => Err(MgError::Other(format!(
            "refusing to atomically write non-regular or linked path: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => atomic_create_new(path, data),
        Err(error) => Err(MgError::Other(format!(
            "inspect atomic-write destination {}: {error}",
            path.display()
        ))),
    }
}

/// Publish a previously absent file through a unique same-directory staging file.
/// File mới được publish từ staging duy nhất cùng thư mục.
fn atomic_create_new(path: &Path, data: &[u8]) -> MgResult<()> {
    use std::io::Write;

    let parent = path.parent().unwrap_or(Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| MgError::Other(format!("invalid write path: {}", path.display())))?;
    let temp = parent.join(format!(
        ".{}.mgc-tmp-{}-{}",
        file_name.to_string_lossy(),
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o644).custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options
            .open(&temp)
            .map_err(|error| MgError::Other(format!("create atomic staging file: {error}")))?;
        file.write_all(data)
            .and_then(|()| file.sync_all())
            .map_err(|error| MgError::Other(format!("sync atomic staging file: {error}")))?;
        match std::fs::symlink_metadata(path) {
            Ok(_) => {
                return Err(MgError::Other(format!(
                    "atomic-write destination appeared during publication: {}",
                    path.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(MgError::Other(format!(
                    "recheck atomic-write destination {}: {error}",
                    path.display()
                )));
            }
        }
        mgc_lockfile::atomic::atomic_replace_file(&temp, path)
            .map_err(|error| MgError::Other(format!("publish atomic file: {error}")))?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| MgError::Other(format!("sync atomic-write directory: {error}")))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Chỉ ghi file nếu nội dung thay đổi (Atomic write if changed)
pub fn atomic_write_if_changed(path: &Path, data: &[u8]) -> MgResult<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            let existing = mgc_adapter_base::project_file::read_regular_text(path, "package.json")?;
            if existing.as_bytes() == data {
                return Ok(false);
            }
        }
        Ok(_) => {
            return Err(MgError::Other(format!(
                "refusing to update non-regular or linked path: {}",
                path.display()
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(MgError::Other(format!(
                "inspect update destination {}: {error}",
                path.display()
            )));
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
        let content = mgc_adapter_base::project_file::read_regular_text(path, "package.json")?;
        Ok(serde_json::from_str(&content)?)
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
            "No package.json in '{}'. Run 'mgc init --template web' first.",
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
    let mut manifest = Manifest::new(&pkg_json.name, mgc_types::ecosystem::Ecosystem::Web);
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
    let existing = match std::fs::symlink_metadata(&pkg_path) {
        Ok(_) => PackageJson::load(&pkg_path)
            .map_err(|error| MgError::Other(format!("read existing package.json: {error}")))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            PackageJson::new(manifest.name.clone(), fallback_version)
        }
        Err(error) => {
            return Err(MgError::Other(format!("inspect package.json: {error}")));
        }
    };
    let merge_workspace = |deps: &[DependencySpec],
                           old: Option<&std::collections::HashMap<String, String>>|
     -> MgResult<Option<std::collections::HashMap<String, String>>> {
        let mut entries = to_map(deps);
        if let Some(old) = old {
            for (name, range) in old {
                if is_workspace_protocol_range(range) {
                    if entries.contains_key(name) {
                        return Err(MgError::Unsupported {
                            core: "web",
                            capability: "replace workspace dependency source",
                            guidance: format!(
                                "dependency '{name}' is currently workspace-owned; MagiCore refuses to silently replace its source with a registry pin"
                            ),
                        });
                    }
                    entries.entry(name.clone()).or_insert_with(|| range.clone());
                }
            }
        }
        Ok((!entries.is_empty()).then_some(entries))
    };
    let pkg = PackageJson {
        name: manifest.name.clone(),
        version: manifest
            .version
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or(existing.version),
        description: existing.description,
        dependencies: merge_workspace(&manifest.dependencies, existing.dependencies.as_ref())?,
        dev_dependencies: merge_workspace(
            &manifest.dev_dependencies,
            existing.dev_dependencies.as_ref(),
        )?,
        peer_dependencies: merge_workspace(
            &manifest.peer_dependencies,
            existing.peer_dependencies.as_ref(),
        )?,
        optional_dependencies: merge_workspace(
            &manifest.optional_dependencies,
            existing.optional_dependencies.as_ref(),
        )?,
        extra: existing.extra,
    };
    pkg.save(&pkg_path)?;
    Ok(())
}
