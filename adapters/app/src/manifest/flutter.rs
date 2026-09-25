//! Flutter pubspec.yaml and Podfile manifest parsing.

use mgc_types::{
    DependencySpec, Ecosystem, Manifest, MgError, MgResult, PackageName, VersionRange,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};

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

/// Expand dependencies declared by Flutter SDK packages into the Pub graph.
/// The SDK supplies package source; MGC still resolves registry packages.
/// Mở rộng dependency của package Flutter SDK vào graph Pub; MGC vẫn tự resolve package registry.
pub(crate) fn expand_flutter_sdk_dependencies(
    project_root: &Path,
    manifest: &Manifest,
) -> MgResult<Manifest> {
    let declared = declared_flutter_sdk_dependencies(project_root)?;
    if declared.is_empty() {
        return Ok(manifest.clone());
    }
    let sdk_root = discover_flutter_sdk_root()?;
    expand_flutter_sdk_dependencies_with_root(project_root, manifest, &sdk_root)
}

pub(crate) fn flutter_sdk_packages(
    project_root: &Path,
) -> MgResult<(Option<PathBuf>, Vec<String>)> {
    if declared_flutter_sdk_dependencies(project_root)?.is_empty() {
        return Ok((None, Vec::new()));
    }
    let sdk_root = discover_flutter_sdk_root()?;
    let packages = flutter_sdk_package_names(project_root, &sdk_root)?;
    Ok((Some(sdk_root), packages))
}

pub(crate) fn flutter_sdk_package_names(
    project_root: &Path,
    sdk_root: &Path,
) -> MgResult<Vec<String>> {
    let declared = declared_flutter_sdk_dependencies(project_root)?;
    if declared.is_empty() {
        return Ok(Vec::new());
    }
    let (names, _) = read_flutter_sdk_dependency_closure(
        project_root,
        sdk_root,
        &Manifest::new("sdk", Ecosystem::App),
        declared,
    )?;
    Ok(names)
}

pub(crate) fn expand_flutter_sdk_dependencies_with_root(
    project_root: &Path,
    manifest: &Manifest,
    sdk_root: &Path,
) -> MgResult<Manifest> {
    let declared = declared_flutter_sdk_dependencies(project_root)?;
    if declared.is_empty() {
        return Ok(manifest.clone());
    }
    let (names, expanded) =
        read_flutter_sdk_dependency_closure(project_root, sdk_root, manifest, declared)?;
    let _ = names;
    Ok(expanded)
}

fn declared_flutter_sdk_dependencies(project_root: &Path) -> MgResult<Vec<(String, bool)>> {
    let path = project_root.join("pubspec.yaml");
    let content = std::fs::read_to_string(&path)
        .map_err(|error| MgError::Other(format!("read pubspec.yaml: {error}")))?;
    let pubspec: serde_yaml::Value = serde_yaml::from_str(&content)
        .map_err(|error| MgError::Other(format!("parse pubspec.yaml: {error}")))?;
    let Some(root) = pubspec.as_mapping() else {
        return Err(MgError::Integrity(
            "pubspec.yaml must be a mapping".to_string(),
        ));
    };
    let mut packages = BTreeMap::new();
    for (section, dev) in [("dependencies", false), ("dev_dependencies", true)] {
        let Some(dependencies) = root
            .get(serde_yaml::Value::String(section.to_string()))
            .and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        for (name, value) in dependencies {
            let Some(fields) = value.as_mapping() else {
                continue;
            };
            let Some(sdk) = fields
                .get(serde_yaml::Value::String("sdk".to_string()))
                .and_then(serde_yaml::Value::as_str)
            else {
                continue;
            };
            if sdk != "flutter" || fields.len() != 1 {
                return Err(MgError::Unsupported {
                    core: "app",
                    capability: "non-Flutter SDK dependency",
                    guidance: "MagiCore can currently bind only dependencies supplied by the selected Flutter SDK".to_string(),
                });
            }
            let name = name.as_str().ok_or_else(|| {
                MgError::Integrity("Flutter SDK dependency name must be a string".to_string())
            })?;
            validate_flutter_sdk_package_name(name)?;
            packages
                .entry(name.to_string())
                .and_modify(|existing_dev| *existing_dev &= dev)
                .or_insert(dev);
        }
    }
    Ok(packages.into_iter().collect())
}

