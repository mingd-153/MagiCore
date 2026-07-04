//! Integration tests for lockfile import from various package manager formats.

use std::io::{Read, Write};
use std::path::Path;

use flate2::write::GzEncoder;
use flate2::Compression;
use mg_lockfile::{Lockfile, LockfilePackage, PackageResolution};

fn detect_format(path: &Path) -> Option<&'static str> {
    let name = path.file_name()?.to_str()?;
    match name {
        "package-lock.json" => Some("npm"),
        "yarn.lock" => Some("yarn"),
        "pnpm-lock.yaml" => Some("pnpm"),
        "bun.lockb" => Some("bun"),
        _ => match path.extension()?.to_str()? {
            "json" => Some("npm"),
            "yaml" | "yml" => Some("pnpm"),
            "lockb" => Some("bun"),
            _ => None,
        },
    }
}

fn parse_npm_lock(path: &Path) -> Result<Lockfile, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("read error: {}", e))?;
    let json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("parse error: {}", e))?;

    let mut lockfile = Lockfile::new(1, "npm");
    let packages = json
        .get("packages")
        .and_then(|p| p.as_object())
        .ok_or_else(|| "no 'packages' field".to_string())?;

    for (key, val) in packages {
        if key.is_empty() {
            continue;
        }
        let version = val.get("version").and_then(|v| v.as_str()).unwrap_or("");
        if version.is_empty() {
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
        let resolved = val.get("resolved").and_then(|r| r.as_str()).unwrap_or("");
        let integrity = val.get("integrity").and_then(|i| i.as_str());

        lockfile.add_package(LockfilePackage {
            id: format!("{}@{}", pkg_name, version),
            name: pkg_name.to_string(),
            version: version.to_string(),
            resolution: PackageResolution {
                r#type: "registry".to_string(),
                url: resolved.to_string(),
                registry: Some("npm".to_string()),
            },
            integrity: integrity.map(String::from),
            dependencies: vec![],
            resolved: false,
            resolved_at: None,
        });
    }
    lockfile.sort_packages();
    lockfile.compute_content_hash();
    Ok(lockfile)
}

fn parse_yarn_lock(path: &Path) -> Result<Lockfile, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("read error: {}", e))?;

    let mut lockfile = Lockfile::new(1, "npm");
    let mut lines = content.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("yarn lockfile v")
        {
            continue;
        }

        if let Some(spec) = trimmed.strip_suffix(':') {
            let spec = spec.trim();
            if let Some((name, _version_req)) = spec.rsplit_once('@') {
                let mut version = String::new();
                let mut resolved = String::new();
                let mut integrity: Option<String> = None;

                while let Some(prop_line) = lines.peek() {
                    let prop_trimmed = prop_line.trim();
                    if prop_trimmed.is_empty() || prop_trimmed.starts_with('#') {
                        lines.next();
                        continue;
                    }
                    if !prop_line.starts_with("  ") {
                        break;
                    }
                    lines.next();

                    if let Some(v) = prop_trimmed.strip_prefix("version ") {
                        version = v.trim().trim_matches('"').to_string();
                    } else if let Some(r) = prop_trimmed.strip_prefix("resolved ") {
                        resolved = r.trim().trim_matches('"').to_string();
                    } else if let Some(i) = prop_trimmed.strip_prefix("integrity ") {
                        integrity = Some(i.trim().trim_matches('"').to_string());
                    }
                }

                if !version.is_empty() {
                    lockfile.add_package(LockfilePackage {
                        id: format!("{}@{}", name, version),
                        name: name.to_string(),
                        version: version.clone(),
                        resolution: PackageResolution {
                            r#type: "registry".to_string(),
                            url: resolved,
                            registry: Some("npm".to_string()),
                        },
                        integrity,
                        dependencies: vec![],
            resolved: false,
            resolved_at: None,
                    });
                }
            }
        }
    }

    lockfile.sort_packages();
    lockfile.compute_content_hash();
    Ok(lockfile)
}

