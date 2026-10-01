//! Cargo.toml manifest helpers for MagiCore-owned Rust dependency operations.
//! Cargo is allowed at compiler/test boundaries, not as a dependency manager.
//! (Helper Cargo.toml cho dependency operation do MGC sở hữu; Cargo chỉ được
//! dùng ở biên compiler/test, không được dùng làm package manager.)

use mgc_types::error::MgResult;
use mgc_types::package::{DependencySpec, PackageName, VersionRange};
use mgc_types::{Ecosystem, Manifest, Version};
use std::path::Path;

pub fn parse_manifest(root: &Path, ecosystem: Ecosystem) -> MgResult<Manifest> {
    let content = crate::project_file::read_regular_text(&root.join("Cargo.toml"), "Cargo.toml")?;
    let v: toml::Value = toml::from_str(&content)
        .map_err(|e| mgc_types::MgError::Other(format!("parse Cargo.toml: {e}")))?;
    let name = v
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown")
        .to_string();
    let mut manifest = Manifest::new(&name, ecosystem);
    manifest.version = v
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|n| n.as_str())
        .and_then(|s| Version::parse(s).ok());

    // The current lock/resolver model does not represent Cargo's build or
    // target-specific dependency groups. Installing only the root tables
    // would produce an incomplete graph while appearing successful.
    // (Mô hình lock/resolver hiện chưa biểu diễn build/target dependency;
    // chỉ cài bảng root sẽ tạo graph thiếu nhưng dễ bị hiểu là thành công.)
    if v.get("build-dependencies")
        .and_then(toml::Value::as_table)
        .is_some_and(|table| !table.is_empty())
    {
        return Err(unsupported_manifest(
            "Cargo build-dependencies are not yet supported by the native resolver",
        ));
    }
    if v.get("target")
        .and_then(toml::Value::as_table)
        .is_some_and(|targets| {
            targets.values().any(|target| {
                ["dependencies", "dev-dependencies", "build-dependencies"]
                    .into_iter()
                    .any(|key| {
                        target
                            .get(key)
                            .and_then(toml::Value::as_table)
                            .is_some_and(|table| !table.is_empty())
                    })
            })
        })
    {
        return Err(unsupported_manifest(
            "Cargo target-specific dependencies are not yet supported by the native resolver",
        ));
    }
    if v.get("features")
        .and_then(toml::Value::as_table)
        .is_some_and(|features| {
            features.values().any(|value| match value {
                toml::Value::Array(items) => !items.is_empty(),
                toml::Value::Table(items) => !items.is_empty(),
                _ => true,
            })
        })
    {
        return Err(unsupported_manifest(
            "Cargo feature activation is not yet modeled by the native resolver",
        ));
    }

    let mut add_deps = |table: Option<&toml::Value>, dev: bool| -> MgResult<()> {
        let Some(table) = table else { return Ok(()) };
        let Some(deps) = table.as_table() else {
            return Ok(());
        };
        for (dep_name, spec) in deps {
            if dep_name == "magicore" {
                continue;
            }
            let range = match spec {
                toml::Value::String(s) => s.clone(),
                toml::Value::Table(t) => {
                    if ["path", "git", "workspace", "package", "registry"]
                        .iter()
                        .any(|key| t.contains_key(*key))
                    {
                        return Err(unsupported_manifest(&format!(
                            "Cargo dependency '{dep_name}' uses a path/git/workspace/alias/registry source that the native resolver does not support"
                        )));
                    }
                    if t.get("optional").and_then(toml::Value::as_bool) == Some(true)
                        || t.get("default-features").and_then(toml::Value::as_bool) == Some(false)
                        || t.get("features")
                            .and_then(toml::Value::as_array)
                            .is_some_and(|features| !features.is_empty())
                    {
                        return Err(unsupported_manifest(&format!(
                            "Cargo dependency '{dep_name}' uses feature/optional semantics that the native resolver does not yet model"
                        )));
                    }
                    t.get("version")
                        .and_then(toml::Value::as_str)
                        .ok_or_else(|| {
                            unsupported_manifest(&format!(
                                "Cargo dependency '{dep_name}' has no registry version requirement"
                            ))
                        })?
                        .to_string()
                }
                _ => "*".to_string(),
            };
            let dep = DependencySpec {
                name: PackageName::new(dep_name)?,
                range: VersionRange::parse(&range)?,
                dev,
                optional: false,
                peer: false,
            };
            manifest.add_dep(dep, dev, false, false);
        }
        Ok(())
    };
    add_deps(v.get("dependencies"), false)?;
    add_deps(v.get("dev-dependencies"), true)?;
    Ok(manifest)
}