fn read_flutter_sdk_dependency_closure(
    project_root: &Path,
    sdk_root: &Path,
    base_manifest: &Manifest,
    declared: Vec<(String, bool)>,
) -> MgResult<(Vec<String>, Manifest)> {
    let canonical_sdk_root = sdk_root
        .canonicalize()
        .map_err(|error| MgError::Other(format!("canonicalize Flutter SDK root: {error}")))?;
    let project_manifest = std::fs::read_to_string(project_root.join("pubspec.yaml"))
        .map_err(|error| MgError::Other(format!("read pubspec.yaml: {error}")))?;
    let project_yaml: serde_yaml::Value = serde_yaml::from_str(&project_manifest)
        .map_err(|error| MgError::Other(format!("parse pubspec.yaml: {error}")))?;
    let mut project_dependencies = BTreeMap::new();
    for (section, dev) in [("dependencies", false), ("dev_dependencies", true)] {
        if let Some(dependencies) = project_yaml
            .get(section)
            .and_then(serde_yaml::Value::as_mapping)
        {
            for name in dependencies.keys().filter_map(serde_yaml::Value::as_str) {
                project_dependencies
                    .entry(name.to_string())
                    .and_modify(|existing_dev| *existing_dev &= dev)
                    .or_insert(dev);
            }
        }
    }

    let mut manifest = base_manifest.clone();
    let mut pending: VecDeque<(String, bool)> = declared.into();
    let mut package_dev_state: BTreeMap<String, bool> = BTreeMap::new();
    let mut sdk_names = BTreeSet::new();
    let mut registry_ranges = BTreeMap::<String, (VersionRange, bool)>::new();
    while let Some((name, dev)) = pending.pop_front() {
        validate_flutter_sdk_package_name(&name)?;
        if package_dev_state
            .get(&name)
            .is_some_and(|previous_dev| !*previous_dev || *previous_dev == dev)
        {
            continue;
        }
        package_dev_state.insert(name.clone(), dev);
        sdk_names.insert(name.clone());
        let package_root = flutter_sdk_package_root(&canonical_sdk_root, &name)?;
        let package_manifest_path = package_root.join("pubspec.yaml");
        let metadata = std::fs::symlink_metadata(&package_manifest_path).map_err(|error| {
            MgError::Other(format!(
                "inspect Flutter SDK package manifest '{}': {error}",
                package_manifest_path.display()
            ))
        })?;
        if !metadata.file_type().is_file() {
            return Err(MgError::Integrity(format!(
                "Flutter SDK package '{name}' has a linked or non-regular pubspec.yaml"
            )));
        }
        let content = std::fs::read_to_string(&package_manifest_path).map_err(|error| {
            MgError::Other(format!(
                "read Flutter SDK package '{name}' manifest: {error}"
            ))
        })?;
        let package_yaml: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
            MgError::Other(format!(
                "parse Flutter SDK package '{name}' manifest: {error}"
            ))
        })?;
        if package_yaml.get("name").and_then(serde_yaml::Value::as_str) != Some(name.as_str()) {
            return Err(MgError::Integrity(format!(
                "Flutter SDK package identity mismatch for '{name}'"
            )));
        }
        if package_yaml
            .get("dependency_overrides")
            .is_some_and(|overrides| !overrides.is_null())
            || package_yaml
                .get("workspace")
                .is_some_and(|workspace| !workspace.is_null())
        {
            return Err(MgError::Unsupported {
                core: "app",
                capability: "Flutter SDK package override/workspace semantics",
                guidance: format!(
                    "Flutter SDK package '{name}' declares dependency_overrides or workspace metadata that MagiCore cannot resolve natively"
                ),
            });
        }
        let dependencies = match package_yaml.get("dependencies") {
            None | Some(serde_yaml::Value::Null) => continue,
            Some(serde_yaml::Value::Mapping(dependencies)) => dependencies,
            Some(_) => {
                return Err(MgError::Integrity(format!(
                    "Flutter SDK package '{name}' dependencies must be a mapping"
                )));
            }
        };
        for (dependency_name, value) in dependencies {
            let dependency_name = dependency_name.as_str().ok_or_else(|| {
                MgError::Integrity("Flutter SDK dependency name must be a string".to_string())
            })?;
            if let Some(fields) = value.as_mapping() {
                let sdk = fields
                        .get(serde_yaml::Value::String("sdk".to_string()))
                        .and_then(serde_yaml::Value::as_str)
                        .ok_or_else(|| MgError::Unsupported {
                            core: "app",
                            capability: "Flutter SDK package dependency source",
                            guidance: format!("Flutter SDK package '{name}' uses an unsupported source for '{dependency_name}'"),
                        })?;
                if fields.len() != 1 || !matches!(sdk, "flutter" | "dart") {
                    return Err(MgError::Unsupported {
                        core: "app",
                        capability: "Flutter SDK package dependency source",
                        guidance: format!(
                            "Flutter SDK package '{name}' uses unsupported SDK source '{sdk}' for '{dependency_name}'"
                        ),
                    });
                }
                if sdk == "flutter" {
                    pending.push_back((dependency_name.to_string(), dev));
                }
                continue;
            }
            let raw_range = match value {
                serde_yaml::Value::String(range) if !range.trim().is_empty() => range.as_str(),
                serde_yaml::Value::Null => "*",
                _ => {
                    return Err(MgError::Unsupported {
                        core: "app",
                        capability: "Flutter SDK package dependency constraint",
                        guidance: format!(
                            "Flutter SDK package '{name}' has an unsupported constraint for '{dependency_name}'"
                        ),
                    });
                }
            };
            let package =
                PackageName::new(dependency_name).map_err(|error| MgError::Unsupported {
                    core: "app",
                    capability: "Flutter SDK dependency name",
                    guidance: format!(
                        "MagiCore cannot safely resolve '{dependency_name}': {error}"
                    ),
                })?;
            let requested_range =
                VersionRange::parse(raw_range).map_err(|error| MgError::Unsupported {
                    core: "app",
                    capability: "Flutter SDK dependency range",
                    guidance: format!(
                        "MagiCore cannot safely resolve '{dependency_name}': {error}"
                    ),
                })?;
            let previous = registry_ranges
                .get(dependency_name)
                .map(|(range, _)| range)
                .or_else(|| manifest.find_dep(dependency_name).map(|dep| &dep.range));
            let combined = match previous {
                Some(previous) if previous.is_star() => requested_range.to_string(),
                Some(previous) if requested_range.is_star() => previous.to_string(),
                Some(previous) if previous == &requested_range => previous.to_string(),
                Some(previous)
                    if previous.as_str().contains("||")
                        || requested_range.as_str().contains("||") =>
                {
                    return Err(MgError::Unsupported {
                        core: "app",
                        capability: "Flutter SDK disjunctive dependency constraints",
                        guidance: format!(
                            "MagiCore cannot safely intersect disjunctive constraints for '{dependency_name}' from the project and Flutter SDK metadata"
                        ),
                    });
                }
                Some(previous) => format!("{} {}", previous.as_str(), requested_range.as_str()),
                None => requested_range.to_string(),
            };
            let range = VersionRange::parse(&combined)?;
            let existing_sdk_dev = registry_ranges
                .get(dependency_name)
                .map(|(_, existing_dev)| *existing_dev)
                .unwrap_or(true);
            // A dependency is dev-only only when every incoming edge is
            // dev-only. A runtime edge from the Flutter SDK must promote a
            // project dev dependency into the runtime group.
            // (Chỉ dev khi mọi cạnh đều là dev; SDK dùng runtime phải promote.)
            let project_dev = project_dependencies
                .get(dependency_name)
                .copied()
                .unwrap_or(true);
            let effective_dev = project_dev && existing_sdk_dev && dev;
            registry_ranges.insert(dependency_name.to_string(), (range.clone(), effective_dev));
            manifest.add_dep(
                DependencySpec::new(package, range),
                effective_dev,
                false,
                false,
            );
        }
    }
    Ok((sdk_names.into_iter().collect(), manifest))
}

