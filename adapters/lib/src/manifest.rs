//! Manifest parsing and writing for library projects.
//! Tách logic Cargo/Python manifest để adapter chính dễ maintain và mở rộng.

use crate::language::find_csproj;
use mgc_types::{DependencySpec, Ecosystem, Manifest, MgResult, PackageName, VersionRange};
use std::path::Path;

pub(crate) fn parse_cargo_manifest(root: &Path) -> MgResult<Manifest> {
    mgc_adapter_base::cargo_manifest::parse_manifest(root, Ecosystem::Lib)
}

/// Parse a project pom.xml into a Manifest (read-only — mgc never rewrites
/// pom.xml). Only the SAME compile/runtime deps the native engine uses enter
/// the manifest, keyed as `groupId:artifactId`; `${property}`/missing
/// versions and test/provided scopes are honestly skipped (they would fail
/// resolution anyway). Gradle projects get an honest empty manifest — build
/// scripts are not parseable without the toolchain and resolve fails closed
/// downstream with guidance.
/// Đọc pom.xml của project thành Manifest (chỉ đọc — mgc không bao giờ viết
/// lại pom.xml). Chỉ những dep compile/runtime mà engine native dùng vào
/// manifest, khóa theo `groupId:artifactId`; version `${property}`/thiếu và
/// scope test/provided được skip trung thực (chúng sẽ fail resolve anyway).
/// Project gradle nhận manifest rỗng trung thực — build script không parse
/// được nếu không có toolchain và resolve sẽ fail-closed kèm hướng dẫn.
pub(crate) fn parse_maven_manifest(root: &Path) -> MgResult<Manifest> {
    let pom_path = root.join("pom.xml");
    if !pom_path.is_file() {
        return Ok(Manifest::new("java-gradle-lib", Ecosystem::Lib));
    }
    let content = std::fs::read_to_string(&pom_path)
        .map_err(|e| mgc_types::MgError::Other(format!("read pom.xml: {e}")))?;
    let (group, artifact) = mgc_resolver::protocols::maven::pom_project_coordinates(&content)
        .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));
    let mut manifest = Manifest::new(&format!("{group}:{artifact}"), Ecosystem::Lib);
    let (deps, _) = mgc_resolver::protocols::maven::collect_pom_dependencies(&content);
    for dep in deps {
        let scope = dep.scope.as_deref().unwrap_or("compile");
        if !matches!(scope, "compile" | "runtime" | "") || dep.optional {
            continue;
        }
        let (Some(g), Some(a), Some(v)) = (&dep.group, &dep.artifact, &dep.version) else {
            continue;
        };
        if v.contains("${") {
            continue;
        }
        let Ok(dep_name) = PackageName::new(format!("{g}:{a}")) else {
            continue;
        };
        let Ok(range) = VersionRange::parse(v) else {
            continue;
        };
        let spec = DependencySpec::new(dep_name, range);
        manifest.add_dep(spec, false, false, false);
    }
    Ok(manifest)
}

pub(crate) fn write_cargo_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    mgc_adapter_base::cargo_manifest::write_manifest(root, manifest)
}

/// Write pom.xml dependency pins from the manifest (native add). mgc owns
/// the compile/runtime `<dependency>` entries: each manifest dep becomes
/// (or updates) a block inside top-level `<dependencies>` (created when
/// absent). dependencyManagement/profiles/plugins are never touched.
/// A non-exact range fails closed — callers pin via resolve-first.
/// (Ghi dependency pom.xml từ manifest.)
pub(crate) fn write_pom_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    let path = root.join("pom.xml");
    let content = std::fs::read_to_string(&path)
        .map_err(|e| mgc_types::MgError::Other(format!("read pom.xml: {e}")))?;
    let mut pins: Vec<(String, String, String)> = Vec::new();
    for dep in manifest.all_dependencies() {
        let version = dep
            .range
            .satisfying_version()
            .map(|v| v.to_string())
            .unwrap_or_else(|| dep.range.to_string());
        let clean = version.trim_start_matches(['=', 'v', ' ']);
        mgc_types::Version::parse(clean).map_err(|_| {
            mgc_types::MgError::Other(format!(
                "pom writer needs an exact version for '{}', got '{}' — resolve-first must pin before save",
                dep.name.as_str(),
                dep.range
            ))
        })?;
        let mut parts = dep.name.as_str().splitn(2, ':');
        let (group, artifact) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        if group.is_empty() || artifact.is_empty() {
            return Err(mgc_types::MgError::Other(format!(
                "pom writer needs group:artifact coordinates, got '{}'",
                dep.name.as_str()
            )));
        }
        pins.push((group.to_string(), artifact.to_string(), clean.to_string()));
    }
    pins.sort();
    pins.dedup();
    let mut out = content;
    for (group, artifact, version) in &pins {
        out = upsert_pom_dependency(&out, group, artifact, version)?;
    }
    std::fs::write(&path, out)
        .map_err(|e| mgc_types::MgError::Other(format!("write {}: {e}", path.display())))?;
    Ok(())
}

