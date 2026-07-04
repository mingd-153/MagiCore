use std::path::Path;

use colored::Colorize;

use super::super::{
    config_delete_value, config_get_value, config_list_values, config_set_value, cpath,
    read_user_toml, write_user_toml,
};

#[derive(clap::Subcommand)]
pub enum ConfigCommand {
    Get {
        key: String,
    },
    Set {
        key: String,
        value: String,
        #[arg(long)]
        scope: Option<String>,
    },
    Delete {
        key: String,
    },
    List,
    /// Generate a default mg.yaml in the current directory
    Init {
        /// Overwrite existing config file
        #[arg(long, short)]
        force: bool,
        /// Config format: yaml, yml, or toml
        #[arg(long, default_value = "yaml")]
        format: String,
    },
    /// Manage trusted dependencies
    Trusted {
        #[command(subcommand)]
        command: TrustedAction,
    },
}

#[derive(clap::Subcommand)]
pub enum TrustedAction {
    /// Add a package to trusted list
    Add { package: String },
    /// Remove a package from trusted list
    Remove { package: String },
    /// List trusted packages
    List,
}

#[derive(clap::Subcommand)]
pub enum AuditCommand {
    /// Run an audit against the advisory database
    Run {
        #[arg(short, long)]
        json: bool,
        #[arg(long)]
        severity: Option<String>,
        #[arg(long)]
        remote: bool,
    },
    /// Update the advisory database using TUF
    Update {
        #[arg(long)]
        force: bool,
    },
}

