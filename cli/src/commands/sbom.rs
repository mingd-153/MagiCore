//! W6: SBOM (Software Bill of Materials) export command

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use mgc_lockfile::{LockDocument, Lockfile};
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
        Some("spdx-json") | Some("spdx") => {
            anyhow::bail!("SPDX output is not implemented; use `--format cyclonedx-json`")
        }
        Some(other) => anyhow::bail!("Unsupported SBOM format: {}", other),
    };

    // Read lockfile
    let lockfile_path = project_root.join("mgc.lock");
    let lockfile_bytes = match mgc_lockfile::read_lockfile_bytes(&lockfile_path) {
        Ok(bytes) => bytes,
        Err(mgc_lockfile::LockfileError::IoError(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            anyhow::bail!(
                "No lockfile found at {}. Run `mgc install` first.",
                lockfile_path.display()
            );
        }
        Err(error) => return Err(error).context("Failed to read lockfile safely"),
    };
    let lockfile_content =
        std::str::from_utf8(&lockfile_bytes).context("Lockfile is not valid UTF-8")?;
    // Accept canonical MGC TOML first and the historical JSON envelope as
    // a compatibility fallback; both paths use the bounded no-follow read.
    let lock_document = match mgc_lockfile::parse_document(lockfile_content) {
        Ok(document) => document,
        Err(toml_error) => match serde_json::from_str::<Lockfile>(lockfile_content) {
            Ok(lockfile) => LockDocument::Legacy(lockfile),
            Err(_) => return Err(toml_error).context("Failed to parse lockfile"),
        },
    };

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
        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        if let Ok(content) = std::fs::read_to_string(project_root.join("package.json"))
            && let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content)
        {
            return pkg
                .get("version")
                .and_then(|v| v.as_str())
                .map(String::from);
        }
        None
    });

    let generator = SbomGenerator::new(options);
    let sbom_content = match lock_document {
        LockDocument::Legacy(lockfile) => generator.generate_json(&lockfile),
        LockDocument::V4(lockfile) => {
            let project_config = mgc_config::project::ProjectConfig::load(project_root)?;
            let trust_keys = project_config
                .and_then(|config| config.trust)
                .map(|trust| trust.keys)
                .unwrap_or_default();
            let policy = mgc_lockfile::policy::resolve_policy(None, Some(project_root));
            let (sbom, report) =
                generator.generate_json_v4_with_report(&lockfile, policy, &trust_keys)?;
            if policy == mgc_lockfile::policy::LockPolicyMode::Warn {
                if !report.signed {
                    eprintln!("WARN: v4 lockfile is unsigned; SBOM reflects untrusted lock contents");
                } else if report
                    .key_id
                    .as_ref()
                    .is_some_and(|key_id| !trust_keys.iter().any(|trusted| trusted == key_id))
                {
                    eprintln!("WARN: v4 lockfile signature is valid but its key is not trusted by mgc.toml [trust].keys");
                }
            }
            Ok(sbom)
        }
    }
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

#[cfg(test)]
#[path = "test/sbom.rs"]
mod tests;
