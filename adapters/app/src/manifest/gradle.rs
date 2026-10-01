//! Gradle build.gradle / build.gradle.kts manifest parsing.

use mgc_adapter_base::project_file::{atomic_write_project_regular, read_project_regular_text};
use mgc_types::{Manifest, MgError, MgResult};
use std::path::Path;

/// Parse build.gradle or build.gradle.kts to Manifest.
pub fn parse_build_gradle(project_root: &Path) -> MgResult<Manifest> {
    let gradle_path = if project_root.join("build.gradle.kts").exists() {
        project_root.join("build.gradle.kts")
    } else {
        project_root.join("build.gradle")
    };

    if !gradle_path.exists() {
        return Err(MgError::Other("build.gradle not found".to_string()));
    }

    Err(MgError::Unsupported {
        core: "app",
        capability: "parse-gradle-manifest",
        guidance: format!(
            "{} is executable Gradle DSL; MagiCore does not parse it as a dependency manifest. Use only the version-catalog operations MagiCore explicitly supports; other dependency operations are unsupported",
            gradle_path.display()
        ),
    })
}

/// Gradle build files are programs; generic manifest rewrites are unsafe.
pub fn write_build_gradle(_project_root: &Path, _manifest: &Manifest) -> MgResult<()> {
    Err(MgError::Unsupported {
        core: "app",
        capability: "write-gradle-manifest",
        guidance: "MagiCore does not rewrite executable Gradle DSL; only explicitly supported version-catalog operations are available, and other dependency mutations are unsupported".to_string(),
    })
}

/// A version-catalog library entry.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    /// Alias in `[libraries]` (e.g. `commons-lang3`).
    pub alias: String,
    /// Maven group (e.g. `org.apache.commons`).
    pub group: String,
    /// Maven artifact (e.g. `commons-lang3`).
    pub artifact: String,
    /// Pinned version string (resolved through `version.ref` when present).
    pub version: String,
    /// The `[versions]` ref name when versioned by reference (`None` for
    /// inline literals).
    pub version_ref: Option<String>,
}

/// Parse `gradle/libs.versions.toml` into library entries. Returns `None`
/// when no catalog exists (NOT an error — the project may not use one).
/// Non-versioned entries (plugins, bundles, version-less refs) are
/// skipped honestly; only resolvable `module + version` pins surface.
/// (Đọc version catalog TOML.)
pub fn parse_version_catalog(project_root: &Path) -> Option<Vec<CatalogEntry>> {
    let content = read_project_regular_text(
        project_root,
        Path::new("gradle/libs.versions.toml"),
        "libs.versions.toml",
    )
    .ok()?;
    parse_catalog_content(&content)
}

