use std::path::Path;

use colored::Colorize;

pub fn cmd_init() -> Result<(), String> {
    eprintln!(
        "{} {}",
        "[INFO]".cyan().bold(),
        "Initializing project...".cyan()
    );

    if !Path::new("package.json").exists() {
        scaffold_package_json()?;
        eprintln!("  {} Created package.json", "[OK]".green().bold());
    } else {
        eprintln!(
            "  {} package.json already exists, skipping",
            "[WARN]".yellow().bold()
        );
    }

    if !Path::new("mgpm.yaml").exists() {
        scaffold_mgpm_yaml()?;
        eprintln!("  {} Created mgpm.yaml", "[OK]".green().bold());
    } else {
        eprintln!(
            "  {} mgpm.yaml already exists, skipping",
            "[WARN]".yellow().bold()
        );
    }

    eprintln!(
        "{} {} {}",
        "[OK]".green().bold(),
        "Project initialized.",
        "Run `mgpm install` to install dependencies.".green()
    );
    Ok(())
}

fn scaffold_package_json() -> Result<(), String> {
    let content = serde_json::to_string_pretty(&serde_json::json!({
        "name": "my-project",
        "version": "0.1.0",
        "private": true,
        "scripts": {
            "test": "echo \"Error: no test specified\" && exit 1"
        }
    }))
    .map_err(|e| format!("failed to serialize package.json: {}", e))?;
    std::fs::write("package.json", content)
        .map_err(|e| format!("failed to write package.json: {}", e))?;
    Ok(())
}

fn scaffold_mgpm_yaml() -> Result<(), String> {
    let content = r#"# MegaGate Package Manager configuration
version: 1

# Registry configuration
registries:
  - url: "https://registry.npmjs.org"
    type: npm

# Catalog for version pinning
# catalogs:
#   default:
#     typescript: "^5.0.0"

# Installation options
install:
  hoist: false
  symlinks: true
  strict_peer_deps: true
  concurrency: 16
  retries: 3
"#;
    std::fs::write("mgpm.yaml", content)
        .map_err(|e| format!("failed to write mgpm.yaml: {}", e))?;
    Ok(())
}
