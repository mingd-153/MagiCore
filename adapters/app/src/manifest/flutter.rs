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

    if pubspec
        .dependency_overrides
        .as_ref()
        .is_some_and(|overrides| !overrides.is_empty())
    {
        return Err(MgError::Unsupported {
            core: "app",
            capability: "Flutter dependency_overrides",
            guidance: "MagiCore does not yet implement pub dependency override semantics; remove dependency_overrides before native resolution".to_string(),
        });
    }
    if pubspec
        .workspace
        .as_ref()
        .is_some_and(|members| !members.is_empty())
    {
        return Err(MgError::Unsupported {
            core: "app",
            capability: "Flutter pub workspace resolution",
            guidance: "MagiCore does not yet resolve Dart pub workspaces as one graph; run this operation only after workspace support is implemented".to_string(),
        });
    }

    let mut manifest = Manifest::new(&pubspec.name, Ecosystem::App);

    // Parse dependencies
    if let Some(deps) = pubspec.dependencies {
        parse_dependency_section(&mut manifest, deps, false)?;
    }

    if let Some(dev_deps) = pubspec.dev_dependencies {
        parse_dependency_section(&mut manifest, dev_deps, true)?;
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
        return Err(MgError::Other("Podfile not found".to_string()));
    }

    Err(MgError::Unsupported {
        core: "app",
        capability: "parse-podfile",
        guidance: format!(
            "{} is executable Ruby DSL; MagiCore has no safe CocoaPods manifest parser",
            podfile_path.display()
        ),
    })
}

/// Write Manifest back to Podfile.
pub fn write_podfile(_project_root: &Path, _manifest: &Manifest) -> MgResult<()> {
    Err(MgError::Unsupported {
        core: "app",
        capability: "write-podfile",
        guidance: "MagiCore does not rewrite executable Ruby Podfile content".to_string(),
    })
}

/// Parse pubspec version value (can be string or object).
fn parse_dependency_section(
    manifest: &mut Manifest,
    dependencies: HashMap<String, serde_yaml::Value>,
    dev: bool,
) -> MgResult<()> {
    for (name, value) in dependencies {
        if let serde_yaml::Value::Mapping(mapping) = &value {
            let sdk = mapping
                .get(serde_yaml::Value::String("sdk".to_string()))
                .and_then(serde_yaml::Value::as_str);
            if let Some(sdk) = sdk {
                if mapping.len() != 1 {
                    return Err(MgError::Unsupported {
                        core: "app",
                        capability: "mixed Flutter SDK dependency source",
                        guidance: format!(
                            "dependency '{name}' combines an SDK source with other fields; MagiCore refuses to guess its resolution"
                        ),
                    });
                }
                // SDK dependencies (flutter, flutter_test, integration_test,
                // etc.) are provided by the selected Flutter/Dart SDK, not
                // packages from pub.dev.
                if !sdk.is_empty() {
                    continue;
                }
            }

            let source = ["path", "git", "hosted"]
                .into_iter()
                .find(|key| mapping.contains_key(serde_yaml::Value::String((*key).to_string())))
                .unwrap_or("non-registry");
            return Err(MgError::Unsupported {
                core: "app",
                capability: "Flutter non-pub.dev dependency source",
                guidance: format!(
                    "dependency '{name}' uses {source}; MagiCore currently resolves only pub.dev registry dependencies and will not substitute a same-name pub.dev package"
                ),
            });
        }

        let range = match value {
            serde_yaml::Value::String(range) if !range.trim().is_empty() => range,
            serde_yaml::Value::Null => "*".to_string(),
            _ => {
                return Err(MgError::Unsupported {
                    core: "app",
                    capability: "Flutter dependency constraint",
                    guidance: format!(
                        "dependency '{name}' has an unsupported pubspec value; expected a version string or an SDK dependency"
                    ),
                });
            }
        };
        let package = PackageName::new(&name).map_err(|error| MgError::Unsupported {
            core: "app",
            capability: "Flutter dependency name",
            guidance: format!("MagiCore cannot safely resolve dependency '{name}': {error}"),
        })?;
        let range = VersionRange::parse(&range).map_err(|error| MgError::Unsupported {
            core: "app",
            capability: "Flutter dependency range",
            guidance: format!("MagiCore cannot safely resolve '{name}': {error}"),
        })?;
        manifest.add_dep(DependencySpec::new(package, range), dev, false, false);
    }
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct PubspecYaml {
    name: String,
    #[serde(default)]
    dependencies: Option<HashMap<String, serde_yaml::Value>>,
    #[serde(default)]
    dev_dependencies: Option<HashMap<String, serde_yaml::Value>>,
    #[serde(default)]
    dependency_overrides: Option<HashMap<String, serde_yaml::Value>>,
    #[serde(default)]
    workspace: Option<Vec<String>>,
}
