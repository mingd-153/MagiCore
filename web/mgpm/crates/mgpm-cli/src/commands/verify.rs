use std::path::{Path, PathBuf};

use colored::Colorize;

use mgpm_core::config::MgpmConfig;
use mgpm_store::ContentStore;

pub fn cmd_verify(config: &MgpmConfig) -> Result<(), String> {
    let lockfile = if Path::new("mgpm.lock").exists() {
        mgpm_lockfile::text::read_text(Path::new("mgpm.lock"))
            .map_err(|e| format!("failed to read lockfile: {e}"))?
    } else {
        return Err("no mgpm.lock found".to_string());
    };

    let store_path = config.store.store_path();
    if !store_path.exists() {
        return Err(format!("store not found at {}", store_path.display()));
    }

    let store = ContentStore::new(store_path).map_err(|e| format!("failed to open store: {e}"))?;

    let mut verified = 0u32;
    let mut missing = 0u32;
    let mut corrupted = 0u32;

    for pkg in &lockfile.packages {
        let status = if let Some(ref integrity) = pkg.integrity {
            let hex_hash = sri_hash_to_hex(integrity);
            if let Some(ref hash) = hex_hash {
                if store.has_file(hash) {
                    match store.get_file(hash) {
                        Ok(path) => match store.verify_integrity(hash, &path) {
                            Ok(true) => "verified".to_string(),
                            _ => "corrupted".to_string(),
                        },
                        Err(_) => "missing".to_string(),
                    }
                } else {
                    "missing".to_string()
                }
            } else {
                "unable to parse integrity".to_string()
            }
        } else {
            "no integrity field".to_string()
        };

        match status.as_str() {
            "verified" => {
                println!("  {} {}@{}", "[OK]".green(), pkg.name.cyan(), pkg.version);
                verified += 1;
            }
            "missing" | "no integrity field" | "unable to parse integrity" => {
                println!(
                    "  {} {}@{} - {}",
                    "[MISS]".yellow(),
                    pkg.name.cyan(),
                    pkg.version,
                    status
                );
                missing += 1;
            }
            _ => {
                println!(
                    "  {} {}@{} - {}",
                    "[ERR]".red(),
                    pkg.name.cyan(),
                    pkg.version,
                    status
                );
                corrupted += 1;
            }
        }
    }

    println!(
        "{} Verified: {}, Missing: {}, Corrupted: {}",
        "[DONE]".green().bold(),
        verified,
        missing,
        corrupted,
    );

    if missing > 0 || corrupted > 0 {
        Err(format!(
            "{} package(s) missing and {} corrupted",
            missing, corrupted
        ))
    } else {
        Ok(())
    }
}

fn sri_hash_to_hex(sri: &str) -> Option<String> {
    let (_algo, b64) = sri.split_once('-')?;
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64).ok()?;
    Some(hex::encode(bytes))
}

pub fn cmd_verify_deep(config: &MgpmConfig) -> Result<(), String> {
    let lockfile = if Path::new("mgpm.lock").exists() {
        mgpm_lockfile::text::read_text(Path::new("mgpm.lock"))
            .map_err(|e| format!("failed to read lockfile: {e}"))?
    } else {
        return Err("no mgpm.lock found".to_string());
    };

    let store_path = config.store.store_path();
    if !store_path.exists() {
        return Err(format!("store not found at {}", store_path.display()));
    }

    let store = ContentStore::new(store_path).map_err(|e| format!("failed to open store: {e}"))?;

    // Build map of installed packages from node_modules
    let nm_path = Path::new("node_modules");
    let mut installed: std::collections::HashMap<String, PathBuf> =
        std::collections::HashMap::new();
    if nm_path.exists() {
        if let Ok(entries) = std::fs::read_dir(nm_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || !path.is_dir() {
                    continue;
                }
                if name.starts_with('@') {
                    if let Ok(sub) = std::fs::read_dir(&path) {
                        for s in sub.flatten() {
                            let sp = s.path();
                            if sp.is_dir() && sp.join("package.json").exists() {
                                installed.insert(
                                    format!("{}/{}", name, s.file_name().to_string_lossy()),
                                    sp,
                                );
                            }
                        }
                    }
                } else if path.join("package.json").exists() {
                    installed.insert(name, path);
                }
            }
        }
    }

    let mut ok = 0u32;
    let mut fail = 0u32;
    let mut miss = 0u32;

    for pkg in &lockfile.packages {
        match installed.get(&pkg.name) {
            Some(_pkg_dir) => {
                let store_ok = match pkg.integrity.as_ref().and_then(|s| sri_hash_to_hex(s)) {
                    Some(h) => store.has_file(&h) && store.get_file(&h).is_ok(),
                    None => true,
                };
                if store_ok {
                    println!("  {} {}@{}", "✓".green(), pkg.name.cyan(), pkg.version);
                    ok += 1;
                } else {
                    println!(
                        "  {} {}@{} (store integrity mismatch)",
                        "✗".red(),
                        pkg.name.red(),
                        pkg.version.red()
                    );
                    fail += 1;
                }
            }
            None => {
                println!(
                    "  {} {}@{} (not in node_modules)",
                    "✗".red(),
                    pkg.name.red(),
                    pkg.version.red()
                );
                miss += 1;
            }
        }
    }

    let lockfile_names: std::collections::HashSet<&str> =
        lockfile.packages.iter().map(|p| p.name.as_str()).collect();
    for name in installed.keys() {
        if !lockfile_names.contains(name.as_str()) {
            println!("  {} {} (not in lockfile)", "[!]".yellow(), name.yellow());
        }
    }

    println!(
        "{} Verified: {}, Store mismatches: {}, Missing from node_modules: {}",
        "[DONE]".green().bold(),
        ok,
        fail,
        miss,
    );

    if fail > 0 || miss > 0 {
        Err(format!(
            "{} store mismatch(es), {} missing from node_modules",
            fail, miss
        ))
    } else {
        Ok(())
    }
}