fn parse_pnpm_lock(path: &Path) -> Result<Lockfile, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("read error: {}", e))?;
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|e| format!("parse error: {}", e))?;

    let mut lockfile = Lockfile::new(1, "npm");
    if let Some(packages) = yaml.get("packages").and_then(|p| p.as_mapping()) {
        for (key, val) in packages {
            let key_str = key.as_str().unwrap_or("");
            if key_str.is_empty() {
                continue;
            }

            let version = val.get("version").and_then(|v| v.as_str()).unwrap_or("");
            if version.is_empty() {
                continue;
            }

            let name = if let Some((n, _)) = key_str.split_once('@') {
                if n.starts_with('/') {
                    key_str
                        .trim_start_matches('/')
                        .rsplit_once('@')
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
                    m.get("integrity")
                        .and_then(|i| i.as_str())
                        .map(String::from)
                } else {
                    None
                }
            });

            lockfile.add_package(LockfilePackage {
                id: format!("{}@{}", name, version),
                name: name.to_string(),
                version: version.to_string(),
                resolution: PackageResolution {
                    r#type: "registry".to_string(),
                    url: integrity.clone().unwrap_or_default(),
                    registry: Some("npm".to_string()),
                },
                integrity,
                dependencies: vec![],
            resolved: false,
            resolved_at: None,
            });
        }
    }

    lockfile.sort_packages();
    lockfile.compute_content_hash();
    Ok(lockfile)
}

fn parse_bun_lock(path: &Path) -> Result<Lockfile, String> {
    let data = std::fs::read(path).map_err(|e| format!("read error: {}", e))?;

    if data.len() < 8 {
        return Err("file too short".to_string());
    }
    let magic = &data[..8];
    if magic != b"bunlock\x00" && magic != b"bunlock\x01" {
        return Err(format!("invalid bun magic: {:?}", magic));
    }

    let compressed = &data[8..];
    let mut decoder = flate2::read::GzDecoder::new(compressed);
    let mut json_str = String::new();
    decoder
        .read_to_string(&mut json_str)
        .map_err(|e| format!("decompress error: {}", e))?;

    let json: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| format!("parse error: {}", e))?;

    let mut lockfile = Lockfile::new(1, "npm");
    if let Some(packages) = json.get("packages").and_then(|p| p.as_object()) {
        for (key, val) in packages {
            let version = val.get("version").and_then(|v| v.as_str()).unwrap_or("");
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

            lockfile.add_package(LockfilePackage {
                id: format!("{}@{}", name, version),
                name: name.to_string(),
                version: version.to_string(),
                resolution: PackageResolution {
                    r#type: "registry".to_string(),
                    url: integrity.unwrap_or("").to_string(),
                    registry: Some("npm".to_string()),
                },
                integrity: integrity.map(String::from),
                dependencies: vec![],
            resolved: false,
            resolved_at: None,
            });
        }
    }

    lockfile.sort_packages();
    lockfile.compute_content_hash();
    Ok(lockfile)
}

// ---------------------------------------------------------------------------
// Test: detect_format
// ---------------------------------------------------------------------------

mod detect_format_tests {
    use super::*;

    #[test]
    fn test_detect_npm_by_name() {
        let p = Path::new("package-lock.json");
        assert_eq!(detect_format(p), Some("npm"));
    }

    #[test]
    fn test_detect_yarn_by_name() {
        let p = Path::new("yarn.lock");
        assert_eq!(detect_format(p), Some("yarn"));
    }

    #[test]
    fn test_detect_pnpm_by_name() {
        let p = Path::new("pnpm-lock.yaml");
        assert_eq!(detect_format(p), Some("pnpm"));
    }

    #[test]
    fn test_detect_bun_by_name() {
        let p = Path::new("bun.lockb");
        assert_eq!(detect_format(p), Some("bun"));
    }

    #[test]
    fn test_detect_by_extension_json() {
        let p = Path::new("some-lock.json");
        assert_eq!(detect_format(p), Some("npm"));
    }

    #[test]
    fn test_detect_by_extension_yaml() {
        let p = Path::new("some-lock.yaml");
        assert_eq!(detect_format(p), Some("pnpm"));
    }

    #[test]
    fn test_detect_by_extension_yml() {
        let p = Path::new("some-lock.yml");
        assert_eq!(detect_format(p), Some("pnpm"));
    }

    #[test]
    fn test_detect_by_extension_lockb() {
        let p = Path::new("custom.lockb");
        assert_eq!(detect_format(p), Some("bun"));
    }