pub async fn cmd_audit(
    config: &mg_core::config::MgpmConfig,
    json: bool,
    severity: Option<String>,
    remote: bool,
) -> Result<(), String> {
    if let Ok(project_dir) = std::env::current_dir() {
        for warning in crate::auth::check_auth_security(&project_dir) {
            eprintln!("  {} {}", "[WARN]".yellow().bold(), warning.yellow());
        }
    }

    for reg in &config.registries {
        if let Some(w) = crate::auth::check_url_for_credentials(&reg.url) {
            eprintln!("  {} {}", "[WARN]".yellow().bold(), w.yellow());
        }
        if let Some(w) = crate::auth::check_url_for_query_token(&reg.url) {
            eprintln!("  {} {}", "[WARN]".yellow().bold(), w.yellow());
        }
    }

    let lockfile = if Path::new("mg.lock").exists() {
        mg_lockfile::text::read_text(Path::new("mg.lock"))
            .map_err(|e| format!("failed to read lockfile: {e}"))?
    } else {
        return Err("no mg.lock found".to_string());
    };

    let mut db = crate::advisory_db::AdvisoryDb::new();
    if remote {
        match db.fetch_remote().await {
            Err(e) => eprintln!(
                "  {} Failed to fetch remote advisories: {}",
                "[WARN]".yellow().bold(),
                e
            ),
            Ok(()) => eprintln!("  {} Fetched remote advisories", "[OK]".green().bold()),
        }
    }
    let mut findings = Vec::new();

    for pkg in &lockfile.packages {
        let matches = db.check(&pkg.name, &pkg.version);
        for advisory in matches {
            if let Some(ref sev) = severity {
                if advisory.severity != *sev {
                    continue;
                }
            }
            findings.push((pkg, advisory));
        }
    }

    if json {
        let output: Vec<serde_json::Value> = findings
            .iter()
            .map(|(pkg, adv)| {
                serde_json::json!({
                    "package": pkg.name,
                    "installed": pkg.version,
                    "severity": adv.severity,
                    "description": adv.description,
                    "vulnerable_versions": adv.vulnerable_versions,
                    "patched_versions": adv.patched_versions,
                    "cve": adv.cve,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        if findings.is_empty() {
            println!("{} No known vulnerabilities found", "[OK]".green().bold());
            return Ok(());
        }
        println!(
            "{} Found {} known vulnerabilities:",
            "[WARN]".yellow().bold(),
            findings.len()
        );
        for (pkg, adv) in &findings {
            let sev_color = match adv.severity.as_str() {
                "critical" => "critical".red().bold(),
                "high" => "high".red(),
                "moderate" => "moderate".yellow(),
                _ => "low".dimmed(),
            };
            println!(
                "  {} {}@{} ({}) - {}",
                sev_color,
                pkg.name.cyan(),
                pkg.version,
                adv.severity,
                adv.description
            );
            println!("    Fix: {}", adv.patched_versions);
            if let Some(ref cve) = adv.cve {
                println!("    CVE: {}", cve);
            }
        }
    }
    Ok(())
}

pub fn handle_config_command(cmd: ConfigCommand) -> Result<(), String> {
    match cmd {
        ConfigCommand::Get { key } => {
            let val = config_get_value(&key)?;
            println!("{}", val);
            Ok(())
        }
        ConfigCommand::Set { key, value, scope } => {
            if key == "_authToken" || key == "registry" {
                return set_npmrc_value(&key, &value, scope.as_deref());
            }
            let msg = config_set_value(&key, &value)?;
            println!("{} {}", "[OK]".green().bold(), msg);
            Ok(())
        }
        ConfigCommand::Delete { key } => {
            let msg = config_delete_value(&key)?;
            println!("{} {}", "[OK]".green().bold(), msg);
            Ok(())
        }
        ConfigCommand::List => {
            let msg = config_list_values()?;
            println!("{}", msg);
            Ok(())
        }
        ConfigCommand::Init { force, format } => cmd_config_init(force, &format),
        ConfigCommand::Trusted { command } => handle_config_trusted(command),
    }
}

pub fn cmd_config_init(force: bool, format: &str) -> Result<(), String> {
    let filename = match format {
        "yaml" | "yml" => "mg.yaml",
        "toml" => "mg.toml",
        other => return Err(format!("unsupported format: {other}. Use yaml, yml, or toml")),
    };
    let config_path = std::path::Path::new(filename);

    if config_path.exists() && !force {
        return Err(format!(
            "{} already exists. Use --force to overwrite",
            filename
        ));
    }

    let config = mg_core::config::MgpmConfig::default();

    let content = match format {
        "toml" => toml::to_string_pretty(&config)
            .map_err(|e| format!("failed to serialize config: {e}"))?,
        _ => serde_yaml::to_string(&config)
            .map_err(|e| format!("failed to serialize config: {e}"))?,
    };

    std::fs::write(config_path, content)
        .map_err(|e| format!("failed to write {}: {e}", filename))?;

    println!(
        "{} Created {}",
        "[OK]".green().bold(),
        filename.cyan()
    );
    Ok(())
}

fn handle_config_trusted(action: TrustedAction) -> Result<(), String> {
    match action {
        TrustedAction::Add { package } => {
            let mut doc = read_user_toml()?;
            let trusted = doc
                .as_table_mut()
                .ok_or("root is not a table")?
                .entry("trusted")
                .or_insert_with(|| toml::Value::Array(Vec::new()));
            let arr = trusted.as_array_mut().ok_or("trusted is not an array")?;
            if !arr.iter().any(|v| v.as_str() == Some(&package)) {
                arr.push(toml::Value::String(package.clone()));
            }
            write_user_toml(&doc)?;
            println!(
                "{} Added {} to trusted packages",
                "[OK]".green().bold(),
                package
            );
            Ok(())
        }
        TrustedAction::Remove { package } => {
            let mut doc = read_user_toml()?;
            let trusted = doc
                .as_table_mut()
                .ok_or("root is not a table")?
                .entry("trusted")
                .or_insert_with(|| toml::Value::Array(Vec::new()));
            let arr = trusted.as_array_mut().ok_or("trusted is not an array")?;
            arr.retain(|v| v.as_str() != Some(&package));
            write_user_toml(&doc)?;
            println!(
                "{} Removed {} from trusted packages",
                "[OK]".green().bold(),
                package
            );
            Ok(())
        }
        TrustedAction::List => {
            let doc = read_user_toml()?;
            let trusted = doc
                .as_table()
                .and_then(|t| t.get("trusted"))
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if trusted.is_empty() {
                println!("No trusted packages configured");
            } else {
                println!("Trusted packages:");
                for pkg in &trusted {
                    println!("  - {}", pkg);
                }
            }
            Ok(())
        }
    }
}

fn set_npmrc_value(key: &str, value: &str, scope: Option<&str>) -> Result<(), String> {
    let home =
        std::env::var("HOME").map_err(|_| "HOME not set; cannot write ~/.npmrc".to_string())?;
    let npmrc_path = Path::new(&home).join(".npmrc");

    let mut content = String::new();
    if npmrc_path.exists() {
        content = std::fs::read_to_string(&npmrc_path)
            .map_err(|e| format!("failed to read .npmrc: {}", e))?;
    }

    let entry = if scope.is_some() {
        format!("//registry.npmjs.org/:_authToken={}\n", value)
    } else if key == "registry" {
        format!("registry={}\n", value)
    } else if key == "_authToken" {
        format!("//registry.npmjs.org/:_authToken={}\n", value)
    } else {
        format!("{}={}\n", key, value)
    };

    content.push_str(&entry);
    std::fs::write(&npmrc_path, &content).map_err(|e| format!("failed to write .npmrc: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) =
            std::fs::set_permissions(&npmrc_path, std::fs::Permissions::from_mode(0o600))
        {
            eprintln!(
                "  {} Failed to set permissions on .npmrc: {}",
                "[WARN]".yellow().bold(),
                e
            );
        }
    }

    println!(
        "{} {} in {}",
        "[OK]".green().bold(),
        format!("Set {} = {}", key, redact_if_auth(key, value)).cyan(),
        cpath(&npmrc_path)
    );
    Ok(())
}

fn redact_if_auth(key: &str, value: &str) -> String {
    if key.contains("auth")
        || key.contains("token")
        || key.contains("password")
        || key.contains("_auth")
    {
        crate::auth::redact_auth(value)
    } else {
        value.to_string()
    }
}