pub(crate) fn flutter_sdk_package_root(sdk_root: &Path, name: &str) -> MgResult<PathBuf> {
    let candidate = if name == "sky_engine" {
        sdk_root.join("bin/cache/pkg/sky_engine")
    } else {
        sdk_root.join("packages").join(name)
    };
    let canonical = candidate.canonicalize().map_err(|error| {
        MgError::Other(format!(
            "Flutter SDK package '{name}' is unavailable at '{}': {error}",
            candidate.display()
        ))
    })?;
    let canonical_root = sdk_root
        .canonicalize()
        .map_err(|error| MgError::Other(format!("canonicalize Flutter SDK root: {error}")))?;
    if !canonical.starts_with(&canonical_root) || !canonical.is_dir() {
        return Err(MgError::Integrity(format!(
            "Flutter SDK package '{name}' resolves outside the selected SDK"
        )));
    }
    Ok(canonical)
}

fn validate_flutter_sdk_package_name(name: &str) -> MgResult<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(MgError::Integrity(format!(
            "invalid Flutter SDK package name '{name}'"
        )));
    }
    Ok(())
}

pub(crate) fn discover_flutter_sdk_root() -> MgResult<PathBuf> {
    if let Some(root) = std::env::var_os("FLUTTER_ROOT").filter(|value| !value.is_empty()) {
        return PathBuf::from(root)
            .canonicalize()
            .map_err(|error| MgError::Other(format!("resolve FLUTTER_ROOT: {error}")));
    }
    let executable = which::which("flutter")
        .map_err(|error| MgError::Other(format!("find flutter on PATH: {error}")))?
        .canonicalize()
        .map_err(|error| MgError::Other(format!("resolve flutter executable: {error}")))?;
    for ancestor in executable.ancestors() {
        if ancestor.join("packages/flutter/pubspec.yaml").is_file() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Err(MgError::Other(
        "could not locate Flutter SDK root from FLUTTER_ROOT or flutter on PATH".to_string(),
    ))
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
    // Flutter SDK entries are supplied by the SDK, not version pins — retain
    // them in both sections so add/remove cannot erase flutter_test or
    // integration_test. Các SDK dependency không phải pin; giữ nguyên cả hai nhóm.
    if let Some(root) = doc.as_mapping() {
        for (section, target) in [
            ("dependencies", &mut deps),
            ("dev_dependencies", &mut dev_deps),
        ] {
            if let Some(original) = root
                .get(serde_yaml::Value::String(section.to_string()))
                .and_then(serde_yaml::Value::as_mapping)
            {
                for (name, value) in original {
                    let is_flutter_sdk = value
                        .as_mapping()
                        .and_then(|fields| fields.get(serde_yaml::Value::String("sdk".to_string())))
                        .and_then(serde_yaml::Value::as_str)
                        == Some("flutter");
                    if is_flutter_sdk {
                        target.insert(name.clone(), value.clone());
                    }
                }
            }
        }
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
                // Flutter SDK packages are supplied by the selected SDK,
                // then inserted into MGC's generated package_config.json.
                if !sdk.is_empty() {
                    if sdk == "flutter" {
                        continue;
                    }
                    return Err(MgError::Unsupported {
                        core: "app",
                        capability: "non-Flutter SDK dependency",
                        guidance: format!(
                            "dependency '{name}' uses SDK '{sdk}', which MagiCore cannot bind into its native package configuration"
                        ),
                    });
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