    #[test]
    fn test_detect_unknown_returns_none() {
        let p = Path::new("some-file.txt");
        assert_eq!(detect_format(p), None);
    }

    #[test]
    fn test_detect_no_extension_returns_none() {
        let p = Path::new("LOCKFILE");
        assert_eq!(detect_format(p), None);
    }
}

// ---------------------------------------------------------------------------
// Test: npm package-lock.json import
// ---------------------------------------------------------------------------

mod npm_import_tests {
    use super::*;

    fn create_npm_lockfile(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("package-lock.json");
        let data = serde_json::json!({
            "name": "test",
            "lockfileVersion": 3,
            "packages": {
                "": {
                    "name": "test",
                    "version": "1.0.0"
                },
                "node_modules/react": {
                    "version": "18.2.0",
                    "resolved": "https://registry.npmjs.org/react/-/react-18.2.0.tgz",
                    "integrity": "sha512-abc123"
                },
                "node_modules/lodash": {
                    "version": "4.17.21",
                    "resolved": "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz",
                    "integrity": "sha512-def456"
                }
            }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&data).unwrap()).unwrap();
        path
    }

    #[test]
    fn test_import_npm_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_npm_lockfile(dir.path());
        let lockfile = parse_npm_lock(&path).unwrap();
        assert_eq!(lockfile.packages.len(), 2);
        assert!(lockfile.find_package("react", "18.2.0").is_some());
        assert!(lockfile.find_package("lodash", "4.17.21").is_some());
    }

    #[test]
    fn test_import_npm_package_count() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_npm_lockfile(dir.path());
        let lockfile = parse_npm_lock(&path).unwrap();
        assert_eq!(lockfile.packages.len(), 2);
    }

    #[test]
    fn test_import_npm_without_root_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.json");
        let data = serde_json::json!({
            "packages": {
                "node_modules/express": {
                    "version": "4.19.2",
                    "resolved": "https://registry.npmjs.org/express/-/express-4.19.2.tgz",
                    "integrity": "sha512-xyz"
                }
            }
        });
        std::fs::write(&path, serde_json::to_string(&data).unwrap()).unwrap();
        let lockfile = parse_npm_lock(&path).unwrap();
        assert_eq!(lockfile.packages.len(), 1);
    }

    #[test]
    fn test_import_npm_scoped_package() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("package-lock.json");
        let data = serde_json::json!({
            "packages": {
                "node_modules/@scope/my-pkg": {
                    "version": "1.0.0",
                    "resolved": "https://registry.npmjs.org/@scope/my-pkg/-/my-pkg-1.0.0.tgz",
                    "integrity": "sha512-scope"
                }
            }
        });
        std::fs::write(&path, serde_json::to_string(&data).unwrap()).unwrap();
        let lockfile = parse_npm_lock(&path).unwrap();
        assert_eq!(lockfile.packages.len(), 1);
    }

    #[test]
    fn test_import_npm_missing_integrity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("package-lock.json");
        let data = serde_json::json!({
            "packages": {
                "node_modules/no-integrity": {
                    "version": "1.0.0",
                    "resolved": "https://registry.npmjs.org/no-integrity/-/no-integrity-1.0.0.tgz"
                }
            }
        });
        std::fs::write(&path, serde_json::to_string(&data).unwrap()).unwrap();
        let lockfile = parse_npm_lock(&path).unwrap();
        let pkg = lockfile.find_package("no-integrity", "1.0.0").unwrap();
        assert!(pkg.integrity.is_none());
    }

    #[test]
    fn test_import_npm_missing_packages_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("package-lock.json");
        std::fs::write(&path, r#"{"name":"empty"}"#).unwrap();
        let result = parse_npm_lock(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_import_npm_empty_packages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("package-lock.json");
        let data = serde_json::json!({ "packages": {} });
        std::fs::write(&path, serde_json::to_string(&data).unwrap()).unwrap();
        let lockfile = parse_npm_lock(&path).unwrap();
        assert!(lockfile.packages.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Test: yarn.lock import
// ---------------------------------------------------------------------------

mod yarn_import_tests {
    use super::*;

    fn create_yarn_lockfile(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("yarn.lock");
        let content = r#"# THIS IS AN AUTOGENERATED FILE. DO NOT EDIT THIS FILE DIRECTLY.
# yarn lockfile v1

react@^18.2.0:
  version "18.2.0"
  resolved "https://registry.npmjs.org/react/-/react-18.2.0.tgz"
  integrity sha512-abc123

lodash@^4.17.21:
  version "4.17.21"
  resolved "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz"
  integrity sha512-def456
"#;
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_import_yarn_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_yarn_lockfile(dir.path());
        let lockfile = parse_yarn_lock(&path).unwrap();
        assert_eq!(lockfile.packages.len(), 2);
        assert!(lockfile.find_package("react", "18.2.0").is_some());
        assert!(lockfile.find_package("lodash", "4.17.21").is_some());
    }

    #[test]
    fn test_import_yarn_scoped_package() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("yarn.lock");
        let content = r#"@scope/pkg@^1.0.0:
  version "1.0.0"
  resolved "https://registry.npmjs.org/@scope/pkg/-/pkg-1.0.0.tgz"
  integrity sha512-scope
"#;
        std::fs::write(&path, content).unwrap();
        let lockfile = parse_yarn_lock(&path).unwrap();
        assert_eq!(lockfile.packages.len(), 1);
    }

    #[test]
    fn test_import_yarn_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("yarn.lock");
        std::fs::write(&path, "# yarn lockfile v1\n").unwrap();
        let lockfile = parse_yarn_lock(&path).unwrap();
        assert!(lockfile.packages.is_empty());
    }

    #[test]
    fn test_import_yarn_missing_version_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("yarn.lock");
        let content = r#"missing-version@^1.0.0:
  resolved "https://registry.npmjs.org/missing-version/-/missing-version-1.0.0.tgz"
"#;
        std::fs::write(&path, content).unwrap();
        let lockfile = parse_yarn_lock(&path).unwrap();
        assert!(lockfile.packages.is_empty());
    }

    #[test]
    fn test_import_yarn_with_dependencies_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("yarn.lock");
        let content = r#"pkg@^1.0.0:
  version "1.0.0"
  resolved "https://registry.npmjs.org/pkg/-/pkg-1.0.0.tgz"
  integrity sha512-abc
  dependencies:
    dep "^1.0.0"
"#;
        std::fs::write(&path, content).unwrap();
        let lockfile = parse_yarn_lock(&path).unwrap();
        assert_eq!(lockfile.packages.len(), 1);
        let pkg = lockfile.find_package("pkg", "1.0.0").unwrap();
        assert_eq!(pkg.name, "pkg");
    }
}

