use std::path::Path;

use mgpm_lockfile::{Lockfile, LockfilePackage, PackageResolution};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockfileFormat {
    Npm,
    Yarn,
    Pnpm,
    Bun,
}

impl LockfileFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Yarn => "yarn",
            Self::Pnpm => "pnpm",
            Self::Bun => "bun",
        }
    }
}

pub fn detect_format(path: &Path) -> Option<LockfileFormat> {
    let name = path.file_name()?.to_str()?;
    match name {
        "package-lock.json" => Some(LockfileFormat::Npm),
        "yarn.lock" => Some(LockfileFormat::Yarn),
        "pnpm-lock.yaml" => Some(LockfileFormat::Pnpm),
        "bun.lockb" => Some(LockfileFormat::Bun),
        _ => match path.extension()?.to_str()? {
            "json" => Some(LockfileFormat::Npm),
            "yaml" | "yml" => Some(LockfileFormat::Pnpm),
            "lockb" => Some(LockfileFormat::Bun),
            _ => None,
        },
    }
}

pub fn import_lockfile(path: &Path, format: LockfileFormat) -> Result<Lockfile, String> {
    match format {
        LockfileFormat::Npm => import_npm(path),
        LockfileFormat::Yarn => import_yarn(path),
        LockfileFormat::Pnpm => import_pnpm(path),
        LockfileFormat::Bun => import_bun(path),
    }
}

fn import_npm(path: &Path) -> Result<Lockfile, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("failed to parse {}: {}", path.display(), e))?;

    let mut lockfile = Lockfile::new(1, "npm");

    let packages = json
        .get("packages")
        .and_then(|p| p.as_object())
        .ok_or_else(|| "no 'packages' field in package-lock.json".to_string())?;

    for (key, val) in packages {
        if key.is_empty() {
            continue;
        }
        let name = val
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let pkg_name = key
            .trim_start_matches("node_modules/")
            .split('/')
            .next()
            .unwrap_or("");
        if pkg_name.is_empty() {
            continue;
        }
        let pkg_name = if key.contains("node_modules/") {
            key.split("node_modules/")
                .last()
                .unwrap_or(pkg_name)
        } else {
            pkg_name
        };
        let resolved = val.get("resolved").and_then(|r| r.as_str()).unwrap_or("");
        let integrity = val.get("integrity").and_then(|i| i.as_str());
        let version = val
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let pkg = LockfilePackage {
            id: format!("{}@{}", pkg_name, version),
            name: pkg_name.to_string(),
            version: version.to_string(),
            resolution: PackageResolution {
                r#type: "registry".to_string(),
                url: resolved.to_string(),
                registry: Some("npm".to_string()),
            },
            integrity: integrity.map(String::from),
        };
        lockfile.add_package(pkg);
    }

    lockfile.sort_packages();
    lockfile.compute_content_hash();
    lockfile.update_timestamp();
    Ok(lockfile)
}

fn import_yarn(path: &Path) -> Result<Lockfile, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;

    let mut lockfile = Lockfile::new(1, "npm");
    let mut lines = content.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("yarn lockfile v") {
            continue;
        }

        if let Some(spec) = trimmed.strip_suffix(':') {
            let spec = spec.trim();
            if let Some((name, _version_req)) = spec.rsplit_once('@') {
                let mut version = String::new();
                let mut resolved = String::new();
                let mut integrity: Option<String> = None;

                for prop_line in lines.by_ref() {
                    let prop_trimmed = prop_line.trim();
                    if prop_trimmed.is_empty() || prop_trimmed.starts_with('#') {
                        continue;
                    }
                    if !prop_line.starts_with("  ") {
                        break;
                    }

                    if let Some(v) = prop_trimmed.strip_prefix("version ") {
                        version = v.trim().trim_matches('"').to_string();
                    } else if let Some(r) = prop_trimmed.strip_prefix("resolved ") {
                        resolved = r.trim().trim_matches('"').to_string();
                    } else if let Some(i) = prop_trimmed.strip_prefix("integrity ") {
                        integrity = Some(i.trim().trim_matches('"').to_string());
                    } else if prop_trimmed.starts_with("dependencies ") || prop_trimmed.starts_with("optionalDependencies ") || prop_trimmed.starts_with("peerDependencies ") || prop_trimmed.starts_with("  ") || prop_trimmed.ends_with(':') {
                        continue;
                    }
                }

                if !version.is_empty() {
                    let pkg = LockfilePackage {
                        id: format!("{}@{}", name, version),
                        name: name.to_string(),
                        version: version.clone(),
                        resolution: PackageResolution {
                            r#type: "registry".to_string(),
                            url: resolved,
                            registry: Some("npm".to_string()),
                        },
                        integrity,
                    };
                    lockfile.add_package(pkg);
                }
            }
        }
    }

    lockfile.sort_packages();
    lockfile.compute_content_hash();
    lockfile.update_timestamp();
    Ok(lockfile)
}

