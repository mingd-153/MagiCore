//! Flutter pubspec.yaml and Podfile manifest parsing.

use mgc_types::{
    DependencySpec, Ecosystem, Manifest, MgError, MgResult, PackageName, VersionRange,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Parse pubspec.yaml to Manifest.
pub fn parse_pubspec(project_root: &Path) -> MgResult<Manifest> {
    let pubspec_path = project_root.join("pubspec.yaml");
    let content = std::fs::read_to_string(&pubspec_path)
        .map_err(|e| MgError::Other(format!("failed to read pubspec.yaml: {}", e)))?;

    let pubspec: PubspecYaml = serde_yaml::from_str(&content)
        .map_err(|e| MgError::Other(format!("failed to parse pubspec.yaml: {}", e)))?;

    let mut manifest = Manifest::new(&pubspec.name, Ecosystem::App);

    // Parse dependencies
    if let Some(deps) = pubspec.dependencies {
        for (name, value) in deps {
            if name == "flutter" {
                continue; // Skip Flutter SDK dependency
            }
            let version = parse_pubspec_version(&value);
            // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
            if let Ok(pkg_name) = PackageName::new(&name)
                && let Ok(range) = VersionRange::parse(&version)
            {
                manifest.add_dep(DependencySpec::new(pkg_name, range), false, false, false);
            }
        }
    }

    // Parse dev_dependencies
    if let Some(dev_deps) = pubspec.dev_dependencies {
        for (name, value) in dev_deps {
            let version = parse_pubspec_version(&value);
            // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
            if let Ok(pkg_name) = PackageName::new(&name)
                && let Ok(range) = VersionRange::parse(&version)
            {
                manifest.add_dep(DependencySpec::new(pkg_name, range), true, false, false);
            }
        }
    }

    Ok(manifest)
}

/// Write Manifest back to pubspec.yaml (native add/update). Rebuilds
/// the `dependencies` map from the manifest (exact pins stay exact);
/// every other key is preserved. A non-exact range fails closed —
/// callers pin via resolve-first. Git/path deps absent from the manifest
/// are dropped honestly (mgc manages versioned pins, not source refs).
/// (Ghi pubspec.yaml từ manifest.)
pub fn write_pubspec(project_root: &Path, manifest: &Manifest) -> MgResult<()> {
    let pubspec_path = project_root.join("pubspec.yaml");
    let content = std::fs::read_to_string(&pubspec_path)
        .map_err(|e| MgError::Other(format!("read pubspec.yaml: {e}")))?;
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&content)
        .map_err(|e| MgError::Other(format!("parse pubspec.yaml: {e}")))?;
    let mut deps = serde_yaml::Mapping::new();
    let mut dev_deps = serde_yaml::Mapping::new();
    // The flutter SDK pseudo-dep is environment, not a pin — the parser
    // skips it, so carry the ORIGINAL block forward verbatim (interop
    // with `flutter pub get` survives).
    if let Some(orig) = doc
        .as_mapping()
        .and_then(|m| m.get(serde_yaml::Value::String("dependencies".to_string())))
        .and_then(|d| d.as_mapping())
        .and_then(|m| m.get(serde_yaml::Value::String("flutter".to_string())))
    {
        deps.insert(
            serde_yaml::Value::String("flutter".to_string()),
            orig.clone(),
        );
    }
    let mut pins: Vec<(String, String, bool)> = Vec::new();
    for dep in manifest.all_dependencies() {
        let version = dep
            .range
            .satisfying_version()
            .map(|v| v.to_string())
            .unwrap_or_else(|| dep.range.to_string());
        let clean = version.trim_start_matches(['=', 'v', ' ']);
        mgc_types::Version::parse(clean).map_err(|_| {
            MgError::Other(format!(
                "pubspec writer needs an exact version for '{}', got '{}' — resolve-first must pin before save",
                dep.name.as_str(),
                dep.range
            ))
        })?;
        // Pinned manifest deps only (the SDK block was carried above).
        if dep.name.as_str() == "flutter" {
            continue;
        }
        pins.push((dep.name.as_str().to_string(), clean.to_string(), dep.dev));
    }
    pins.sort();
    pins.dedup();
    for (name, version, is_dev) in pins {
        // `^x.y.z` is Dart's caret-implied default (like `flutter pub add`).
        let target = if is_dev { &mut dev_deps } else { &mut deps };
        target.insert(
            serde_yaml::Value::String(name),
            serde_yaml::Value::String(format!("^{version}")),
        );
    }
    if let Some(map) = doc.as_mapping_mut() {
        map.insert(
            serde_yaml::Value::String("dependencies".to_string()),
            serde_yaml::Value::Mapping(deps),
        );
        if !dev_deps.is_empty() {
            map.insert(
                serde_yaml::Value::String("dev_dependencies".to_string()),
                serde_yaml::Value::Mapping(dev_deps),
            );
        }
    }
    std::fs::write(
        &pubspec_path,
        serde_yaml::to_string(&doc)
            .map_err(|e| MgError::Other(format!("serialize pubspec.yaml: {e}")))?,
    )
    .map_err(|e| MgError::Other(format!("write pubspec.yaml: {e}")))?;
    Ok(())
}

/// Parse CocoaPods Podfile (for ObjC).
pub fn parse_podfile(project_root: &Path) -> MgResult<Manifest> {
    let podfile_path = project_root.join("Podfile");
    if !podfile_path.exists() {
        let name = project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "app".to_string());
        return Ok(Manifest::new(&name, Ecosystem::App));
    }

    // Issue #13: Parse Podfile (Ruby DSL format)
    let name = project_root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "app".to_string());
    Ok(Manifest::new(&name, Ecosystem::App))
}

/// Write Manifest back to Podfile.
pub fn write_podfile(_project_root: &Path, _manifest: &Manifest) -> MgResult<()> {
    // Issue #13: Implement Podfile write
    Ok(())
}

/// Parse pubspec version value (can be string or object).
fn parse_pubspec_version(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Mapping(m) => {
            // Handle git/path dependencies
            if let Some(serde_yaml::Value::String(s)) =
                m.get(serde_yaml::Value::String("version".to_string()))
            {
                return s.clone();
            }
            "*".to_string()
        }
        _ => "*".to_string(),
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct PubspecYaml {
    name: String,
    #[serde(default)]
    dependencies: Option<HashMap<String, serde_yaml::Value>>,
    #[serde(default)]
    dev_dependencies: Option<HashMap<String, serde_yaml::Value>>,
}