fn unsupported_manifest(reason: &str) -> mgc_types::MgError {
    mgc_types::MgError::Unsupported {
        core: "lib",
        capability: "native Cargo dependency resolution",
        guidance: reason.to_string(),
    }
}

pub fn write_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    let path = root.join("Cargo.toml");
    let content = crate::project_file::read_regular_text(&path, "Cargo.toml")?;
    let mut v: toml::Value = toml::from_str(&content)
        .map_err(|e| mgc_types::MgError::Other(format!("parse Cargo.toml: {e}")))?;
    let root_table = v
        .as_table_mut()
        .ok_or_else(|| mgc_types::MgError::Other("Cargo.toml not a table".into()))?;

    // Update only Cargo's two root dependency tables. Rebuilding them from
    // the simplified MGC Manifest used to erase Cargo-specific options
    // (`features`, `optional`, `default-features`) and silently drop
    // build/target dependencies from the user's project.
    // (Chỉ cập nhật hai bảng dependency root; dựng lại từ Manifest giản
    // lược từng làm mất option Cargo và dependency build/target.)
    sync_dependency_table(root_table, "dependencies", &manifest.dependencies)?;
    sync_dependency_table(root_table, "dev-dependencies", &manifest.dev_dependencies)?;

    let rendered =
        toml::to_string_pretty(&v).map_err(|e| mgc_types::MgError::Other(e.to_string()))?;
    crate::project_file::atomic_write_regular(&path, rendered.as_bytes())?;
    Ok(())
}

fn sync_dependency_table(
    root: &mut toml::Table,
    table_name: &str,
    desired: &[DependencySpec],
) -> MgResult<()> {
    let desired_versions: std::collections::BTreeMap<String, String> = desired
        .iter()
        .map(|dependency| {
            let version = if dependency.range.is_star() {
                "*".to_string()
            } else {
                dependency.range.as_str().to_string()
            };
            (dependency.name.as_str().to_string(), version)
        })
        .collect();

    if !root.contains_key(table_name) {
        if desired_versions.is_empty() {
            return Ok(());
        }
        root.insert(
            table_name.to_string(),
            toml::Value::Table(toml::Table::new()),
        );
    }
    let table = root
        .get_mut(table_name)
        .and_then(toml::Value::as_table_mut)
        .ok_or_else(|| {
            mgc_types::MgError::Other(format!("Cargo.toml [{table_name}] is not a table"))
        })?;

    let removed: Vec<String> = table
        .keys()
        .filter(|name| name.as_str() != "magicore" && !desired_versions.contains_key(*name))
        .cloned()
        .collect();
    for name in removed {
        table.remove(&name);
    }

    for (name, version) in desired_versions {
        match table.get_mut(&name) {
            Some(toml::Value::String(current)) => *current = version,
            Some(toml::Value::Table(spec)) => {
                if spec.contains_key("path")
                    || spec.contains_key("git")
                    || spec.contains_key("workspace")
                    || spec.contains_key("package")
                    || spec.contains_key("registry")
                {
                    return Err(mgc_types::MgError::Unsupported {
                        core: "lib",
                        capability: "native Cargo dependency mutation",
                        guidance: format!(
                            "dependency '{name}' uses a Cargo path/git/workspace/alias/registry source that the current native resolver cannot preserve safely"
                        ),
                    });
                }
                spec.insert("version".to_string(), toml::Value::String(version));
            }
            Some(_) => {
                return Err(mgc_types::MgError::Other(format!(
                    "Cargo.toml dependency '{name}' has an unsupported specification"
                )));
            }
            None => {
                table.insert(name, toml::Value::String(version));
            }
        }
    }
    Ok(())
}
