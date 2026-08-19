//! Cargo.toml manifest helpers — parse/write cho cores orchestrate cargo (Q10)
//! (bevy/esp32-rust/rust-lib: mg add = cargo add + cargo fetch; write giữ [package.metadata.megagate])

use mg_types::error::MgResult;
use mg_types::package::{DependencySpec, PackageName, VersionRange};
use mg_types::{Ecosystem, Manifest, Version};
use std::path::Path;

pub fn parse_manifest(root: &Path, ecosystem: Ecosystem) -> MgResult<Manifest> {
    let content = std::fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|e| mg_types::MgError::Other(format!("read Cargo.toml: {e}")))?;
    let v: toml::Value = toml::from_str(&content)
        .map_err(|e| mg_types::MgError::Other(format!("parse Cargo.toml: {e}")))?;
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
    let mut add_deps = |table: Option<&toml::Value>, dev: bool| -> MgResult<()> {
        let Some(table) = table else { return Ok(()) };
        let Some(deps) = table.as_table() else {
            return Ok(());
        };
        for (dep_name, spec) in deps {
            if dep_name == "megagate" {
                continue;
            }
            let range = match spec {
                toml::Value::String(s) => s.clone(),
                toml::Value::Table(t) => t
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("*")
                    .to_string(),
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

pub fn write_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    let path = root.join("Cargo.toml");
    let content = std::fs::read_to_string(&path)
        .map_err(|e| mg_types::MgError::Other(format!("read Cargo.toml: {e}")))?;
    let mut v: toml::Value = toml::from_str(&content)
        .map_err(|e| mg_types::MgError::Other(format!("parse Cargo.toml: {e}")))?;
    let deps = v
        .as_table_mut()
        .ok_or_else(|| mg_types::MgError::Other("Cargo.toml not a table".into()))?;

    let mut deps_table = toml::Table::new();
    for dep in manifest.dependencies.iter().filter(|d| !d.range.is_star()) {
        deps_table.insert(
            dep.name.as_str().to_string(),
            toml::Value::String(dep.range.as_str().to_string()),
        );
    }
    deps.insert("dependencies".to_string(), toml::Value::Table(deps_table));

    let mut dev_table = toml::Table::new();
    for dep in manifest
        .dev_dependencies
        .iter()
        .filter(|d| !d.range.is_star())
    {
        dev_table.insert(
            dep.name.as_str().to_string(),
            toml::Value::String(dep.range.as_str().to_string()),
        );
    }
    deps.insert(
        "dev-dependencies".to_string(),
        toml::Value::Table(dev_table),
    );

    std::fs::write(
        &path,
        toml::to_string_pretty(&v).map_err(|e| mg_types::MgError::Other(e.to_string()))?,
    )
    .map_err(|e| mg_types::MgError::Other(format!("write Cargo.toml: {e}")))?;
    Ok(())
}
