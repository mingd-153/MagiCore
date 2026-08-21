//! `mg import` — Import and migrate legacy package-manager lockfiles to mg.lock.
//!
//! Supported lockfiles:
//! - `package-lock.json` (npm v2, v3)
//! - `pnpm-lock.yaml` (pnpm v6, v9)
//! - `yarn.lock` (yarn v1)
//! - `bun.lock` (bun v1)

use anyhow::{Context, Result};

/// Run `mg import` in project directory.
/// Chuyển đổi lockfile cũ thành mg.lock chuẩn xác và an toàn.
pub async fn run(project_dir: Option<std::path::PathBuf>) -> Result<()> {
    let cwd = project_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let detected = mg_lockfile::import::detect_legacy_lockfiles(&cwd);

    if detected.is_empty() {
        mg_ui::warning(
            "No legacy lockfiles found (package-lock.json, pnpm-lock.yaml, yarn.lock, bun.lock).",
        );
        return Ok(());
    }

    let manifest_path = cwd.join("package.json");
    if !manifest_path.exists() {
        anyhow::bail!("manifest package.json not found in {}", cwd.display());
    }

    let content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let pkg_json: serde_json::Value =
        serde_json::from_str(&content).with_context(|| "failed to parse package.json")?;

    let name = pkg_json
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed");
    let mut manifest = mg_types::Manifest::new(name, mg_types::Ecosystem::Web);

    // Extract production dependencies from package.json
    if let Some(deps) = pkg_json.get("dependencies").and_then(|v| v.as_object()) {
        for (pkg_name, range) in deps {
            if let Some(r_str) = range.as_str() {
                if let (Ok(p_name), Ok(v_range)) = (
                    mg_types::PackageName::new(pkg_name),
                    mg_types::VersionRange::parse(r_str),
                ) {
                    manifest.add_dep(
                        mg_types::DependencySpec::new(p_name, v_range),
                        false,
                        false,
                        false,
                    );
                }
            }
        }
    }

    // Extract dev dependencies from package.json
    if let Some(dev_deps) = pkg_json.get("devDependencies").and_then(|v| v.as_object()) {
        for (pkg_name, range) in dev_deps {
            if let Some(r_str) = range.as_str() {
                if let (Ok(p_name), Ok(v_range)) = (
                    mg_types::PackageName::new(pkg_name),
                    mg_types::VersionRange::parse(r_str),
                ) {
                    manifest.add_dep(
                        mg_types::DependencySpec::new(p_name, v_range),
                        true,
                        false,
                        false,
                    );
                }
            }
        }
    }

    let mode = "frontend";
    let core = "web";

    let imported =
        mg_lockfile::import::import_legacy_lockfile_explicit(&cwd, core, mode, &manifest)?
            .ok_or_else(|| anyhow::anyhow!("failed to import legacy lockfile"))?;

    let package_count = imported.packages.len();
    mg_lockfile::write_lockfile(&cwd, &imported)
        .with_context(|| "failed to write imported mg.lock")?;

    // Auto-create .mg.core signature marker if missing
    let marker_path = cwd.join(mg_config::project::ProjectConfig::CORE_MARKER_FILE);
    if !marker_path.exists() {
        let _ = std::fs::write(&marker_path, format!("{core}\n"));
    }

    let sources: Vec<&str> = detected.iter().map(|d| d.file_name).collect();
    mg_ui::success(&format!(
        "Imported {} packages from {} into mg.lock (and generated checksum).",
        package_count,
        sources.join(", ")
    ));

    Ok(())
}

#[cfg(test)]
#[path = "test/import.rs"]
mod tests;