/// Insert or update one dependency inside the TOP-LEVEL `<dependencies>`
/// block (a direct child of `<project>` — dependencyManagement, profiles
/// and plugin blocks are never touched). Creates the block when absent.
fn upsert_pom_dependency(
    content: &str,
    group: &str,
    artifact: &str,
    version: &str,
) -> MgResult<String> {
    let entry = render_pom_dependency(group, artifact, version);
    // Locate the top-level <dependencies> span by tag-depth tracking
    // (dependencyManagement nests its own <dependencies> deeper).
    let mut depth = 0usize;
    let mut top_start: Option<usize> = None;
    let mut top_end: Option<usize> = None;
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if content[i..].starts_with("</") {
                let end = content[i..].find('>').map(|p| i + p).unwrap_or(bytes.len());
                let tag = content[i + 2..end].trim().to_string();
                if tag == "dependencies" && depth == 2 && top_start.is_some() && top_end.is_none() {
                    top_end = Some(end + 1);
                }
                if !tag.starts_with('?') {
                    depth = depth.saturating_sub(1);
                }
                i = end + 1;
                continue;
            }
            if content[i..].starts_with("<!--") {
                let end = content[i..]
                    .find("-->")
                    .map(|p| i + p + 3)
                    .unwrap_or(bytes.len());
                i = end;
                continue;
            }
            let end = content[i..].find('>').map(|p| i + p).unwrap_or(bytes.len());
            let mut tag = content[i + 1..end].trim().to_string();
            let self_close = tag.ends_with('/');
            if self_close {
                tag = tag.trim_end_matches('/').trim().to_string();
            }
            let name = tag.split_whitespace().next().unwrap_or("").to_string();
            if name == "dependencies" && depth == 1 && top_start.is_none() {
                top_start = Some(i);
            }
            if !self_close && !name.starts_with('?') && !name.starts_with('!') {
                depth += 1;
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
    if let (Some(s), Some(e)) = (top_start, top_end) {
        let mut block = content[s..e].to_string();
        let mut search_from = 0;
        let mut replaced = false;
        while let Some(rel) = block[search_from..].find("<dependency>") {
            let ds = search_from + rel;
            let Some(de) = block[ds..]
                .find("</dependency>")
                .map(|p| ds + p + "</dependency>".len())
            else {
                break;
            };
            let chunk = &block[ds..de];
            if chunk.contains(&format!("<groupId>{group}</groupId>"))
                && chunk.contains(&format!("<artifactId>{artifact}</artifactId>"))
            {
                block.replace_range(ds..de, entry.trim_end());
                replaced = true;
                break;
            }
            search_from = de;
        }
        if !replaced {
            block = block.replacen("</dependencies>", &format!("{entry}</dependencies>"), 1);
        }
        let mut out = content.to_string();
        out.replace_range(s..e, &block);
        return Ok(out);
    }
    if content.contains("</project>") {
        return Ok(content.replacen(
            "</project>",
            &format!("  <dependencies>\n{entry}  </dependencies>\n</project>"),
            1,
        ));
    }
    Err(mgc_types::MgError::Other(
        "pom.xml has no </project> root — refusing to write".to_string(),
    ))
}

fn render_pom_dependency(group: &str, artifact: &str, version: &str) -> String {
    format!(
        "    <dependency>\n      <groupId>{group}</groupId>\n      <artifactId>{artifact}</artifactId>\n      <version>{version}</version>\n    </dependency>\n"
    )
}

/// Write csproj PackageReference pins from the manifest (native add).
/// mgc owns the references: each manifest dep becomes (or updates)
/// `<PackageReference Include="id" Version="x" />` inside an ItemGroup
/// (created when absent). Everything else is preserved verbatim.
/// A non-exact range fails closed — callers pin via resolve-first.
/// (Ghi PackageReference csproj từ manifest.)
pub(crate) fn write_csproj_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    let path = find_csproj(root).ok_or_else(|| {
        mgc_types::MgError::Other(
            "no .csproj found — scaffold one with `mgc create-lib dotnet`".to_string(),
        )
    })?;
    let content = std::fs::read_to_string(&path)
        .map_err(|e| mgc_types::MgError::Other(format!("read {}: {e}", path.display())))?;
    let mut pins: Vec<(String, String)> = Vec::new();
    for dep in manifest.all_dependencies() {
        let version = dep
            .range
            .satisfying_version()
            .map(|v| v.to_string())
            .unwrap_or_else(|| dep.range.to_string());
        let clean = version.trim_start_matches(['=', 'v', ' ']);
        mgc_types::Version::parse(clean).map_err(|_| {
            mgc_types::MgError::Other(format!(
                "csproj writer needs an exact version for '{}', got '{}' — resolve-first must pin before save",
                dep.name.as_str(),
                dep.range
            ))
        })?;
        pins.push((dep.name.as_str().to_string(), clean.to_string()));
    }
    pins.sort();
    pins.dedup();
    let mut out = content;
    for (name, version) in &pins {
        let escaped = name.replace('&', "&amp;").replace('"', "&quot;");
        // Update in place when the reference already exists (any version).
        let mut replaced = false;
        for line in out.lines() {
            let t = line.trim();
            if t.starts_with("<PackageReference") && t.contains(&format!("Include=\"{escaped}\"")) {
                let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                let replacement = format!(
                    "{indent}<PackageReference Include=\"{escaped}\" Version=\"{version}\" />"
                );
                out = out.replacen(line, &replacement, 1);
                replaced = true;
                break;
            }
        }
        if replaced {
            continue;
        }
        let entry =
            format!("    <PackageReference Include=\"{escaped}\" Version=\"{version}\" />\n");
        if out.contains("</ItemGroup>") {
            out = out.replacen("</ItemGroup>", &format!("{entry}</ItemGroup>"), 1);
        } else if out.contains("</Project>") {
            out = out.replacen(
                "</Project>",
                &format!("  <ItemGroup>\n{entry}  </ItemGroup>\n</Project>"),
                1,
            );
        } else {
            return Err(mgc_types::MgError::Other(
                "csproj has no </Project> root — refusing to write".to_string(),
            ));
        }
    }
    std::fs::write(&path, out)
        .map_err(|e| mgc_types::MgError::Other(format!("write {}: {e}", path.display())))?;
    Ok(())
}

