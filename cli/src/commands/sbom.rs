//! W6: SBOM (Software Bill of Materials) export command

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use mgc_lockfile::Lockfile;
use mgc_sbom::{SbomFormat, SbomGenerator, SbomOptions};

pub async fn run(
    format: Option<String>,
    output: Option<PathBuf>,
    name: Option<String>,
    version: Option<String>,
    dir: Option<PathBuf>,
) -> Result<()> {
    let project_root = dir.as_deref().unwrap_or_else(|| Path::new("."));

    // Parse format
    let sbom_format = match format.as_deref() {
        Some("cyclonedx-json") | Some("cyclonedx") | None => SbomFormat::CycloneDx,
        Some("spdx-json") | Some("spdx") => SbomFormat::Spdx,
        Some(other) => anyhow::bail!("Unsupported SBOM format: {}", other),
    };

    // Read lockfile
    let lockfile_path = project_root.join("mgc.lock");
    if !lockfile_path.exists() {
        anyhow::bail!(
            "No lockfile found at {}. Run `mgc install` first.",
            lockfile_path.display()
        );
    }

    let lockfile_content =
        std::fs::read_to_string(&lockfile_path).context("Failed to read lockfile")?;
    let lockfile: Lockfile =
        serde_json::from_str(&lockfile_content).context("Failed to parse lockfile")?;

    // Generate SBOM
    let options = SbomOptions {
        format: sbom_format,
        include_dev: true,
        include_licenses: true,
        include_hashes: true,
    };

    // Use component name/version from CLI args for root component metadata
    // (generator will extract from lockfile if not passed here)
    let _component_name = name.or_else(|| {
        project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
    });

    let _component_version = version.or_else(|| {
        // Try to read from package.json or mgc.toml
        if let Ok(content) = std::fs::read_to_string(project_root.join("package.json")) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                return pkg
                    .get("version")
                    .and_then(|v| v.as_str())
                    .map(String::from);
            }
        }
        None
    });

    let generator = SbomGenerator::new(options);
    let sbom_content = generator
        .generate_json(&lockfile)
        .context("Failed to generate SBOM")?;

    // Output
    if let Some(output_path) = output {
        std::fs::write(&output_path, sbom_content)
            .context(format!("Failed to write SBOM to {}", output_path.display()))?;
        println!("✓ SBOM exported to {}", output_path.display());
    } else {
        println!("{}", sbom_content);
    }

    Ok(())
}
