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
            if let Ok(pkg_name) = PackageName::new(&name) {
                if let Ok(range) = VersionRange::parse(&version) {
                    manifest.add_dep(DependencySpec::new(pkg_name, range), false, false, false);
                }
            }
        }
    }

    // Parse dev_dependencies
    if let Some(dev_deps) = pubspec.dev_dependencies {
        for (name, value) in dev_deps {
            let version = parse_pubspec_version(&value);
            if let Ok(pkg_name) = PackageName::new(&name) {
                if let Ok(range) = VersionRange::parse(&version) {
                    manifest.add_dep(DependencySpec::new(pkg_name, range), true, false, false);
                }
            }
        }
    }

    Ok(manifest)
}

/// Write Manifest back to pubspec.yaml.
pub fn write_pubspec(_project_root: &Path, _manifest: &Manifest) -> MgResult<()> {
    // Issue #13: Implement write with YAML preservation
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
