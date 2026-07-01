use std::path::Path;

use colored::Colorize;

pub fn cmd_import(source: &str, format: &str) -> Result<(), String> {
    let path = Path::new(source);
    if !path.exists() {
        return Err(format!("source file not found: {}", source));
    }

    let lf_format = if format == "auto" {
        crate::importer::detect_format(path)
            .ok_or_else(|| format!("unable to detect lockfile format from '{}'", path.display()))?
    } else {
        match format {
            "npm" => crate::importer::LockfileFormat::Npm,
            "yarn" => crate::importer::LockfileFormat::Yarn,
            "pnpm" => crate::importer::LockfileFormat::Pnpm,
            "bun" => crate::importer::LockfileFormat::Bun,
            other => return Err(format!("unsupported format: {other}")),
        }
    };

    let lockfile = crate::importer::import_lockfile(path, lf_format)?;
    let out_path = Path::new("mgpm.lock");

    mgpm_lockfile::text::write_text(&lockfile, out_path)
        .map_err(|e| format!("failed to write mgpm.lock: {e}"))?;

    println!(
        "{} Imported {} packages from {} ({}) to mgpm.lock",
        "[OK]".green().bold(),
        lockfile.packages.len(),
        source,
        lf_format.as_str(),
    );
    Ok(())
}

pub fn cmd_export(output: &str) -> Result<(), String> {
    let lockfile = if Path::new("mgpm.lock").exists() {
        mgpm_lockfile::text::read_text(Path::new("mgpm.lock"))
            .map_err(|e| format!("failed to read lockfile: {e}"))?
    } else {
        return Err("no mgpm.lock found".to_string());
    };

    let mut packages_map = serde_json::Map::new();
    for pkg in &lockfile.packages {
        let node_key = format!("node_modules/{}", pkg.name);
        let mut entry = serde_json::Map::new();
        entry.insert(
            "version".to_string(),
            serde_json::Value::String(pkg.version.clone()),
        );
        entry.insert(
            "resolved".to_string(),
            serde_json::Value::String(pkg.resolution.url.clone()),
        );
        if let Some(ref integrity) = pkg.integrity {
            entry.insert(
                "integrity".to_string(),
                serde_json::Value::String(integrity.clone()),
            );
        }
        entry.insert("dev".to_string(), serde_json::Value::Bool(false));
        packages_map.insert(node_key, serde_json::Value::Object(entry));
    }

    let export = serde_json::json!({
        "name": "mgpm-export",
        "lockfileVersion": 3,
        "requires": true,
        "packages": packages_map,
    });

    let content =
        serde_json::to_string_pretty(&export).map_err(|e| format!("failed to serialize: {e}"))?;
    std::fs::write(output, &content).map_err(|e| format!("failed to write {}: {}", output, e))?;

    println!(
        "{} Exported {} packages to {}",
        "[OK]".green().bold(),
        lockfile.packages.len(),
        output,
    );
    Ok(())
}
