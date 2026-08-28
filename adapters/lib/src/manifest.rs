//! Manifest parsing and writing for library projects.
//! Tách logic Cargo/Python manifest để adapter chính dễ maintain và mở rộng.

use mgc_types::{DependencySpec, Ecosystem, Manifest, MgResult, PackageName, VersionRange};
use std::path::Path;

pub(crate) fn parse_cargo_manifest(root: &Path) -> MgResult<Manifest> {
    mgc_adapter_base::cargo_manifest::parse_manifest(root, Ecosystem::Lib)
}

pub(crate) fn write_cargo_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    mgc_adapter_base::cargo_manifest::write_manifest(root, manifest)
}

pub(crate) fn parse_pyproject_manifest(root: &Path) -> MgResult<Manifest> {
    let content = std::fs::read_to_string(root.join("pyproject.toml"))
        .map_err(|e| mgc_types::MgError::Other(format!("read pyproject.toml: {e}")))?;
    let v: toml::Value = toml::from_str(&content)
        .map_err(|e| mgc_types::MgError::Other(format!("parse pyproject.toml: {e}")))?;
    let name = v
        .get("project")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown")
        .to_string();
    let mut manifest = Manifest::new(&name, Ecosystem::Lib);
    if let Some(deps) = v
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
    {
        for dep in deps {
            let spec = dep.as_str().unwrap_or_default();
            let dep = parse_python_dependency(spec)?;
            manifest.add_dep(dep, false, false, false);
        }
    }
    Ok(manifest)
}

fn parse_python_dependency(spec: &str) -> MgResult<DependencySpec> {
    let trimmed = spec.trim();
    let split_at = ["==", ">=", "<=", "~=", ">", "<"]
        .iter()
        .filter_map(|op| trimmed.find(op).map(|idx| (idx, *op)))
        .min_by_key(|(idx, _)| *idx);

    let Some((idx, op)) = split_at else {
        return DependencySpec::parse(trimmed);
    };

    let name = PackageName::new(trimmed[..idx].trim())?;
    let raw_range = trimmed[idx + op.len()..].trim();
    let range = match op {
        "==" => VersionRange::parse(raw_range)?,
        "~=" => VersionRange::parse(&format!("~{raw_range}"))?,
        _ => VersionRange::parse(&format!("{op}{raw_range}"))?,
    };
    Ok(DependencySpec::new(name, range))
}

pub(crate) fn write_pyproject_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    let path = root.join("pyproject.toml");
    let content = std::fs::read_to_string(&path)
        .map_err(|e| mgc_types::MgError::Other(format!("read pyproject.toml: {e}")))?;
    let mut v: toml::Value = toml::from_str(&content)
        .map_err(|e| mgc_types::MgError::Other(format!("parse pyproject.toml: {e}")))?;
    let project = v
        .as_table_mut()
        .and_then(|t| t.get_mut("project"))
        .and_then(|p| p.as_table_mut())
        .ok_or_else(|| mgc_types::MgError::Other("pyproject.toml missing [project]".to_string()))?;

    let deps = manifest
        .dependencies
        .iter()
        .filter(|d| !d.range.is_star())
        .map(|dep| {
            toml::Value::String(format!(
                "{}>={}",
                dep.name.as_str(),
                dep.range
                    .as_str()
                    .trim_start_matches('^')
                    .trim_start_matches('~')
                    .trim_start_matches('=')
            ))
        })
        .collect();
    project.insert("dependencies".to_string(), toml::Value::Array(deps));

    std::fs::write(
        &path,
        toml::to_string_pretty(&v).map_err(|e| mgc_types::MgError::Other(e.to_string()))?,
    )
    .map_err(|e| mgc_types::MgError::Other(format!("write pyproject.toml: {e}")))?;
    Ok(())
}
