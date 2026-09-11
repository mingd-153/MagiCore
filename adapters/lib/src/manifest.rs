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

/// Parse go.mod into a Manifest (read-only view for listing/audit — mgc
/// never rewrites go.mod, the go toolchain owns it).
/// Đọc go.mod thành Manifest (chỉ đọc phục vụ list/audit — mgc không bao
/// giờ viết lại go.mod, go toolchain là chủ).
pub(crate) fn parse_go_mod_manifest(root: &Path) -> MgResult<Manifest> {
    let content = std::fs::read_to_string(root.join("go.mod"))
        .map_err(|e| mgc_types::MgError::Other(format!("read go.mod: {e}")))?;
    let mut name = "unknown".to_string();
    let mut deps: Vec<(String, String)> = Vec::new();

    // Single pass over the lines: track the parenthesized require block
    // state; strip `// indirect`-style comments BEFORE splitting so a
    // trailing comment never contaminates the version. Only `require`
    // entries feed the dep set — exclude/replace blocks are ignored.
    // Một lượt duyệt theo dòng: theo dõi trạng thái block require trong
    // ngoặc; cắt comment `// indirect` TRƯỚC khi tách để comment cuối
    // không làm bẩn version. Chỉ entry `require` vào tập dep — bỏ
    // exclude/replace.
    let mut in_require_block = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(module) = trimmed.strip_prefix("module ") {
            name = module.trim().to_string();
            continue;
        }
        if trimmed.starts_with("require") {
            if trimmed.ends_with('(') {
                in_require_block = true;
                continue;
            }
            // Single-line require: `require path vX // indirect?`.
            // Require một dòng: `require path vX // indirect?`.
            if let Some(dep) = trimmed.strip_prefix("require ") {
                let no_comment = dep.split("//").next().unwrap_or("").trim();
                if let Some((path, version)) = no_comment.split_once(' ') {
                    deps.push((path.trim().to_string(), version.trim().to_string()));
                }
            }
            continue;
        }
        if in_require_block {
            if trimmed == ")" {
                in_require_block = false;
                continue;
            }
            let no_comment = trimmed.split("//").next().unwrap_or("").trim();
            if let Some((path, version)) = no_comment.split_once(' ') {
                deps.push((path.trim().to_string(), version.trim().to_string()));
            }
        }
    }

    let mut manifest = Manifest::new(&name, Ecosystem::Lib);
    for (path, version) in deps {
        // Go module paths with multiple slashes cannot be PackageName
        // (npm-scoped) — record the LAST segment; the full path rides the
        // spec's original string form where needed.
        // Path module Go nhiều slash không thành PackageName (npm-scoped)
        // — ghi ĐOẠN CUỐI; path đầy đủ nằm trong chuỗi gốc khi cần.
        let display = path.rsplit('/').next().unwrap_or(&path);
        if let Ok(name) = PackageName::new(display.to_string()) {
            let Ok(range) = VersionRange::parse(version.trim_start_matches('v')) else {
                continue;
            };
            let spec = DependencySpec::new(name, range);
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
