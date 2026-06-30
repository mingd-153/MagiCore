use std::path::Path;

use colored::Colorize;

#[derive(clap::Subcommand)]
pub enum LockfileSubcommand {
    /// Upgrade lockfile format (text/binary)
    Upgrade {
        #[arg(short, long, default_value = "both")]
        to: String,
    },
    /// Validate lockfile integrity
    Validate,
    /// Migrate lockfile from v1 to v2 format
    ///
    /// v2 uses BLAKE3 content hashing instead of DefaultHasher for stronger integrity guarantees.
    /// This command verifies the v1 content hash before upgrading.
    Migrate,
}

pub fn cmd_lockfile(command: LockfileSubcommand) -> Result<(), String> {
    match command {
        LockfileSubcommand::Upgrade { to } => cmd_lockfile_upgrade(to),
        LockfileSubcommand::Validate => cmd_lockfile_validate(),
        LockfileSubcommand::Migrate => cmd_lockfile_migrate(),
    }
}

fn cmd_lockfile_upgrade(to: String) -> Result<(), String> {
    let text_path = Path::new("mgpm.lock");
    let binary_path = Path::new("mgpm.lockb");

    let lockfile = if text_path.exists() {
        mgpm_lockfile::text::read_text(text_path)
            .map_err(|e| format!("failed to read text lockfile: {e}"))?
    } else if binary_path.exists() {
        mgpm_lockfile::binary::read_binary(binary_path)
            .map_err(|e| format!("failed to read binary lockfile: {e}"))?
    } else {
        return Err("no lockfile found (mgpm.lock or mgpm.lockb)".to_string());
    };

    match to.as_str() {
        "text" => {
            mgpm_lockfile::text::write_text(&lockfile, text_path)
                .map_err(|e| format!("failed to write text lockfile: {e}"))?;
            println!(
                "{} Written lockfile to text format (mgpm.lock)",
                "[OK]".green().bold()
            );
        }
        "binary" => {
            mgpm_lockfile::binary::write_binary(&lockfile, binary_path)
                .map_err(|e| format!("failed to write binary lockfile: {e}"))?;
            println!(
                "{} Written lockfile to binary format (mgpm.lockb)",
                "[OK]".green().bold()
            );
        }
        "both" => {
            mgpm_lockfile::text::write_text(&lockfile, text_path)
                .map_err(|e| format!("failed to write text lockfile: {e}"))?;
            mgpm_lockfile::binary::write_binary(&lockfile, binary_path)
                .map_err(|e| format!("failed to write binary lockfile: {e}"))?;
            println!(
                "{} Written lockfile to both formats (mgpm.lock, mgpm.lockb)",
                "[OK]".green().bold()
            );
        }
        _ => {
            return Err(format!(
                "unknown format '{}' (use 'text', 'binary', or 'both')",
                to
            ))
        }
    }

    Ok(())
}

fn cmd_lockfile_validate() -> Result<(), String> {
    let text_path = Path::new("mgpm.lock");
    let binary_path = Path::new("mgpm.lockb");

    let lockfile = if text_path.exists() {
        eprintln!(
            "{} Reading lockfile from mgpm.lock...",
            "[INFO]".cyan().bold()
        );
        mgpm_lockfile::text::read_text(text_path)
            .map_err(|e| format!("failed to read text lockfile: {e}"))?
    } else if binary_path.exists() {
        eprintln!(
            "{} Reading lockfile from mgpm.lockb...",
            "[INFO]".cyan().bold()
        );
        mgpm_lockfile::binary::read_binary(binary_path)
            .map_err(|e| format!("failed to read binary lockfile: {e}"))?
    } else {
        return Err("no lockfile found (mgpm.lock or mgpm.lockb)".to_string());
    };

    let mut issues = Vec::new();

    if lockfile.version != mgpm_lockfile::LOCKFILE_VERSION {
        issues.push(format!(
            "version mismatch: found {}, expected {}",
            lockfile.version,
            mgpm_lockfile::LOCKFILE_VERSION
        ));
    }

    if lockfile.metadata.content_hash.is_empty() {
        issues.push("content hash is empty".to_string());
    } else if !lockfile.verify_content_hash() {
        issues.push("content hash mismatch — lockfile has been tampered with".to_string());
    }

    for (i, pkg) in lockfile.packages.iter().enumerate() {
        if pkg.name.is_empty() {
            issues.push(format!("package at index {} has empty name", i));
        }
        if pkg.version.is_empty() {
            issues.push(format!("package '{}' has empty version", pkg.name));
        }
        if pkg.resolution.url.is_empty() {
            issues.push(format!("package '{}' has empty resolution URL", pkg.name));
        }
        if pkg.id.is_empty() {
            issues.push(format!("package at index {} has empty id", i));
        }
    }

    if issues.is_empty() {
        println!(
            "{} Lockfile is valid ({} packages, hash: {})",
            "[OK]".green().bold(),
            lockfile.packages.len(),
            lockfile.metadata.content_hash
        );
    } else {
        eprintln!(
            "{} Found {} issue(s):",
            "[WARN]".yellow().bold(),
            issues.len()
        );
        for issue in &issues {
            eprintln!("  - {}", issue.red());
        }
        return Err(format!(
            "lockfile validation failed with {} issue(s)",
            issues.len()
        ));
    }

    Ok(())
}

fn cmd_lockfile_migrate() -> Result<(), String> {
    let text_path = Path::new("mgpm.lock");
    let binary_path = Path::new("mgpm.lockb");

    if !text_path.exists() && !binary_path.exists() {
        return Err("no lockfile found (mgpm.lock or mgpm.lockb)".to_string());
    }

    let lockfile = if text_path.exists() {
        eprintln!(
            "{} Reading lockfile from mgpm.lock...",
            "[INFO]".cyan().bold()
        );
        mgpm_lockfile::text::read_text(text_path)
            .map_err(|e| format!("failed to read text lockfile: {e}"))?
    } else {
        eprintln!(
            "{} Reading lockfile from mgpm.lockb...",
            "[INFO]".cyan().bold()
        );
        mgpm_lockfile::binary::read_binary(binary_path)
            .map_err(|e| format!("failed to read binary lockfile: {e}"))?
    };

    if lockfile.version == mgpm_lockfile::LOCKFILE_VERSION {
        println!(
            "{} Lockfile is already at v{} (latest). No migration needed.",
            "[OK]".green().bold(),
            mgpm_lockfile::LOCKFILE_VERSION
        );
        return Ok(());
    }

    if lockfile.version != mgpm_lockfile::lockfile::LOCKFILE_VERSION_V1 {
        return Err(format!(
            "unsupported lockfile version {} (expected v1 or v{})",
            lockfile.version,
            mgpm_lockfile::LOCKFILE_VERSION
        ));
    }

    eprintln!("{} Found v1 lockfile. Migrating to v2...", "[INFO]".cyan().bold());

    let mut lockfile = lockfile;
    lockfile.migrate_v1_to_v2()
        .map_err(|e| format!("migration failed: {e}"))?;

    // Write both formats
    mgpm_lockfile::text::write_text(&lockfile, text_path)
        .map_err(|e| format!("failed to write text lockfile: {e}"))?;
    mgpm_lockfile::binary::write_binary(&lockfile, binary_path)
        .map_err(|e| format!("failed to write binary lockfile: {e}"))?;

    println!(
        "{} Lockfile migrated from v1 to v{} (BLAKE3 content hashing enabled)",
        "[OK]".green().bold(),
        mgpm_lockfile::LOCKFILE_VERSION
    );

    Ok(())
}