/// Write go.mod require pins from the manifest for native add.
/// mgc owns the require set. All other directives are preserved verbatim.
/// Versions are always v-prefixed. Non-exact ranges fail closed here;
/// callers must pin through resolve-first before saving.
pub(crate) fn write_go_mod_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    let path = root.join("go.mod");
    let content = std::fs::read_to_string(&path)
        .map_err(|e| mgc_types::MgError::Other(format!("read go.mod: {e}")))?;
    let mut kept: Vec<String> = Vec::new();
    let mut in_require_block = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if in_require_block {
            if trimmed == ")" {
                in_require_block = false;
            }
            continue;
        }
        if trimmed == "require (" {
            in_require_block = true;
            continue;
        }
        if trimmed.starts_with("require ") {
            continue;
        }
        kept.push(line.to_string());
    }
    let mut pins: Vec<(String, String)> = Vec::new();
    for dep in manifest.all_dependencies() {
        let version = dep
            .range
            .satisfying_version()
            .map(|v| v.to_string())
            .unwrap_or_else(|| dep.range.to_string());
        let clean = version.trim_start_matches(['=', 'v', ' ']);
        let parsed = mgc_types::Version::parse(clean).map_err(|_| {
            mgc_types::MgError::Other(format!(
                "go.mod writer needs an exact version for '{}', got '{}' — resolve-first must pin before save",
                dep.name.as_str(),
                dep.range
            ))
        })?;
        pins.push((dep.name.as_str().to_string(), format!("v{parsed}")));
    }
    pins.sort();
    pins.dedup();
    while kept.last().is_some_and(|l| l.trim().is_empty()) {
        kept.pop();
    }
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str("\nrequire (\n");
    for (name, version) in &pins {
        out.push_str(&format!("\t{name} {version}\n"));
    }
    out.push_str(")\n");
    std::fs::write(&path, out)
        .map_err(|e| mgc_types::MgError::Other(format!("write go.mod: {e}")))?;
    Ok(())
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