/// Bump one catalog pin to a new version: version-ref entries rewrite
/// the `[versions]` value (shared refs bump together — reported), inline
/// literals rewrite in place. Verifies by re-parse (the pin must read
/// back the new version) and returns whether anything changed.
/// (Nâng version catalog + verify bằng đọc lại.)
pub fn bump_catalog_pin(
    project_root: &Path,
    group: &str,
    artifact: &str,
    new_version: &str,
) -> MgResult<bool> {
    let content = read_project_regular_text(
        project_root,
        Path::new("gradle/libs.versions.toml"),
        "libs.versions.toml",
    )?;
    let entries = parse_catalog_content(&content)
        .ok_or_else(|| MgError::Other("unsupported libs.versions.toml shape".to_string()))?;
    let hit = entries
        .iter()
        .find(|e| e.group == group && e.artifact == artifact);
    let Some(hit) = hit else {
        return Ok(false);
    };
    if hit.version == new_version {
        return Ok(false);
    }
    let mut editable: toml_edit::DocumentMut = content
        .parse()
        .map_err(|e| MgError::Other(format!("parse editable libs.versions.toml: {e}")))?;
    if let Some(ref_name) = &hit.version_ref {
        let source: toml::Value = toml::from_str(&content)
            .map_err(|e| MgError::Other(format!("parse libs.versions.toml: {e}")))?;
        if count_version_ref_uses(&source, ref_name) > 1 {
            return Err(MgError::Unsupported {
                core: "app",
                capability: "bump-shared-gradle-version-ref",
                guidance: format!(
                    "version ref '{ref_name}' is shared by multiple catalog entries; refusing to change unrelated dependencies through a single-package update"
                ),
            });
        }
        // Shared ref: rewrite the single [versions] value.
        editable["versions"][ref_name] = toml_edit::value(new_version);
    } else {
        // Inline literal: rewrite `group:name:old` → `group:name:new`
        // inside [libraries] only (TOML-structure-aware, not blind text).
        let library = &mut editable["libraries"][&hit.alias];
        if let Some(value) = library.as_value_mut().filter(|value| value.is_str()) {
            let mut replacement =
                toml_edit::Value::from(format!("{group}:{artifact}:{new_version}"));
            *replacement.decor_mut() = value.decor().clone();
            *value = replacement;
        } else if library.is_inline_table() {
            library["version"] = toml_edit::value(new_version);
        } else {
            return Err(MgError::Other(format!(
                "catalog pin {group}:{artifact} has unsupported editable shape"
            )));
        }
    }
    let next = editable.to_string();
    atomic_write_project_regular(
        project_root,
        Path::new("gradle/libs.versions.toml"),
        next.as_bytes(),
    )?;
    // Verify by re-parse: the pin must read back the new version.
    let verified = read_project_regular_text(
        project_root,
        Path::new("gradle/libs.versions.toml"),
        "libs.versions.toml",
    )?;
    let again = parse_catalog_content(&verified).unwrap_or_default();
    let ok = again
        .iter()
        .any(|e| e.group == group && e.artifact == artifact && e.version == new_version);
    if !ok {
        return Err(MgError::Other(format!(
            "catalog rewrite did not stick for {group}:{artifact} (fail-closed)"
        )));
    }
    Ok(true)
}

fn parse_catalog_content(content: &str) -> Option<Vec<CatalogEntry>> {
    let doc: toml::Value = toml::from_str(content).ok()?;
    let versions = doc.get("versions")?.as_table()?;
    let libraries = doc.get("libraries")?.as_table()?;
    let mut out = Vec::new();
    for (alias, spec) in libraries {
        let (module, version, version_ref): (String, String, Option<String>) = match spec {
            toml::Value::String(s) => {
                let mut parts = s.rsplitn(2, ':');
                let (Some(version), Some(module)) = (parts.next(), parts.next()) else {
                    continue;
                };
                if version.is_empty() || module.is_empty() {
                    continue;
                }
                (module.to_string(), version.to_string(), None)
            }
            toml::Value::Table(t) => {
                let Some(module) = t.get("module").and_then(|m| m.as_str()) else {
                    continue;
                };
                match t.get("version") {
                    Some(toml::Value::String(v)) => (module.to_string(), v.clone(), None),
                    Some(other) => {
                        let Some(r) = other.get("ref").and_then(|x| x.as_str()) else {
                            continue;
                        };
                        let Some(v) = versions.get(r).and_then(|x| x.as_str()) else {
                            continue;
                        };
                        (module.to_string(), v.to_string(), Some(r.to_string()))
                    }
                    None => continue,
                }
            }
            _ => continue,
        };
        let mut gp = module.splitn(2, ':');
        let (Some(group), Some(artifact)) = (gp.next(), gp.next()) else {
            continue;
        };
        if group.is_empty() || artifact.is_empty() {
            continue;
        }
        out.push(CatalogEntry {
            alias: alias.clone(),
            group: group.to_string(),
            artifact: artifact.to_string(),
            version,
            version_ref,
        });
    }
    Some(out)
}

fn count_version_ref_uses(value: &toml::Value, ref_name: &str) -> usize {
    match value {
        toml::Value::Table(table) => {
            let current = usize::from(
                table
                    .get("version")
                    .and_then(|version| version.get("ref"))
                    .and_then(toml::Value::as_str)
                    == Some(ref_name),
            );
            current
                + table
                    .values()
                    .map(|child| count_version_ref_uses(child, ref_name))
                    .sum::<usize>()
        }
        toml::Value::Array(items) => items
            .iter()
            .map(|child| count_version_ref_uses(child, ref_name))
            .sum(),
        _ => 0,
    }
}