// ---------------------------------------------------------------------------
// Test: pnpm-lock.yaml import
// ---------------------------------------------------------------------------

mod pnpm_import_tests {
    use super::*;

    fn create_pnpm_lockfile(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("pnpm-lock.yaml");
        let content = r#"lockfileVersion: '6.0'

packages:
  /react@18.2.0:
    version: 18.2.0
    resolution:
      integrity: sha512-abc123
    dev: false

  /lodash@4.17.21:
    version: 4.17.21
    resolution:
      integrity: sha512-def456
    dev: false
"#;
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_import_pnpm_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_pnpm_lockfile(dir.path());
        let lockfile = parse_pnpm_lock(&path).unwrap();
        assert_eq!(lockfile.packages.len(), 2);
        assert!(lockfile.find_package("react", "18.2.0").is_some());
        assert!(lockfile.find_package("lodash", "4.17.21").is_some());
    }

    #[test]
    fn test_import_pnpm_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pnpm-lock.yaml");
        std::fs::write(&path, "lockfileVersion: '6.0'\npackages: {}\n").unwrap();
        let lockfile = parse_pnpm_lock(&path).unwrap();
        assert!(lockfile.packages.is_empty());
    }

    #[test]
    fn test_import_pnpm_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pnpm-lock.yaml");
        let content = r#"lockfileVersion: '6.0'
packages:
  '/@scope/pkg@1.0.0':
    version: 1.0.0
    resolution:
      integrity: sha512-scope
"#;
        std::fs::write(&path, content).unwrap();
        let lockfile = parse_pnpm_lock(&path).unwrap();
        assert_eq!(lockfile.packages.len(), 1);
    }

    #[test]
    fn test_import_pnpm_no_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pnpm-lock.yaml");
        let content = r#"lockfileVersion: '6.0'
