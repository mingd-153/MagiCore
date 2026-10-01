//! Manifest parsing and writing for library projects.
//! Tách logic Cargo/Python manifest để adapter chính dễ maintain và mở rộng.

use crate::language::find_csproj;
use mgc_types::{DependencySpec, Ecosystem, Manifest, MgResult, PackageName, VersionRange};
use std::path::Path;

/// Whether the MGC native Python engine can own this project's dependency
/// declarations without silently ignoring another tool's lock or source.
/// PEP 621 optional dependency groups are metadata unless explicitly selected;
/// MGC currently installs only the default `dependencies` set.
/// (Chỉ nhận lane Python native khi MGC sở hữu đủ khai báo và không bỏ qua
/// lockfile/nguồn dependency do tool khác quản lý.)
pub fn supports_native_python_project(root: &Path) -> bool {
    let manifest_path = root.join("pyproject.toml");
    let Ok(content) =
        mgc_adapter_base::project_file::read_regular_text(&manifest_path, "pyproject.toml")
    else {
        return false;
    };
    let Ok(document) = toml::from_str::<toml::Value>(&content) else {
        return false;
    };
    let Some(project) = document.get("project").and_then(toml::Value::as_table) else {
        return false;
    };
    if project
        .get("dynamic")
        .and_then(toml::Value::as_array)
        .is_some_and(|dynamic| dynamic.iter().any(|v| v.as_str() == Some("dependencies")))
        || project
            .get("dependencies")
            .is_some_and(|dependencies| dependencies.as_array().is_none())
        || project.get("optional-dependencies").is_some_and(|groups| {
            !groups.as_table().is_some_and(|table| {
                table.values().all(|items| {
                    items
                        .as_array()
                        .is_some_and(|items| items.iter().all(toml::Value::is_str))
                })
            })
        })
    {
        return false;
    }
    if let Some(dependencies) = project.get("dependencies").and_then(toml::Value::as_array) {
        // The current resolver models only a simple name + version range.
        // Admit no PEP 508 extras, environment markers, URL references, or
        // compound syntax that would be flattened during MGC add/remove.
        // (Resolver hiện chỉ mô hình tên + version range đơn giản; từ chối
        // extras/marker/URL/cú pháp ghép để không làm mất semantics.)
        if dependencies.iter().any(|dependency| {
            dependency
                .as_str()
                .is_none_or(|spec| parse_python_dependency(spec).is_err())
        }) {
            return false;
        }
    }

    // Do not select only PEP 621 dependencies when a second manager or
    // dependency-group mechanism may contribute packages to the environment.
    let tool = document.get("tool");
    let uv = tool.and_then(|v| v.get("uv"));
    let has_foreign_dependency_source = [
        tool.and_then(|v| v.get("poetry")),
        tool.and_then(|v| v.get("pdm")),
        tool.and_then(|v| v.get("pixi")),
        uv.and_then(|v| v.get("sources")),
        uv.and_then(|v| v.get("dev-dependencies")),
        uv.and_then(|v| v.get("dependency-groups")),
        document.get("dependency-groups"),
    ]
    .into_iter()
    .flatten()
    .any(|value| !value.as_table().is_some_and(toml::map::Map::is_empty));
    if has_foreign_dependency_source {
        return false;
    }

    let Ok(mut entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries.all(|entry| {
        let Ok(entry) = entry else {
            return false;
        };
        let Some(name) = entry.file_name().into_string().ok() else {
            return false;
        };
        let name = name.to_ascii_lowercase();
        !matches!(
            name.as_str(),
            "uv.lock"
                | "poetry.lock"
                | "pdm.lock"
                | "pipfile"
                | "pipfile.lock"
                | "pixi.lock"
                | "conda-lock.yml"
                | "conda-lock.yaml"
                | "requirements.lock"
                | "requirements.txt"
                | "pylock.toml"
        ) && !(name.starts_with("requirements") && name.ends_with(".txt"))
            && !(name.starts_with("pylock.") && name.ends_with(".toml"))
    })
}

pub(crate) fn parse_cargo_manifest(root: &Path) -> MgResult<Manifest> {
    mgc_adapter_base::cargo_manifest::parse_manifest(root, Ecosystem::Lib)
}

/// Parse a project pom.xml into a Manifest (read-only — mgc never rewrites
/// pom.xml). Only the SAME compile/runtime deps the native engine uses enter
/// the manifest, keyed as `groupId:artifactId`; local properties and literal
/// dependencyManagement versions are resolved, while unresolved versions
/// fail closed rather than silently disappearing. Gradle projects get an honest empty manifest — build
/// scripts are not parseable without the toolchain and resolve fails closed
/// downstream with guidance.
/// Đọc pom.xml của project thành Manifest (chỉ đọc — mgc không bao giờ viết
/// lại pom.xml). Chỉ những dep compile/runtime mà engine native dùng vào
/// manifest, khóa theo `groupId:artifactId`; property cục bộ và version
/// literal trong dependencyManagement được giải; version chưa giải được
/// sẽ fail-closed thay vì bị âm thầm bỏ qua.
/// Project gradle nhận manifest rỗng trung thực — build script không parse
/// được nếu không có toolchain và resolve sẽ fail-closed kèm hướng dẫn.
pub(crate) fn parse_maven_manifest(root: &Path) -> MgResult<Manifest> {
    let pom_path = root.join("pom.xml");
    match std::fs::symlink_metadata(&pom_path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(mgc_types::MgError::Other(
                "refusing symlinked Maven manifest pom.xml".into(),
            ));
        }
        Ok(_) => {
            return Err(mgc_types::MgError::Other(
                "Maven manifest pom.xml is not a regular file".into(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Manifest::new("java-gradle-lib", Ecosystem::Lib));
        }
        Err(error) => {
            return Err(mgc_types::MgError::Other(format!(
                "inspect pom.xml: {error}"
            )));
        }
    }
    let content = mgc_adapter_base::project_file::read_regular_text(&pom_path, "pom.xml")?;
    let (group, artifact) = mgc_resolver::protocols::maven::pom_project_coordinates(&content)?
        .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));
    let mut manifest = Manifest::new(&format!("{group}:{artifact}"), Ecosystem::Lib);
    let (deps, _) = mgc_resolver::protocols::maven::collect_pom_dependencies(&content)?;
    let properties = mgc_resolver::protocols::maven::collect_pom_properties(&content)?;
    let managed = mgc_resolver::protocols::maven::collect_managed_versions(&content)?;
    for dep in deps {
        let scope = dep.scope.as_deref().unwrap_or("compile");
        if !matches!(scope, "compile" | "runtime" | "") || dep.optional {
            continue;
        }
        let (Some(g), Some(a)) = (&dep.group, &dep.artifact) else {
            return Err(mgc_types::MgError::Other(
                "pom.xml contains a compile/runtime dependency without groupId or artifactId; refusing to mutate an incomplete manifest".into(),
            ));
        };
        let dep_name = PackageName::new(format!("{g}:{a}")).map_err(|e| {
            mgc_types::MgError::Other(format!(
                "pom.xml contains invalid dependency coordinates '{g}:{a}': {e}"
            ))
        })?;
        let raw_version = dep
            .version
            .as_deref()
            .or_else(|| managed.get(&(g.clone(), a.clone())).map(String::as_str))
            .ok_or_else(|| {
                mgc_types::MgError::Other(format!(
                    "pom.xml dependency '{}' cannot be resolved from this pom.xml; parent/BOM inheritance is not resolved here, refusing to mutate an incomplete manifest",
                    dep_name.as_str()
                ))
            })?;
        let version = mgc_resolver::protocols::maven::substitute_properties(raw_version, &properties)
            .ok_or_else(|| {
                mgc_types::MgError::Other(format!(
                    "pom.xml dependency '{}' uses an unresolved version property '{}'; refusing to mutate an incomplete manifest",
                    dep_name.as_str(), raw_version
                ))
            })?;
        let range = VersionRange::parse(&version).map_err(|e| {
            mgc_types::MgError::Other(format!(
                "pom.xml dependency '{}' has unsupported version '{}': {e}",
                dep_name.as_str(),
                version
            ))
        })?;
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
    let content = mgc_adapter_base::project_file::read_regular_text(&path, "pom.xml")?;
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
    // Prune stale top-level pins (remove honesty): dependency blocks
    // whose G:A is NOT in the manifest are deleted. A remove that leaves
    // the pin behind is a lie.
    // (Xóa dependency không còn trong manifest.)
    out = prune_pom_dependencies(&out, &pins);
    mgc_adapter_base::project_file::atomic_write_regular(&path, out.as_bytes())?;
    Ok(())
}

/// Drop top-level `<dependency>` blocks whose group:artifact is absent
/// from `pins`. Other blocks (dependencyManagement, plugins, profiles)
/// are never touched.
fn prune_pom_dependencies(content: &str, pins: &[(String, String, String)]) -> String {
    // Collect top-level <dependency> spans with the same depth walk as
    // upsert (duplicated walk for clarity over cleverness).
    let mut spans: Vec<(usize, usize, String, String, bool)> = Vec::new();
    let bytes = content.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if content[i..].starts_with("</") {
                let end = content[i..].find('>').map(|p| i + p).unwrap_or(bytes.len());
                depth = depth.saturating_sub(1);
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
            let tag = content[i + 1..end].trim().to_string();
            let name = tag.split_whitespace().next().unwrap_or("").to_string();
            if name == "dependency"
                && depth == 2
                && let Some(fe) = content[end..]
                    .find("</dependency>")
                    .map(|p| end + p + "</dependency>".len())
            {
                let chunk = &content[i..fe];
                let g = chunk
                    .split("<groupId>")
                    .nth(1)
                    .and_then(|s| s.split("</groupId>").next())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let a = chunk
                    .split("<artifactId>")
                    .nth(1)
                    .and_then(|s| s.split("</artifactId>").next())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                spans.push((i, fe, g, a, pom_dependency_is_mgc_managed(chunk)));
                i = fe;
                continue;
            }
            if !tag.ends_with('/') && !name.starts_with('?') && !name.starts_with('!') {
                depth += 1;
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
    let mut out = content.to_string();
    for (s, e, g, a, managed) in spans.iter().rev() {
        if *managed && !pins.iter().any(|(pg, pa, _)| pg == g && pa == a) {
            out.replace_range(*s..*e, "");
        }
    }
    // Collapse triple blank lines left by removals (cosmetic only).
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out
}

/// Only the default compile/runtime, non-optional dependency set is MGC-owned.
/// MGC chỉ sở hữu dep compile/runtime mặc định, không optional.
fn pom_dependency_is_mgc_managed(dependency: &str) -> bool {
    let scope = dependency
        .split("<scope>")
        .nth(1)
        .and_then(|value| value.split("</scope>").next())
        .map(str::trim);
    let optional = dependency
        .split("<optional>")
        .nth(1)
        .and_then(|value| value.split("</optional>").next())
        .map(str::trim);
    scope.is_none_or(|scope| matches!(scope, "compile" | "runtime")) && optional != Some("true")
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
                if !pom_dependency_is_mgc_managed(chunk) {
                    return Err(mgc_types::MgError::Unsupported {
                        core: "lib",
                        capability: "mutate non-runtime or optional Maven dependency",
                        guidance: format!(
                            "MagiCore will not rewrite '{group}:{artifact}' because its Maven scope/optional semantics are outside the native dependency model"
                        ),
                    });
                }
                let updated = update_pom_dependency_version(chunk, version)?;
                block.replace_range(ds..de, &updated);
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

/// Update only a dependency's version element, preserving exclusions, scope,
/// optionality, classifier, and other Maven metadata.
/// Chỉ đổi version, giữ exclusions/scope/optional/classifier và metadata khác.
fn update_pom_dependency_version(dependency: &str, version: &str) -> MgResult<String> {
    if let Some(start) = dependency.find("<version>") {
        let value_start = start + "<version>".len();
        let value_end = dependency[value_start..]
            .find("</version>")
            .map(|offset| value_start + offset)
            .ok_or_else(|| {
                mgc_types::MgError::Other(
                    "malformed Maven dependency has an unclosed version element".into(),
                )
            })?;
        let mut updated = dependency.to_string();
        updated.replace_range(value_start..value_end, version);
        return Ok(updated);
    }
    let close = dependency.rfind("</dependency>").ok_or_else(|| {
        mgc_types::MgError::Other("malformed Maven dependency has no closing element".into())
    })?;
    let mut updated = dependency.to_string();
    updated.insert_str(close, &format!("<version>{version}</version>"));
    Ok(updated)
}

fn render_pom_dependency(group: &str, artifact: &str, version: &str) -> String {
    format!(
        "    <dependency>\n      <groupId>{group}</groupId>\n      <artifactId>{artifact}</artifactId>\n      <version>{version}</version>\n    </dependency>\n"
    )
}

/// Drop `<PackageReference>` elements (single- or multi-line) whose
/// `Include` id is absent from `keep`. Anything else is preserved byte
/// for byte, including comments and whitespace.
fn prune_csproj_references(content: &str, keep: &std::collections::HashSet<&str>) -> String {
    fn kept(element: &str, keep: &std::collections::HashSet<&str>) -> bool {
        keep.iter().any(|id| {
            element.contains(&format!("Include=\"{id}\""))
                || element.contains(&format!("Include='{id}'"))
        })
    }
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find("<PackageReference") {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let tag_end = tail.find('>').map(|p| p + 1);
        let Some(tag_end) = tag_end else {
            out.push_str(tail);
            rest = "";
            break;
        };
        let opening = &tail[..tag_end];
        let element_end = if opening.ends_with("/>") {
            tag_end
        } else if let Some(fe) = tail
            .find("</PackageReference>")
            .map(|p| p + "</PackageReference>".len())
        {
            fe
        } else {
            out.push_str(tail);
            rest = "";
            break;
        };
        let element = &tail[..element_end];
        if kept(element, keep) {
            out.push_str(element);
            rest = &tail[element_end..];
        } else {
            // Dropped: swallow one following newline so no blank line
            // litters the ItemGroup.
            rest = tail[element_end..]
                .strip_prefix('\n')
                .unwrap_or(&tail[element_end..]);
        }
    }
    out.push_str(rest);
    out
}

/// Write csproj PackageReference pins from the manifest (native add).
/// mgc owns the references: each manifest dep becomes (or updates)
/// `<PackageReference Include="id" Version="x" />` inside an ItemGroup
/// (created when absent). Everything else is preserved verbatim.
/// A non-exact range fails closed — callers pin via resolve-first.
/// (Ghi PackageReference csproj từ manifest.)
pub(crate) fn write_csproj_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    // Validate the pre-existing graph before pruning; unresolved references
    // cannot be represented in Manifest and must never be silently deleted.
    // Kiểm tra graph cũ trước khi prune; reference không biểu diễn được không được âm thầm xóa.
    let _current_manifest = parse_csproj_manifest(root)?;
    let path = find_csproj(root).ok_or_else(|| {
        mgc_types::MgError::Other(
            "no .csproj found — scaffold one with `mgc create-lib dotnet`".to_string(),
        )
    })?;
    let content = mgc_adapter_base::project_file::read_regular_text(&path, ".csproj")?;
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
    // Prune stale references first (remove honesty): any PackageReference
    // whose id is NOT in the manifest is deleted — single-line and
    // multi-line forms. A remove that leaves the file behind is a lie.
    // (Xóa reference không còn trong manifest.)
    let keep: std::collections::HashSet<&str> = pins.iter().map(|(n, _)| n.as_str()).collect();
    let mut out = prune_csproj_references(&content, &keep);
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
    mgc_adapter_base::project_file::atomic_write_regular(&path, out.as_bytes())?;
    Ok(())
}

/// Refuse links/non-regular manifest paths, then publish through a unique
/// same-directory temporary and atomic replace. This prevents a manifest
/// symlink from redirecting a package operation outside the project.
/// Từ chối symlink/file không thường; publish atomic qua temp cùng thư mục.
/// Write go.mod require pins from the manifest for native add.
/// mgc owns the require set. All other directives are preserved verbatim.
/// Versions are always v-prefixed. Non-exact ranges fail closed here;
/// callers must pin through resolve-first before saving.
pub(crate) fn write_go_mod_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    let path = root.join("go.mod");
    let content = mgc_adapter_base::project_file::read_regular_text(&path, "go.mod")?;
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
    mgc_adapter_base::project_file::atomic_write_regular(&path, out.as_bytes())?;
    Ok(())
}

/// Parse go.mod into a Manifest for MGC-owned resolve/add/remove/update.
/// MGC owns the require set it writes; unrelated module directives are
/// preserved. Dependencies keep
/// their FULL module path (PackageName's repo-path form) so the native
/// GoModProtocol can resolve them; the project's own `replace` directives
/// rewrite pins and `exclude` drops exact (module, version) pairs — the
/// effective module graph, honestly.
/// Đọc go.mod thành Manifest cho resolve/add/remove/update do MGC sở hữu.
/// MGC sở hữu require set được ghi; directive module không liên quan được giữ.
/// Dependency giữ
/// NGUYÊN path module (dạng repo-path của PackageName) để GoModProtocol
/// native resolve được; directive `replace` của project viết lại pin và
/// `exclude` loại cặp (module, version) đúng đó — graph module hiệu lực,
/// trung thực.
pub(crate) fn parse_go_mod_manifest(root: &Path) -> MgResult<Manifest> {
    let content =
        mgc_adapter_base::project_file::read_regular_text(&root.join("go.mod"), "go.mod")?;
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

/// Parse the project's .csproj `<PackageReference>` entries into a Manifest.
/// Unresolved versions and conditional/update/remove semantics fail closed;
/// they cannot be represented by the flat dependency model.
/// Đọc `<PackageReference>` của .csproj thành Manifest (chỉ đọc — mgc
/// từ chối version chưa giải và semantics condition/update/remove mà graph
/// phẳng chưa biểu diễn được.
pub(crate) fn parse_csproj_manifest(root: &Path) -> MgResult<Manifest> {
    use crate::language::find_csproj;
    let Some(csproj) = find_csproj(root) else {
        return Err(mgc_types::MgError::Other(
            "no unique regular root-level .csproj was found; refusing to return an empty .NET dependency manifest".into(),
        ));
    };
    let content = mgc_adapter_base::project_file::read_regular_text(&csproj, ".csproj")?;
    if has_unmodeled_package_reference_semantics(&content) {
        return Err(mgc_types::MgError::Unsupported {
            core: "lib",
            capability: ".NET conditional/Update/Remove PackageReference",
            guidance: "MagiCore does not yet preserve conditional, Update, or Remove PackageReference semantics; these package graphs are refused before mutation".to_string(),
        });
    }
    let name = csproj
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("dotnet-lib")
        .to_string();
    let (refs, unresolved) = mgc_resolver::protocols::nuget::parse_package_references(&content);
    if !unresolved.is_empty() {
        return Err(mgc_types::MgError::Unsupported {
            core: "lib",
            capability: ".NET PackageReference with unresolved version",
            guidance: format!(
                "MagiCore cannot safely mutate this project because these PackageReference entries have floating or missing versions: {}",
                unresolved.join(", ")
            ),
        });
    }
    let mut manifest = Manifest::new(&name, Ecosystem::Lib);
    for r in refs {
        let Some(version) = &r.version else {
            continue;
        };
        let dep_name = PackageName::new(r.id.clone()).map_err(|error| {
            mgc_types::MgError::Other(format!(
                "invalid NuGet PackageReference id '{}': {error}",
                r.id
            ))
        })?;
        let range = VersionRange::parse(version).map_err(|error| {
            mgc_types::MgError::Unsupported {
                core: "lib",
                capability: ".NET PackageReference version range",
                guidance: format!(
                    "MagiCore cannot safely mutate PackageReference '{}' with version '{version}': {error}",
                    r.id
                ),
            }
        })?;
        let spec = DependencySpec::new(dep_name, range);
        manifest.add_dep(spec, false, false, false);
    }
    Ok(manifest)
}

/// Detect conditions on package groups/references that the flat Manifest cannot represent.
/// Phát hiện điều kiện theo target mà Manifest phẳng chưa thể biểu diễn.
fn has_unmodeled_package_reference_semantics(content: &str) -> bool {
    let mut rest = content;
    while let Some(start) = rest.find('<') {
        rest = &rest[start..];
        if rest.starts_with("<!--") {
            let Some(end) = rest.find("-->") else {
                return true;
            };
            rest = &rest[end + 3..];
            continue;
        }
        let Some(end) = rest.find('>') else {
            return true;
        };
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        let tag = tag.trim_start();
        let Some(name_end) = tag.find(char::is_whitespace).or_else(|| tag.find('/')) else {
            continue;
        };
        let qualified_name = &tag[..name_end];
        let element_name = qualified_name.rsplit(':').next().unwrap_or_default();
        if !matches!(element_name, "ItemGroup" | "PackageReference") {
            continue;
        }
        if qualified_name.contains(':') {
            return true;
        }
        let package_reference = element_name == "PackageReference";
        let mut attrs = &tag[name_end..];
        while !attrs.is_empty() {
            attrs = attrs.trim_start_matches(char::is_whitespace);
            if attrs.is_empty() || attrs.starts_with('/') {
                break;
            }
            let key_end = attrs
                .find(|ch: char| ch.is_whitespace() || ch == '=' || ch == '/')
                .unwrap_or(attrs.len());
            let key = &attrs[..key_end];
            attrs = &attrs[key_end..];
            attrs = attrs.trim_start_matches(char::is_whitespace);
            if !attrs.starts_with('=') {
                continue;
            }
            attrs = attrs[1..].trim_start_matches(char::is_whitespace);
            if key == "Condition" || (package_reference && matches!(key, "Update" | "Remove")) {
                return true;
            }
            if let Some(quote) = attrs.chars().next().filter(|ch| *ch == '\'' || *ch == '"') {
                attrs = &attrs[1..];
                let Some(value_end) = attrs.find(quote) else {
                    return true;
                };
                attrs = &attrs[value_end + 1..];
            } else {
                let value_end = attrs
                    .find(|ch: char| ch.is_whitespace() || ch == '/')
                    .unwrap_or(attrs.len());
                attrs = &attrs[value_end..];
            }
        }
    }
    false
}

pub(crate) fn parse_pyproject_manifest(root: &Path) -> MgResult<Manifest> {
    let content = mgc_adapter_base::project_file::read_regular_text(
        &root.join("pyproject.toml"),
        "pyproject.toml",
    )?;
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
    if trimmed.is_empty()
        || trimmed.chars().any(char::is_whitespace)
        || trimmed
            .chars()
            .any(|ch| matches!(ch, '[' | ']' | ';' | '@' | ','))
    {
        return Err(mgc_types::MgError::Other(
            "PEP 508 extras, markers, URL references, and compound constraints are unsupported by the native resolver".to_string(),
        ));
    }
    let split_at = ["==", ">=", "<=", "~=", ">", "<"]
        .iter()
        .filter_map(|op| trimmed.find(op).map(|idx| (idx, *op)))
        .min_by_key(|(idx, _)| *idx);

    let Some((idx, op)) = split_at else {
        return DependencySpec::parse(trimmed);
    };

    let name = PackageName::new(trimmed[..idx].trim())?;
    let raw_range = trimmed[idx + op.len()..].trim();
    if raw_range.is_empty()
        || raw_range
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || ".-*+^~=<>!|".contains(ch)))
    {
        return Err(mgc_types::MgError::Other(format!(
            "unsupported PEP 508 version requirement in '{trimmed}'"
        )));
    }
    let range = match op {
        "==" => VersionRange::parse(raw_range)?,
        "~=" => VersionRange::parse(&format!("~{raw_range}"))?,
        _ => VersionRange::parse(&format!("{op}{raw_range}"))?,
    };
    Ok(DependencySpec::new(name, range))
}

pub(crate) fn write_pyproject_manifest(root: &Path, manifest: &Manifest) -> MgResult<()> {
    let path = root.join("pyproject.toml");
    let content = mgc_adapter_base::project_file::read_regular_text(&path, "pyproject.toml")?;
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

    let rendered =
        toml::to_string_pretty(&v).map_err(|e| mgc_types::MgError::Other(e.to_string()))?;
    mgc_adapter_base::project_file::atomic_write_regular(&path, rendered.as_bytes())?;
    Ok(())
}

#[cfg(test)]
#[path = "test/manifest_test.rs"]
mod tests;
