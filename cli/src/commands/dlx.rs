use anyhow::Result;
use std::process::Command;

/// mg dlx <package[@version]> [args...] — download+execute a package without installing it permanently
/// Similar to pnpm dlx / npx (but cached for reuse).
pub async fn run(package: String, args: Vec<String>) -> Result<()> {
    // Determine cache dir for dlx
    let dlx_cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from(".megagate-dlx"))
        .join("megagate")
        .join("dlx");
    std::fs::create_dir_all(&dlx_cache_dir)?;

    // Parse package name and optional version
    let (pkg_name, pkg_version) = if let Some(at_pos) = package.rfind('@') {
        if at_pos > 0 {
            (&package[..at_pos], Some(&package[at_pos + 1..]))
        } else {
            // scoped packages like @org/name (starts with @)
            (&package[..], None)
        }
    } else {
        (&package[..], None)
    };

    let version_tag = pkg_version.unwrap_or("latest");
    let slug = pkg_name.replace('/', "__").replace('@', "_");
    let pkg_dir = dlx_cache_dir.join(format!("{}__{}", slug, version_tag));

    // Install the package into temporary isolated dir if not cached
    if !pkg_dir.join("node_modules").join(".bin").exists()
        && !pkg_dir.join("node_modules").join(pkg_name).exists()
    {
        mg_ui::info(&format!("dlx: installing {}@{}", pkg_name, version_tag));
        std::fs::create_dir_all(&pkg_dir)?;

        // Create a minimal package.json
        let pkg_json = serde_json::json!({
            "name": "megagate-dlx-env",
            "version": "1.0.0",
            "dependencies": {
                pkg_name: version_tag
            }
        });
        std::fs::write(
            pkg_dir.join("package.json"),
            serde_json::to_string_pretty(&pkg_json)?,
        )?;

        // Run mg install in this directory
        let mg_bin = std::env::current_exe()?;
        let install_status = Command::new(&mg_bin)
            .arg("install")
            .arg("--ignore-scripts")
            .current_dir(&pkg_dir)
            .status()?;

        if !install_status.success() {
            anyhow::bail!("dlx: failed to install '{}'", package);
        }
    }

    // Find the binary
    let bin_dir = pkg_dir.join("node_modules").join(".bin");
    let bin_name = pkg_name.split('/').next_back().unwrap_or(pkg_name);
    let bin_path = bin_dir.join(bin_name);

    if !bin_path.exists() {
        // Try to find any executable in the package bin dir
        anyhow::bail!(
            "dlx: no binary '{}' found for package '{}'. Is it an executable package?",
            bin_name,
            pkg_name
        );
    }

    mg_ui::info(&format!("$ {} {}", bin_path.display(), args.join(" ")));

    let status = Command::new(&bin_path)
        .args(&args)
        .current_dir(std::env::current_dir()?)
        .status()?;

    if !status.success() {
        let code = status.code().unwrap_or(1);
        std::process::exit(code);
    }

    Ok(())
}