/// Parse the project's .csproj `<PackageReference>` entries into a Manifest
/// (read-only — mgc never rewrites csproj). Entries with a concrete Version
/// enter the manifest; missing/`*`/floating versions are honestly skipped —
/// they cannot be resolved deterministically (caller-side markers are not
/// available on Manifest, so the skip is documented here).
/// Đọc `<PackageReference>` của .csproj thành Manifest (chỉ đọc — mgc
/// không bao giờ viết lại csproj). Entry có Version cụ thể vào manifest;
/// version thiếu/`*`/float được skip trung thực — không resolve tất định
/// được (Manifest không có chỗ ghi marker nên skip được ghi chú ở đây).
pub(crate) fn parse_csproj_manifest(root: &Path) -> MgResult<Manifest> {
    use crate::language::find_csproj;
    let Some(csproj) = find_csproj(root) else {
        return Ok(Manifest::new("dotnet-lib", Ecosystem::Lib));
    };
    let content = std::fs::read_to_string(&csproj)
        .map_err(|e| mgc_types::MgError::Other(format!("read {}: {e}", csproj.display())))?;
    let name = csproj
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("dotnet-lib")
        .to_string();
    let (refs, _unresolved) = mgc_resolver::protocols::nuget::parse_package_references(&content);
    let mut manifest = Manifest::new(&name, Ecosystem::Lib);
    for r in refs {
        let Some(version) = &r.version else {
            continue;
        };
        let Ok(dep_name) = PackageName::new(r.id.clone()) else {
            continue;
        };
        let Ok(range) = VersionRange::parse(version) else {
            continue;
        };
        let spec = DependencySpec::new(dep_name, range);
        manifest.add_dep(spec, false, false, false);
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
        .map(|dep| {
            // Star ranges are REAL pins-any-version deps (hand-written
            // `dependencies = ["six"]` or `*`): serialize as the bare
            // PEP 508 name — dropping them would silently delete a dep
            // the user declared.
            // (Range `*` là dep thật: ghi tên trần, không được rào mất.)
            if dep.range.is_star() {
                return toml::Value::String(dep.name.as_str().to_string());
            }
            let raw = dep.range.as_str().trim();
            // Faithful operator mapping (mgc spelling → PEP 508): exact
            // pins (`==`, mgc's own `=`, bare versions) serialize as
            // `==` — never widened to `>=`. Only npm-isms (`^`, `~`,
            // which PEP 508 lacks) coerce to a lower bound.
            // (Ánh xạ operator trung thực: ghim exact giữ `==`.)
            let bound = if let Some(v) = raw.strip_prefix("==").or_else(|| raw.strip_prefix('=')) {
                format!("=={v}")
            } else if let Some(v) = raw.strip_prefix("~=") {
                format!("~={v}")
            } else if raw.starts_with(">=") || raw.starts_with("<=") || raw.starts_with("!=") {
                raw.to_string()
            } else if let Some(v) = raw.strip_prefix('>') {
                format!(">{v}")
            } else if let Some(v) = raw.strip_prefix('<') {
                format!("<{v}")
            } else if let Some(v) = raw.strip_prefix('^').or_else(|| raw.strip_prefix('~')) {
                format!(">={v}")
            } else {
                // Bare version = exact pin (mgc convention; PEP 508 has
                // no bare form).
                format!("=={raw}")
            };
            toml::Value::String(format!("{}{}", dep.name.as_str(), bound))
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
