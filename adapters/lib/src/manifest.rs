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

/// Parse go.mod into a Manifest (read-only view for listing/audit/resolve —
/// mgc never rewrites go.mod, the go toolchain owns it). Dependencies keep
/// their FULL module path (PackageName's repo-path form) so the native
/// GoModProtocol can resolve them; the project's own `replace` directives
/// rewrite pins and `exclude` drops exact (module, version) pairs — the
/// effective module graph, honestly.
/// Đọc go.mod thành Manifest (chỉ đọc phục vụ list/audit/resolve — mgc
/// không bao giờ viết lại go.mod, go toolchain là chủ). Dependency giữ
/// NGUYÊN path module (dạng repo-path của PackageName) để GoModProtocol
/// native resolve được; directive `replace` của project viết lại pin và
/// `exclude` loại cặp (module, version) đúng đó — graph module hiệu lực,
/// trung thực.
pub(crate) fn parse_go_mod_manifest(root: &Path) -> MgResult<Manifest> {
    let content = std::fs::read_to_string(root.join("go.mod"))
        .map_err(|e| mgc_types::MgError::Other(format!("read go.mod: {e}")))?;
    let mut name = "unknown".to_string();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(module) = trimmed.strip_prefix("module ") {
            name = module.trim().to_string();
            break;
        }
    }
    // One shared parser with the native engine (require/replace/exclude,
    // single-line and block forms).
    // (Một parser dùng chung với engine native — require/replace/exclude,
    // dạng một dòng và khối.)
    let gomod = mgc_resolver::protocols::go::parse_go_mod(&content)?;

    let mut manifest = Manifest::new(&name, Ecosystem::Lib);
    for req in &gomod.requires {
        // `exclude` removes that exact (module, version) pin from the graph.
        // (`exclude` loại pin (module, version) đúng đó khỏi graph.)
        if gomod
            .excludes
            .iter()
            .any(|(p, v)| p == &req.path && v == &req.version)
        {
            continue;
        }
        // `replace` rewrites the pin — version-scoped when the directive
        // pins the old version.
        // (`replace` viết lại pin — theo version khi directive ghim version
        // cũ.)
        let mut path = req.path.clone();
        let mut version = req.version.clone();
        for rep in &gomod.replaces {
            let version_ok = rep.old_version.as_ref().is_none_or(|v| *v == req.version);
            if rep.old_path == path && version_ok {
                path = rep.new_path.clone();
                if let Some(nv) = &rep.new_version {
                    version = nv.clone();
                }
                break;
            }
        }
        if let Ok(dep_name) = PackageName::new(path) {
            let Ok(range) = VersionRange::parse(&version) else {
                continue;
            };
            let spec = DependencySpec::new(dep_name, range);
            manifest.add_dep(spec, false, false, false);
        }
    }
    Ok(manifest)
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

#[cfg(test)]
#[path = "test/manifest_test.rs"]
mod tests;