packages:
  /naked@1.0.0:
    version: 1.0.0
"#;
        std::fs::write(&path, content).unwrap();
        let lockfile = parse_pnpm_lock(&path).unwrap();
        assert_eq!(lockfile.packages.len(), 1);
        let pkg = lockfile.find_package("naked", "1.0.0").unwrap();
        assert!(pkg.integrity.is_none());
    }

    #[test]
    fn test_import_pnpm_empty_packages_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pnpm-lock.yaml");
        std::fs::write(&path, "lockfileVersion: '6.0'\n").unwrap();
        let lockfile = parse_pnpm_lock(&path).unwrap();
        assert!(lockfile.packages.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Test: bun.lockb import
// ---------------------------------------------------------------------------

mod bun_import_tests {
    use super::*;

    fn create_bun_lockfile(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("bun.lockb");
        let packages = serde_json::json!({
            "packages": {
                "react@18.2.0": {
                    "version": "18.2.0",
                    "integrity": "sha512-abc123"
                },
                "lodash@4.17.21": {
                    "version": "4.17.21",
                    "integrity": "sha512-def456"
                }
            }
        });
        let json_str = serde_json::to_string(&packages).unwrap();

        let mut data = Vec::new();
        data.extend_from_slice(b"bunlock\x00");
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(json_str.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();
        data.extend_from_slice(&compressed);

        std::fs::write(&path, &data).unwrap();
        path
    }

    #[test]
    fn test_import_bun_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_bun_lockfile(dir.path());
        let lockfile = parse_bun_lock(&path).unwrap();
        assert_eq!(lockfile.packages.len(), 2);
        assert!(lockfile.find_package("react", "18.2.0").is_some());
        assert!(lockfile.find_package("lodash", "4.17.21").is_some());
    }

    #[test]
    fn test_import_bun_too_short() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bun.lockb");
        std::fs::write(&path, &[0u8; 4]).unwrap();
        let result = parse_bun_lock(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too short"));
    }

    #[test]
    fn test_import_bun_invalid_magic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bun.lockb");
        std::fs::write(&path, &[0u8; 12]).unwrap();
        let result = parse_bun_lock(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("magic"));
    }

    #[test]
    fn test_import_bun_scoped_package() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bun.lockb");
        let packages = serde_json::json!({
            "packages": {
                "@scope/my-pkg@1.0.0": {
                    "version": "1.0.0",
                    "integrity": "sha512-scope"
                }
            }
        });
        let mut data = Vec::new();
        data.extend_from_slice(b"bunlock\x01");
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(serde_json::to_string(&packages).unwrap().as_bytes())
            .unwrap();
        data.extend_from_slice(&encoder.finish().unwrap());
        std::fs::write(&path, &data).unwrap();

        let lockfile = parse_bun_lock(&path).unwrap();
        assert_eq!(lockfile.packages.len(), 1);
    }

    #[test]
    fn test_import_bun_empty_packages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bun.lockb");
        let packages = serde_json::json!({ "packages": {} });
        let mut data = Vec::new();
        data.extend_from_slice(b"bunlock\x00");
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(serde_json::to_string(&packages).unwrap().as_bytes())
            .unwrap();
        data.extend_from_slice(&encoder.finish().unwrap());
        std::fs::write(&path, &data).unwrap();

        let lockfile = parse_bun_lock(&path).unwrap();
        assert!(lockfile.packages.is_empty());
    }

    #[test]
    fn test_import_bun_no_integrity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bun.lockb");
        let packages = serde_json::json!({
            "packages": {
                "naked@1.0.0": {
                    "version": "1.0.0"
                }
            }
        });
        let mut data = Vec::new();
        data.extend_from_slice(b"bunlock\x00");
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(serde_json::to_string(&packages).unwrap().as_bytes())
            .unwrap();
        data.extend_from_slice(&encoder.finish().unwrap());
        std::fs::write(&path, &data).unwrap();

        let lockfile = parse_bun_lock(&path).unwrap();
        let pkg = lockfile.find_package("naked", "1.0.0").unwrap();
        assert!(pkg.integrity.is_none());
    }
}