fn import_pnpm(path: &Path) -> Result<Lockfile, String> {
    use serde_yaml::Value;

    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let yaml: Value = serde_yaml::from_str(&content)
        .map_err(|e| format!("failed to parse {}: {}", path.display(), e))?;

    let mut lockfile = Lockfile::new(1, "npm");

    if let Some(packages) = yaml.get("packages").and_then(|p| p.as_mapping()) {
        for (key, val) in packages {
            let key_str = key.as_str().unwrap_or("");
            if key_str.is_empty() {
                continue;
            }

            let version = val
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if version.is_empty() {
                continue;
            }

            let name = if let Some((n, _)) = key_str.split_once('@') {
                if n.starts_with('/') {
                    key_str
                        .trim_start_matches('/')
                        .split_once('@')
                        .map(|(n, _)| n)
                        .unwrap_or(key_str)
                } else {
                    n
                }
            } else {
                key_str
            };
            let name = name.trim_start_matches('/');

            if name.is_empty() {
                continue;
            }

            let integrity = val.get("resolution").and_then(|r| {
                if let Some(m) = r.as_mapping() {
                    m.get("integrity").and_then(|i| i.as_str()).map(String::from)
                } else {
                    None
                }
            });
            let resolved = integrity.clone().unwrap_or_default();

            let pkg = LockfilePackage {
                id: format!("{}@{}", name, version),
                name: name.to_string(),
                version: version.to_string(),
                resolution: PackageResolution {
                    r#type: "registry".to_string(),
                    url: resolved,
                    registry: Some("npm".to_string()),
                },
                integrity,
            };
            lockfile.add_package(pkg);
        }
    }

    lockfile.sort_packages();
    lockfile.compute_content_hash();
    lockfile.update_timestamp();
    Ok(lockfile)
}

fn import_bun(path: &Path) -> Result<Lockfile, String> {
    let data = std::fs::read(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;

    if data.len() < 8 {
        return Err("bun.lockb file too short".to_string());
    }

    let magic = &data[..8];
    if magic != b"bunlock\x00" && magic != b"bunlock\x01" {
        return Err(format!("invalid bun lockfile magic: {:?}", magic));
    }

    let compressed = &data[8..];
    use std::io::Read;
    let mut decoder = flate2::read::GzDecoder::new(compressed);
    let mut json_str = String::new();
    decoder
        .read_to_string(&mut json_str)
        .map_err(|e| format!("failed to decompress bun lockfile: {}", e))?;

    let json: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("failed to parse bun lockfile JSON: {}", e))?;

    let mut lockfile = Lockfile::new(1, "npm");

    if let Some(packages) = json.get("packages").and_then(|p| p.as_object()) {
        for (key, val) in packages {
            let version = val
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if version.is_empty() {
                continue;
            }

            let name = if let Some(idx) = key.rfind('@') {
                if idx == 0 {
                    key.as_str()
                } else {
                    &key[..idx]
                }
            } else {
                key.as_str()
            };

            let integrity = val.get("integrity").and_then(|i| i.as_str());
            let resolved = integrity.unwrap_or("");

            let pkg = LockfilePackage {
                id: format!("{}@{}", name, version),
                name: name.to_string(),
                version: version.to_string(),
                resolution: PackageResolution {
                    r#type: "registry".to_string(),
                    url: resolved.to_string(),
                    registry: Some("npm".to_string()),
                },
                integrity: integrity.map(String::from),
            };
            lockfile.add_package(pkg);
        }
    }

    lockfile.sort_packages();
    lockfile.compute_content_hash();
    lockfile.update_timestamp();
    Ok(lockfile)
}
