use std::io;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCmd;

use clap::{CommandFactory, Parser};
use colored::Colorize;
use tokio::sync::mpsc;

use mgpm_core::config::MgpmConfig;
use mgpm_installer::installer::{InstallOptions as RealInstallOptions, Installer};
use mgpm_store::ContentStore;

mod profiler;
mod advisory_db;
mod tuf;
mod importer;
mod sandbox;
mod auth;

use profiler::PhaseProfiler;
use advisory_db::AdvisoryDb;
use importer::{detect_format, import_lockfile, LockfileFormat};

#[derive(Parser)]
#[command(name = "mgpm", version, about = "MegaGate Package Manager")]
struct Cli {
    #[arg(short = 'r', long, global = true)]
    recursive: bool,
    #[arg(short = 'F', long, global = true)]
    filter: Vec<String>,
    #[arg(long, global = true)]
    since: Option<String>,
    #[arg(long, global = true)]
    fail_fast: bool,
    #[arg(long, global = true)]
    profile: bool,
    #[arg(long, global = true)]
    timings: bool,
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(clap::Subcommand)]
enum CliCommand {
    Install {
        #[arg(short, long)]
        offline: bool,
        #[arg(long)]
        frozen_lockfile: bool,
        #[arg(short, long)]
        production: bool,
        #[arg(long)]
        hoist: bool,
        #[arg(long)]
        sandbox: bool,
    },
    Add {
        packages: Vec<String>,
        #[arg(short, long)]
        dev: bool,
        #[arg(short, long)]
        peer: bool,
        #[arg(short, long)]
        optional: bool,
        #[arg(short, long)]
        exact: bool,
    },
    Remove {
        packages: Vec<String>,
    },
    Update {
        #[arg(short, long)]
        latest: bool,
    },
    Run {
        script: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    Exec {
        command: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    Store {
        #[command(subcommand)]
        command: StoreCommand,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Init,
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Lockfile operations
    Lockfile {
        #[command(subcommand)]
        command: LockfileSubcommand,
    },
    /// Daemon management
    Daemon {
        #[command(subcommand)]
        command: DaemonSubcommand,
    },
    /// Check for known vulnerabilities
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
    /// Verify lockfile integrity
    Verify {
        /// Deep verification: walk node_modules and cross-reference with lockfile
        #[arg(long)]
        deep: bool,
    },
    /// Import from another package manager's lockfile
    Import {
        source: String,
        #[arg(short, long, default_value = "auto")]
        format: String,
    },
    /// Export to npm-compatible package-lock.json
    Export {
        #[arg(short, long, default_value = "package-lock.json")]
        output: String,
    },
}

#[derive(clap::Subcommand)]
enum StoreCommand {
    Prune,
    Status,
    /// Manage completion cache
    CompletionsCache {
        #[command(subcommand)]
        command: CompletionsCacheAction,
    },
}

#[derive(clap::Subcommand)]
enum CompletionsCacheAction {
    /// Warm the completion cache
    Warm,
    /// Clear the completion cache
    Clear,
}

#[derive(clap::Subcommand)]
enum LockfileSubcommand {
    /// Upgrade lockfile format
    Upgrade {
        #[arg(short, long, default_value = "both")]
        to: String,
    },
    /// Validate lockfile integrity
    Validate,
}

#[derive(clap::Subcommand)]
enum DaemonSubcommand {
    /// Start the daemon
    Start,
    /// Stop the daemon
    Stop,
    /// Check daemon status
    Status,
}

#[derive(clap::Subcommand)]
enum AuditCommand {
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

#[derive(clap::Subcommand)]
enum TrustedAction {
    /// Add a package to trusted list
    Add { package: String },
    /// Remove a package from trusted list
    Remove { package: String },
    /// List trusted packages
    List,
}

#[derive(clap::Subcommand)]
enum ConfigCommand {
    Get { key: String },
    Set {
        key: String,
        value: String,
        #[arg(long)]
        scope: Option<String>,
    },
    Delete { key: String },
    List,
    /// Manage trusted dependencies
    Trusted {
        #[command(subcommand)]
        command: TrustedAction,
    },
}

// ---------------------------------------------------------------------------
// Config loading with precedence: project config > user config > env > defaults
// ---------------------------------------------------------------------------

fn load_config() -> (MgpmConfig, Option<PathBuf>) {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let uc_path = home.join(".config").join("mgpm").join("config.toml");
    let mut user_config_path: Option<PathBuf> = None;

    let mut config = MgpmConfig::default();

    if uc_path.exists() {
        user_config_path = Some(uc_path.clone());
        if let Ok(uc) = MgpmConfig::load(&uc_path) {
            merge_into(&mut config, uc);
        }
    }

    for path in &[PathBuf::from("mgpm.yaml"), PathBuf::from("mgpm.toml")] {
        if path.exists() {
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let pc: Option<MgpmConfig> =
                if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
                    serde_yaml::from_str(&content).ok()
                } else {
                    toml::from_str(&content).ok()
                };
            if let Some(pc) = pc {
                merge_into(&mut config, pc);
            }
            break;
        }
    }

    apply_npmrc_config(&mut config);
    apply_env_overrides(&mut config);
    (config, user_config_path)
}

fn merge_into(base: &mut MgpmConfig, overlay: MgpmConfig) {
    if overlay.workspace.is_some() {
        base.workspace = overlay.workspace;
    }
    if !overlay.catalogs.is_empty() {
        base.catalogs = overlay.catalogs;
    }
    if !overlay.overrides.is_empty() {
        base.overrides = overlay.overrides;
    }
    if !overlay.registries.is_empty() {
        base.registries = overlay.registries;
    }
    if !overlay.trusted.is_empty() {
        base.trusted = overlay.trusted;
    }
    if !overlay.scoped_registries.is_empty() {
        base.scoped_registries = overlay.scoped_registries;
    }
    if !overlay.trusted_registries.is_empty() {
        base.trusted_registries = overlay.trusted_registries;
    }
    base.install = overlay.install;
    base.store = overlay.store;
    base.cli = overlay.cli;
}

fn apply_env_overrides(config: &mut MgpmConfig) {
    use std::env::var;

    if let Ok(v) = var("MGPM_HOIST") {
        config.install.hoist = v == "true" || v == "1";
    }
    if let Ok(v) = var("MGPM_CONCURRENCY") {
        if let Ok(n) = v.parse::<usize>() {
            config.install.concurrency = n;
        }
    }
    if let Ok(v) = var("MGPM_RETRIES") {
        if let Ok(n) = v.parse::<u32>() {
            config.install.retries = n;
        }
    }
    if let Ok(v) = var("MGPM_SYMLINKS") {
        config.install.symlinks = v == "true" || v == "1";
    }
    if let Ok(v) = var("MGPM_STRICT_PEER_DEPS") {
        config.install.strict_peer_deps = v == "true" || v == "1";
    }
    if let Ok(v) = var("MGPM_STORE_PATH") {
        config.store.path = Some(PathBuf::from(v));
    }
    if let Ok(v) = var("MGPM_NO_COLOR") {
        config.cli.color = !(v == "true" || v == "1");
    }
    if let Ok(v) = var("MGPM_LOG_LEVEL") {
        config.cli.log_level = v;
    }
    if let Ok(v) = var("MGPM_JSON") {
        config.cli.json = v == "true" || v == "1";
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_package_json(path: &Path) -> Result<serde_json::Value, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    serde_json::from_str(&content).map_err(|e| format!("failed to parse {}: {}", path.display(), e))
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.2} {}", size, UNITS[unit])
    }
}

fn scan_store_size(store_path: &Path) -> io::Result<(usize, u64)> {
    let files_dir = store_path.join("files");
    if !files_dir.exists() {
        return Ok((0, 0));
    }
    let mut file_count = 0usize;
    let mut total_size = 0u64;
    scan_dir_recursive(&files_dir, &mut file_count, &mut total_size)?;
    Ok((file_count, total_size))
}

fn scan_dir_recursive(dir: &Path, file_count: &mut usize, total_size: &mut u64) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            scan_dir_recursive(&path, file_count, total_size)?;
        } else if path.is_file() {
            *file_count += 1;
            if let Ok(meta) = entry.metadata() {
                *total_size += meta.len();
            }
        }
    }
    Ok(())
}

fn cpath(path: &Path) -> colored::ColoredString {
    path.display().to_string().cyan()
}

// ---------------------------------------------------------------------------
// User config file operations (using toml::Value)
// ---------------------------------------------------------------------------

fn user_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("mgpm")
        .join("config.toml")
}

fn ensure_user_config_dir() -> io::Result<PathBuf> {
    let path = user_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(path)
}

fn read_user_toml() -> Result<toml::Value, String> {
    let path = user_config_path();
    if !path.exists() {
        return Ok(toml::Value::Table(toml::value::Table::new()));
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("failed to read config: {}", e))?;
    toml::from_str(&content).map_err(|e| format!("failed to parse config: {}", e))
}

fn write_user_toml(value: &toml::Value) -> Result<(), String> {
    let path =
        ensure_user_config_dir().map_err(|e| format!("failed to create config dir: {}", e))?;
    let content =
        toml::to_string(value).map_err(|e| format!("failed to serialize config: {}", e))?;
    std::fs::write(&path, content).map_err(|e| format!("failed to write config: {}", e))
}

fn get_nested<'a>(value: &'a toml::Value, parts: &[&str]) -> Option<&'a toml::Value> {
    let mut current = value;
    for part in parts {
        match current {
            toml::Value::Table(t) => current = t.get(*part)?,
            _ => return None,
        }
    }
    Some(current)
}

fn set_nested(value: &mut toml::Value, parts: &[&str], val: toml::Value) -> Result<(), String> {
    if parts.is_empty() {
        return Err("empty key path".to_string());
    }
    if parts.len() == 1 {
        let table = value.as_table_mut().ok_or("root is not a table")?;
        table.insert(parts[0].to_string(), val);
        return Ok(());
    }
    let table = value.as_table_mut().ok_or("root is not a table")?;
    if !table.contains_key(parts[0]) {
        table.insert(
            parts[0].to_string(),
            toml::Value::Table(toml::value::Table::new()),
        );
    }
    let next = table
        .get_mut(parts[0])
        .ok_or_else(|| format!("missing key '{}'", parts[0]))?;
    set_nested(next, &parts[1..], val)
}

fn delete_nested(value: &mut toml::Value, parts: &[&str]) -> Result<toml::Value, String> {
    if parts.is_empty() {
        return Err("empty key path".to_string());
    }
    if parts.len() == 1 {
        let table = value.as_table_mut().ok_or("not a table")?;
        return table
            .remove(parts[0])
            .ok_or_else(|| format!("key '{}' not found", parts[0]));
    }
    let table = value.as_table_mut().ok_or("not a table")?;
    let next = table
        .get_mut(parts[0])
        .ok_or_else(|| format!("key '{}' not found", parts[0]))?;
    delete_nested(next, &parts[1..])
}

fn config_get_value(key: &str) -> Result<String, String> {
    let doc = read_user_toml()?;
    let parts: Vec<&str> = key.split('.').collect();
    let val = get_nested(&doc, &parts).ok_or_else(|| format!("key '{}' not found", key))?;
    Ok(val.to_string())
}

fn config_set_value(key: &str, value: &str) -> Result<String, String> {
    let mut doc = read_user_toml()?;
    let parts: Vec<&str> = key.split('.').collect();
    set_nested(&mut doc, &parts, toml::Value::String(value.to_string()))?;
    write_user_toml(&doc)?;
    Ok(format!("Set {} = {}", key, value))
}

fn config_delete_value(key: &str) -> Result<String, String> {
    let mut doc = read_user_toml()?;
    let parts: Vec<&str> = key.split('.').collect();
    delete_nested(&mut doc, &parts)?;
    write_user_toml(&doc)?;
    Ok(format!("Deleted {}", key))
}

fn config_list_values() -> Result<String, String> {
    let doc = read_user_toml()?;
    let path = user_config_path();
    let header = format!("Configuration ({})", cpath(&path));
    if doc.as_table().map_or(true, |t| t.is_empty()) {
        Ok(format!("{}\n  (empty)", header))
    } else {
        let body = toml::to_string(&doc).map_err(|e| format!("serialize error: {}", e))?;
        Ok(format!("{}\n{}", header, body.trim()))
    }
}

// ---------------------------------------------------------------------------
// Init scaffold
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Run / Exec helpers
// ---------------------------------------------------------------------------

fn build_node_bin_path() -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    let node_bin = format!("{}/node_modules/.bin", cwd.display());
    match std::env::var("PATH") {
        Ok(existing) => format!("{}:{}", node_bin, existing),
        Err(_) => format!("{}:/usr/bin:/bin:/usr/local/bin", node_bin),
    }
}

fn run_script(script_name: &str) -> Result<(), String> {
    let pkg = load_package_json(Path::new("package.json"))?;
    let scripts = pkg
        .get("scripts")
        .and_then(|s| s.as_object())
        .ok_or_else(|| "no 'scripts' field in package.json".to_string())?;
    let script_cmd = scripts
        .get(script_name)
        .and_then(|s| s.as_str())
        .ok_or_else(|| format!("script '{}' not found in package.json", script_name))?;

    let parts: Vec<&str> = script_cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Err(format!("script '{}' resolves to an empty command", script_name));
    }

    let path = build_node_bin_path();
    let status = ProcessCmd::new(parts[0])
        .args(&parts[1..])
        .env("PATH", &path)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| format!("failed to run script: {}", e))?;

    if status.success() {
        println!(
            "{} Script '{}' completed",
            "[OK]".green().bold(),
            script_name
        );
        Ok(())
    } else {
        Err(format!(
            "Script '{}' failed with exit code {:?}",
            script_name,
            status.code()
        ))
    }
}

fn exec_command(cmd: &str, args: &[String]) -> Result<(), String> {
    let bin_path = PathBuf::from("node_modules").join(".bin").join(cmd);
    let executable = if bin_path.exists() {
        bin_path
    } else {
        PathBuf::from(cmd)
    };

    let path = build_node_bin_path();
    let status = ProcessCmd::new(&executable)
        .args(args)
        .env("PATH", &path)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| format!("failed to execute '{}': {}", cmd, e))?;

    if status.success() {
        println!("{} Command '{}' completed", "[OK]".green().bold(), cmd);
        Ok(())
    } else {
        Err(format!(
            "Command '{}' failed with exit code {:?}",
            cmd,
            status.code()
        ))
    }
}

// ---------------------------------------------------------------------------
// Workspace helpers (recursive commands, change detection)
// ---------------------------------------------------------------------------

fn is_workspace_context(recursive: bool, filter: &[String], since: Option<&str>) -> bool {
    recursive || !filter.is_empty() || since.is_some()
}

fn resolve_workspace_members<'a>(
    workspace: &'a mgpm_workspace::Workspace,
    recursive: bool,
    filter: &[String],
    since: Option<&str>,
) -> Result<Vec<&'a mgpm_workspace::WorkspaceMember>, String> {
    let mut members: Vec<&mgpm_workspace::WorkspaceMember> = if recursive {
        workspace
            .topological_sort()
            .map_err(|e| format!("topological sort failed: {e}"))?
    } else {
        workspace.members().iter().collect()
    };

    if !filter.is_empty() {
        members.retain(|m| {
            let path_str = m.path.to_string_lossy();
            filter
                .iter()
                .any(|f| m.name.contains(f.as_str()) || path_str.contains(f.as_str()))
        });
    }

    if let Some(ref_) = since {
        let changed = workspace
            .changed_since(ref_)
            .map_err(|e| format!("change detection failed: {e}"))?;
        let changed_set: std::collections::HashSet<&str> =
            changed.iter().map(|m| m.name.as_str()).collect();
        members.retain(|m| changed_set.contains(m.name.as_str()));
    }

    Ok(members)
}

fn run_on_members(
    members: &[&mgpm_workspace::WorkspaceMember],
    command_label: &str,
    fail_fast: bool,
    mut f: impl FnMut(&mgpm_workspace::WorkspaceMember) -> Result<(), String>,
) -> Result<(), String> {
    let total = members.len();
    if total == 0 {
        eprintln!(
            "  {} No workspace members to process",
            "[WARN]".yellow().bold()
        );
        return Ok(());
    }

    let mut succeeded = 0usize;

    for member in members {
        match f(member) {
            Ok(()) => succeeded += 1,
            Err(e) => {
                eprintln!("  {} [{}] {}", "[FAIL]".red().bold(), member.name, e.red());
                if fail_fast {
                    return Err(format!("[{}] {} failed: {}", member.name, command_label, e));
                }
            }
        }
    }

    println!(
        "{} Ran {} on {}/{} workspace members",
        "[DONE]".green().bold(),
        command_label,
        succeeded,
        total,
    );

    if succeeded < total {
        Err(format!("{} member(s) failed", total - succeeded))
    } else {
        Ok(())
    }
}

fn cmd_run_recursive(
    members: &[&mgpm_workspace::WorkspaceMember],
    script: &str,
    args: &[String],
    fail_fast: bool,
) -> Result<(), String> {
    run_on_members(members, "run", fail_fast, |member| {
        println!("[{}] Running '{}'...", member.name.green(), script);

        let pkg_path = member.path.join("package.json");
        let pkg = load_package_json(&pkg_path)?;
        let scripts = pkg
            .get("scripts")
            .and_then(|s| s.as_object())
            .ok_or_else(|| format!("no 'scripts' field in {}", pkg_path.display()))?;
        let script_cmd = scripts
            .get(script)
            .and_then(|s| s.as_str())
            .ok_or_else(|| format!("script '{}' not found in {}", script, pkg_path.display()))?;

        let parts: Vec<&str> = script_cmd.split_whitespace().collect();
        if parts.is_empty() {
            return Err(format!("script '{}' resolves to an empty command", script));
        }

        let path = format!(
            "{}:{}",
            member.path.join("node_modules").join(".bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );

        let output = std::process::Command::new(parts[0])
            .args(&parts[1..])
            .args(args)
            .current_dir(&member.path)
            .env("PATH", &path)
            .output()
            .map_err(|e| format!("failed to execute: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() {
            print!("{stdout}");
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                eprintln!("{}", stderr.red());
            }
            return Err(format!("exit code: {:?}", output.status.code()));
        }
        Ok(())
    })
}

fn cmd_exec_recursive(
    members: &[&mgpm_workspace::WorkspaceMember],
    command: &str,
    args: &[String],
    fail_fast: bool,
) -> Result<(), String> {
    run_on_members(members, "exec", fail_fast, |member| {
        println!("[{}] Executing '{}'...", member.name.green(), command);
        let bin_path = member.path.join("node_modules").join(".bin").join(command);
        let executable = if bin_path.exists() {
            bin_path
        } else {
            PathBuf::from(command)
        };

        let path = format!(
            "{}:{}",
            member.path.join("node_modules").join(".bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );

        let output = std::process::Command::new(&executable)
            .args(args)
            .current_dir(&member.path)
            .env("PATH", &path)
            .output()
            .map_err(|e| format!("failed to execute command: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() {
            print!("{stdout}");
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                eprintln!("{}", stderr.red());
            }
            return Err(format!("exit code: {:?}", output.status.code()));
        }
        Ok(())
    })
}

async fn cmd_install_recursive(
    config: &MgpmConfig,
    recursive: bool,
    filter: &[String],
    since: Option<&str>,
    fail_fast: bool,
    offline: bool,
    production: bool,
    hoist: bool,
    profile: bool,
    timings: bool,
) -> Result<(), String> {
    let project_root = std::env::current_dir().map_err(|e| e.to_string())?;
    let workspace = mgpm_workspace::Workspace::discover(&project_root)
        .map_err(|e| format!("workspace not found: {e}"))?;
    let members = resolve_workspace_members(&workspace, recursive, filter, since)?;
    let total = members.len();
    if total == 0 {
        eprintln!(
            "  {} No workspace members to process",
            "[WARN]".yellow().bold()
        );
        return Ok(());
    }

    let mut succeeded = 0usize;

    for member in &members {
        println!("[{}] Installing dependencies...", member.name.green());
        let prev = std::env::current_dir().ok();
        std::env::set_current_dir(&member.path)
            .map_err(|e| format!("cannot enter {}: {e}", member.path.display()))?;

        let result = cmd_install(config, offline, production, hoist, profile, timings).await;

        if let Some(p) = prev {
            std::env::set_current_dir(p).ok();
        }

        match result {
            Ok(()) => succeeded += 1,
            Err(e) => {
                eprintln!("  {} [{}] {}", "[FAIL]".red().bold(), member.name, e.red());
                if fail_fast {
                    return Err(format!("[{}] install failed: {}", member.name, e));
                }
            }
        }
    }

    println!(
        "{} Ran install on {}/{} workspace members",
        "[DONE]".green().bold(),
        succeeded,
        total,
    );

    if succeeded < total {
        Err(format!("{} member(s) failed", total - succeeded))
    } else {
        Ok(())
    }
}

async fn cmd_add_recursive(
    _config: &MgpmConfig,
    recursive: bool,
    filter: &[String],
    since: Option<&str>,
    fail_fast: bool,
    packages: &[String],
    dev: bool,
    peer: bool,
    optional: bool,
    exact: bool,
) -> Result<(), String> {
    let project_root = std::env::current_dir().map_err(|e| e.to_string())?;
    let workspace = mgpm_workspace::Workspace::discover(&project_root)
        .map_err(|e| format!("workspace not found: {e}"))?;
    let members = resolve_workspace_members(&workspace, recursive, filter, since)?;
    let total = members.len();
    if total == 0 {
        eprintln!(
            "  {} No workspace members to process",
            "[WARN]".yellow().bold()
        );
        return Ok(());
    }

    let mut succeeded = 0usize;

    for member in &members {
        println!("[{}] Adding packages...", member.name.green());
        let prev = std::env::current_dir().ok();
        std::env::set_current_dir(&member.path)
            .map_err(|e| format!("cannot enter {}: {e}", member.path.display()))?;

        let result = cmd_add(packages.to_vec(), dev, peer, optional, exact).await;

        if let Some(p) = prev {
            std::env::set_current_dir(p).ok();
        }

        match result {
            Ok(()) => succeeded += 1,
            Err(e) => {
                eprintln!("  {} [{}] {}", "[FAIL]".red().bold(), member.name, e.red());
                if fail_fast {
                    return Err(format!("[{}] add failed: {}", member.name, e));
                }
            }
        }
    }

    println!(
        "{} Ran add on {}/{} workspace members",
        "[DONE]".green().bold(),
        succeeded,
        total,
    );

    if succeeded < total {
        Err(format!("{} member(s) failed", total - succeeded))
    } else {
        Ok(())
    }
}

async fn cmd_remove_recursive(
    _config: &MgpmConfig,
    recursive: bool,
    filter: &[String],
    since: Option<&str>,
    fail_fast: bool,
    packages: &[String],
) -> Result<(), String> {
    let project_root = std::env::current_dir().map_err(|e| e.to_string())?;
    let workspace = mgpm_workspace::Workspace::discover(&project_root)
        .map_err(|e| format!("workspace not found: {e}"))?;
    let members = resolve_workspace_members(&workspace, recursive, filter, since)?;
    let total = members.len();
    if total == 0 {
        eprintln!(
            "  {} No workspace members to process",
            "[WARN]".yellow().bold()
        );
        return Ok(());
    }

    let mut succeeded = 0usize;

    for member in &members {
        println!("[{}] Removing packages...", member.name.green());
        let prev = std::env::current_dir().ok();
        std::env::set_current_dir(&member.path)
            .map_err(|e| format!("cannot enter {}: {e}", member.path.display()))?;

        let result = cmd_remove(packages.to_vec()).await;

        if let Some(p) = prev {
            std::env::set_current_dir(p).ok();
        }

        match result {
            Ok(()) => succeeded += 1,
            Err(e) => {
                eprintln!("  {} [{}] {}", "[FAIL]".red().bold(), member.name, e.red());
                if fail_fast {
                    return Err(format!("[{}] remove failed: {}", member.name, e));
                }
            }
        }
    }

    println!(
        "{} Ran remove on {}/{} workspace members",
        "[DONE]".green().bold(),
        succeeded,
        total,
    );

    if succeeded < total {
        Err(format!("{} member(s) failed", total - succeeded))
    } else {
        Ok(())
    }
}

async fn cmd_update_recursive(
    _config: &MgpmConfig,
    recursive: bool,
    filter: &[String],
    since: Option<&str>,
    fail_fast: bool,
    latest: bool,
) -> Result<(), String> {
    let project_root = std::env::current_dir().map_err(|e| e.to_string())?;
    let workspace = mgpm_workspace::Workspace::discover(&project_root)
        .map_err(|e| format!("workspace not found: {e}"))?;
    let members = resolve_workspace_members(&workspace, recursive, filter, since)?;
    let total = members.len();
    if total == 0 {
        eprintln!(
            "  {} No workspace members to process",
            "[WARN]".yellow().bold()
        );
        return Ok(());
    }

    let mut succeeded = 0usize;

    for member in &members {
        println!("[{}] Updating packages...", member.name.green());
        let prev = std::env::current_dir().ok();
        std::env::set_current_dir(&member.path)
            .map_err(|e| format!("cannot enter {}: {e}", member.path.display()))?;

        let result = cmd_update(latest).await;

        if let Some(p) = prev {
            std::env::set_current_dir(p).ok();
        }

        match result {
            Ok(()) => succeeded += 1,
            Err(e) => {
                eprintln!("  {} [{}] {}", "[FAIL]".red().bold(), member.name, e.red());
                if fail_fast {
                    return Err(format!("[{}] update failed: {}", member.name, e));
                }
            }
        }
    }

    println!(
        "{} Ran update on {}/{} workspace members",
        "[DONE]".green().bold(),
        succeeded,
        total,
    );

    if succeeded < total {
        Err(format!("{} member(s) failed", total - succeeded))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

async fn cmd_install(
    config: &MgpmConfig,
    offline: bool,
    production: bool,
    hoist: bool,
    profile: bool,
    timings: bool,
) -> Result<(), String> {
    let project_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    for warning in auth::check_auth_security(&project_dir) {
        eprintln!("  {} {}", "[WARN]".yellow().bold(), warning.yellow());
    }

    for reg in &config.registries {
        if let Some(w) = auth::check_url_for_credentials(&reg.url) {
            eprintln!("  {} {}", "[WARN]".yellow().bold(), w.yellow());
        }
        if let Some(w) = auth::check_url_for_query_token(&reg.url) {
            eprintln!("  {} {}", "[WARN]".yellow().bold(), w.yellow());
        }
    }

    let mut profiler = PhaseProfiler::new();

    eprintln!(
        "{} {}",
        "[INFO]".cyan().bold(),
        format!(
            "Installing dependencies (offline={}, production={}, hoist={})...",
            offline, production, hoist
        )
        .cyan()
    );

    let pkg = load_package_json(Path::new("package.json"))?;

    profiler.start("resolve");

    let wanted_deps = {
        let deps = pkg.get("dependencies").and_then(|d| d.as_object());
        let dev_deps = pkg.get("devDependencies").and_then(|d| d.as_object());

        let mut wanted: Vec<mgpm_lockfile::WantedDependency> = Vec::new();
        if let Some(d) = deps {
            for (name, version) in d {
                let v = version.as_str().unwrap_or("*");
                wanted.push(mgpm_lockfile::WantedDependency {
                    name: mgpm_core::PackageName::new(name)
                        .map_err(|e| format!("invalid package name '{}': {}", name, e))?,
                    version_req: v.to_string(),
                    dev: false,
                    optional: false,
                });
            }
        }
        if !production {
            if let Some(d) = dev_deps {
                for (name, version) in d {
                    let v = version.as_str().unwrap_or("*");
                    wanted.push(mgpm_lockfile::WantedDependency {
                        name: mgpm_core::PackageName::new(name)
                            .map_err(|e| format!("invalid package name '{}': {}", name, e))?,
                        version_req: v.to_string(),
                        dev: true,
                        optional: false,
                    });
                }
            }
        }
        wanted
    };

    if wanted_deps.is_empty() {
        eprintln!("  {} No dependencies to install", "[WARN]".yellow().bold());
        return Ok(());
    }

    let lockfile = if Path::new("mgpm.lock").exists() {
        mgpm_lockfile::text::read_text(Path::new("mgpm.lock"))
            .map_err(|e| format!("failed to read lockfile: {}", e))?
    } else {
        let mut lf = mgpm_lockfile::Lockfile::new(1, "https://registry.npmjs.org");
        for dep in &wanted_deps {
            let pkg = mgpm_lockfile::LockfilePackage {
                id: format!("{}@{}", dep.name.as_str(), dep.version_req),
                name: dep.name.as_str().to_string(),
                version: dep.version_req.clone(),
                resolution: mgpm_lockfile::PackageResolution {
                    r#type: "registry".to_string(),
                    url: format!(
                        "https://registry.npmjs.org/{}/-/{}-{}.tgz",
                        dep.name.as_str(),
                        dep.name.as_str(),
                        dep.version_req
                    ),
                    registry: Some("npm".to_string()),
                },
                integrity: None,
            };
            lf.add_package(pkg);
        }
        lf.sort_packages();
        lf.compute_content_hash();
        lf.update_timestamp();
        lf
    };

    profiler.end("resolve");

    // Dependency confusion check
    {
        let workspace_packages: Vec<String> = if let Ok(root) = std::env::current_dir() {
            if let Ok(ws) = mgpm_workspace::Workspace::discover(&root) {
                ws.members().iter().map(|m| m.name.clone()).collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let dep_infos: Vec<mgpm_resolver::solver::DepInfo> = wanted_deps
            .iter()
            .map(|d| mgpm_resolver::solver::DepInfo {
                name: d.name.as_str().to_string(),
                version: Some(d.version_req.clone()),
                registry: config.registries.first().map(|r| r.url.clone()),
            })
            .collect();

        let warnings = mgpm_resolver::solver::check_dependency_confusion(
            &workspace_packages,
            &dep_infos,
            &config.scoped_registries,
            &config.trusted_registries,
        );
        for w in &warnings {
            eprintln!("  {} {}", "[WARN]".yellow().bold(), w.yellow());
        }
    }

    let store_path = config.store.store_path();
    let install_opts = RealInstallOptions {
        concurrency: config.install.concurrency,
        retries: config.install.retries,
        retry_delay_ms: 1000,
        store_path,
        virtual_store_path: PathBuf::from(".mgpm").join("virtual_store"),
        hoisted_node_modules: hoist || config.install.hoist,
        hoist_pattern: config.install.hoist_pattern.clone(),
        offline: offline || config.cli.dry_run,
        dry_run: config.cli.dry_run,
        project_root: std::env::current_dir().unwrap_or_default(),
        sqlite_path: dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".mgpm")
            .join("mgpm.db"),
        jsonl_log: false,
    };

    let (tx, mut rx) = mpsc::channel::<mgpm_installer::installer::InstallProgress>(256);

    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let phase_str = format!("{:?}", progress.phase);
            eprintln!(
                "  {} {} {}",
                "[INSTALL]".cyan(),
                progress.package.cyan(),
                phase_str.dimmed()
            );
        }
    });

    profiler.start("fetch");

    let installer = Installer::new(install_opts, tx)
        .map_err(|e| format!("failed to create installer: {}", e))?;
    let result = installer.install_lockfile(&lockfile).await;

    profiler.end("fetch");
    profiler.start("extract");
    profiler.end("extract");
    profiler.start("link");
    profiler.end("link");

    eprintln!(
        "{} {}",
        "[OK]".green().bold(),
        format!(
            "Installed: {} succeeded, {} failed, {} skipped",
            result.succeeded, result.failed, result.skipped
        )
        .green()
    );

    if result.failed > 0 {
        for err in &result.errors {
            eprintln!("  {} {}", "[ERROR]".red().bold(), format!("{}", err).red());
        }
    }

    if profile {
        eprintln!("{}", profiler.report());
    }
    if timings {
        let json = profiler.report_json();
        let timings_dir = PathBuf::from(".mgpm").join("timings");
        std::fs::create_dir_all(&timings_dir)
            .map_err(|e| format!("failed to create timings dir: {e}"))?;

        let latest_path = timings_dir.join("latest.json");
        std::fs::write(&latest_path, &json)
            .map_err(|e| format!("failed to write timings: {e}"))?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let hist_path = timings_dir.join(format!("{}.json", timestamp));
        std::fs::write(&hist_path, &json)
            .map_err(|e| format!("failed to write historical timings: {e}"))?;

        println!("{}", json);
    }

    Ok(())
}

async fn cmd_add(
    packages: Vec<String>,
    dev: bool,
    peer: bool,
    optional: bool,
    exact: bool,
) -> Result<(), String> {
    for pkg in &packages {
        eprintln!(
            "{} {} (dev={}, peer={}, optional={}, exact={})",
            "[INFO]".cyan().bold(),
            format!("Adding '{}'...", pkg).cyan(),
            dev,
            peer,
            optional,
            exact,
        );
    }
    Ok(())
}

async fn cmd_remove(packages: Vec<String>) -> Result<(), String> {
    for pkg in &packages {
        eprintln!(
            "{} {}",
            "[INFO]".cyan().bold(),
            format!("Removing '{}'...", pkg).cyan()
        );
    }
    Ok(())
}

async fn cmd_update(latest: bool) -> Result<(), String> {
    eprintln!(
        "{} {}",
        "[INFO]".cyan().bold(),
        format!("Updating packages (latest={})...", latest).cyan()
    );
    Ok(())
}

fn cmd_store_prune(config: &MgpmConfig) -> Result<(), String> {
    eprintln!("{} {}", "[INFO]".cyan().bold(), "Pruning store...".cyan());

    let store_path = config.store.store_path();
    if !store_path.exists() {
        eprintln!(
            "  {} Store directory not found at {}",
            "[WARN]".yellow().bold(),
            cpath(&store_path).yellow()
        );
        return Ok(());
    }

    let store =
        ContentStore::new(store_path).map_err(|e| format!("failed to open store: {}", e))?;
    let removed = store.gc().map_err(|e| format!("gc failed: {}", e))?;

    eprintln!(
        "{} {} {} {}",
        "[OK]".green().bold(),
        "Removed".green(),
        format!("{}", removed).green().bold(),
        "unreferenced files".green()
    );
    Ok(())
}

fn cmd_store_status(config: &MgpmConfig) -> Result<(), String> {
    let store_path = config.store.store_path();
    eprintln!("{} {}", "[INFO]".cyan().bold(), "Store status:".cyan());
    eprintln!("  Path: {}", cpath(&store_path));

    if !store_path.exists() {
        eprintln!(
            "  {} Store directory does not exist",
            "[WARN]".yellow().bold()
        );
        eprintln!("  {} packages: 0", "[INFO]".cyan());
        eprintln!("  {} used: {}", "[INFO]".cyan(), format_size(0).cyan());
        return Ok(());
    }

    match scan_store_size(&store_path) {
        Ok((count, size)) => {
            eprintln!(
                "  {} packages: {}",
                "[INFO]".cyan(),
                format!("{}", count).green().bold()
            );
            eprintln!(
                "  {} used: {}",
                "[INFO]".cyan(),
                format_size(size).green().bold()
            );

            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            let sqlite_path = home.join(".mgpm").join("mgpm.db");
            if sqlite_path.exists() {
                if let Ok(conn) = rusqlite::Connection::open(&sqlite_path) {
                    if let Ok(count) = conn.query_row("SELECT COUNT(*) FROM refcounts", [], |row| {
                        row.get::<_, i64>(0)
                    }) {
                        eprintln!(
                            "  {} referenced packages: {}",
                            "[INFO]".cyan(),
                            format!("{}", count).green().bold()
                        );
                    }
                }
            }
        }
        Err(e) => return Err(format!("failed to scan store: {}", e)),
    }

    Ok(())
}

fn cmd_init() -> Result<(), String> {
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

// ---------------------------------------------------------------------------
// .npmrc parsing
// ---------------------------------------------------------------------------

fn parse_npmrc(path: &Path) -> Result<Vec<(String, String)>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let mut entries = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim().to_string();
            let value = line[eq_pos + 1..].trim().to_string();
            entries.push((key, value));
        }
    }
    Ok(entries)
}

fn apply_npmrc_config(config: &mut MgpmConfig) {
    let project_npmrc = Path::new(".npmrc");
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let user_npmrc = home.join(".npmrc");

    for npmrc_path in &[project_npmrc, &user_npmrc] {
        if !npmrc_path.exists() {
            continue;
        }
        let entries = match parse_npmrc(npmrc_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for (key, value) in &entries {
            // Scoped registry: @scope:registry=https://...
            if key.ends_with(":registry") && key.starts_with('@') {
                if let Some(reg) = config.registries.first_mut() {
                    reg.url.clone_from(value);
                }
            }
            // _authToken
            if key.ends_with(":_authToken") {
                if let Some(reg) = config.registries.first_mut() {
                    reg.token = Some(value.clone());
                }
            }
            // registry= (top-level)
            if key == "registry" {
                if let Some(reg) = config.registries.first_mut() {
                    reg.url.clone_from(value);
                }
            }
            // always-auth
            if key == "always-auth" && value == "true" {
                if let Some(reg) = config.registries.first_mut() {
                    reg.always_auth = true;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Audit command
// ---------------------------------------------------------------------------

async fn cmd_audit(
    config: &MgpmConfig,
    json: bool,
    severity: Option<String>,
    remote: bool,
) -> Result<(), String> {
    if let Ok(project_dir) = std::env::current_dir() {
        for warning in auth::check_auth_security(&project_dir) {
            eprintln!("  {} {}", "[WARN]".yellow().bold(), warning.yellow());
        }
    }

    for reg in &config.registries {
        if let Some(w) = auth::check_url_for_credentials(&reg.url) {
            eprintln!("  {} {}", "[WARN]".yellow().bold(), w.yellow());
        }
        if let Some(w) = auth::check_url_for_query_token(&reg.url) {
            eprintln!("  {} {}", "[WARN]".yellow().bold(), w.yellow());
        }
    }

    let lockfile = if Path::new("mgpm.lock").exists() {
        mgpm_lockfile::text::read_text(Path::new("mgpm.lock"))
            .map_err(|e| format!("failed to read lockfile: {e}"))?
    } else {
        return Err("no mgpm.lock found".to_string());
    };

    let mut db = AdvisoryDb::new();
    if remote {
        match db.fetch_remote().await {
            Err(e) => eprintln!("  {} Failed to fetch remote advisories: {}", "[WARN]".yellow().bold(), e),
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

// ---------------------------------------------------------------------------
// Verify command
// ---------------------------------------------------------------------------

fn cmd_verify(config: &MgpmConfig) -> Result<(), String> {
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

    let store = ContentStore::new(store_path)
        .map_err(|e| format!("failed to open store: {e}"))?;

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
    let bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        b64,
    )
    .ok()?;
    Some(hex::encode(bytes))
}

// ---------------------------------------------------------------------------
// Verify --deep command
// ---------------------------------------------------------------------------

fn cmd_verify_deep(config: &MgpmConfig) -> Result<(), String> {
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

    let store = ContentStore::new(store_path)
        .map_err(|e| format!("failed to open store: {e}"))?;

    // Build map of installed packages from node_modules
    let nm_path = Path::new("node_modules");
    let mut installed: std::collections::HashMap<String, PathBuf> = std::collections::HashMap::new();
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
                                installed.insert(format!("{}/{}", name, s.file_name().to_string_lossy()), sp);
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
                    println!("  {} {}@{} (store integrity mismatch)", "✗".red(), pkg.name.red(), pkg.version.red());
                    fail += 1;
                }
            }
            None => {
                println!("  {} {}@{} (not in node_modules)", "✗".red(), pkg.name.red(), pkg.version.red());
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
        Err(format!("{} store mismatch(es), {} missing from node_modules", fail, miss))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Import command
// ---------------------------------------------------------------------------

fn cmd_import(source: &str, format: &str) -> Result<(), String> {
    let path = Path::new(source);
    if !path.exists() {
        return Err(format!("source file not found: {}", source));
    }

    let lf_format = if format == "auto" {
        detect_format(path)
            .ok_or_else(|| {
                format!(
                    "unable to detect lockfile format from '{}'",
                    path.display()
                )
            })?
    } else {
        match format {
            "npm" => LockfileFormat::Npm,
            "yarn" => LockfileFormat::Yarn,
            "pnpm" => LockfileFormat::Pnpm,
            "bun" => LockfileFormat::Bun,
            other => return Err(format!("unsupported format: {other}")),
        }
    };

    let lockfile = import_lockfile(path, lf_format)?;
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

// ---------------------------------------------------------------------------
// Export command
// ---------------------------------------------------------------------------

fn cmd_export(output: &str) -> Result<(), String> {
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
        entry.insert(
            "dev".to_string(),
            serde_json::Value::Bool(false),
        );
        packages_map.insert(node_key, serde_json::Value::Object(entry));
    }

    let export = serde_json::json!({
        "name": "mgpm-export",
        "lockfileVersion": 3,
        "requires": true,
        "packages": packages_map,
    });

    let content = serde_json::to_string_pretty(&export)
        .map_err(|e| format!("failed to serialize: {e}"))?;
    std::fs::write(output, &content)
        .map_err(|e| format!("failed to write {}: {}", output, e))?;

    println!(
        "{} Exported {} packages to {}",
        "[OK]".green().bold(),
        lockfile.packages.len(),
        output,
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), String> {
    mgpm_core::init_tracing(Default::default());

    let cli = Cli::parse();
    let rec = cli.recursive;
    let flt = cli.filter.clone();
    let sinc = cli.since.clone();
    let ff = cli.fail_fast;
    let profiling = cli.profile;
    let timings = cli.timings;
    let (config, _user_config_path) = load_config();

    let result: Result<(), String> = match cli.command {
        CliCommand::Install {
            offline,
            frozen_lockfile: _,
            production,
            hoist,
            sandbox,
        } => {
            let _guard: Option<sandbox::SandboxGuard> = if sandbox {
                let project_dir = std::env::current_dir().map_err(|e| e.to_string())?;
                Some(sandbox::enable_sandbox(&project_dir)?)
            } else {
                None
            };
            if is_workspace_context(rec, &flt, sinc.as_deref()) {
                cmd_install_recursive(
                    &config,
                    rec,
                    &flt,
                    sinc.as_deref(),
                    ff,
                    offline,
                    production,
                    hoist,
                    profiling,
                    timings,
                )
                .await
            } else {
                cmd_install(&config, offline, production, hoist, profiling, timings).await
            }
        }
        CliCommand::Add {
            ref packages,
            dev,
            peer,
            optional,
            exact,
        } => {
            if is_workspace_context(rec, &flt, sinc.as_deref()) {
                cmd_add_recursive(
                    &config,
                    rec,
                    &flt,
                    sinc.as_deref(),
                    ff,
                    packages,
                    dev,
                    peer,
                    optional,
                    exact,
                )
                .await
            } else {
                cmd_add(packages.clone(), dev, peer, optional, exact).await
            }
        }
        CliCommand::Remove { ref packages } => {
            if is_workspace_context(rec, &flt, sinc.as_deref()) {
                cmd_remove_recursive(&config, rec, &flt, sinc.as_deref(), ff, packages).await
            } else {
                cmd_remove(packages.clone()).await
            }
        }
        CliCommand::Update { latest } => {
            if is_workspace_context(rec, &flt, sinc.as_deref()) {
                cmd_update_recursive(&config, rec, &flt, sinc.as_deref(), ff, latest).await
            } else {
                cmd_update(latest).await
            }
        }
        CliCommand::Run {
            ref script,
            ref args,
        } => {
            if is_workspace_context(rec, &flt, sinc.as_deref()) {
                let s = script.clone();
                let a = args.clone();
                (|| -> Result<(), String> {
                    let project_root = std::env::current_dir().map_err(|e| e.to_string())?;
                    let workspace = mgpm_workspace::Workspace::discover(&project_root)
                        .map_err(|e| format!("workspace not found: {e}"))?;
                    let members =
                        resolve_workspace_members(&workspace, rec, &flt, sinc.as_deref())?;
                    cmd_run_recursive(&members, &s, &a, ff)
                })()
            } else {
                run_script(script)
            }
        }
        CliCommand::Exec {
            ref command,
            ref args,
        } => {
            if is_workspace_context(rec, &flt, sinc.as_deref()) {
                let c = command.clone();
                let a = args.clone();
                (|| -> Result<(), String> {
                    let project_root = std::env::current_dir().map_err(|e| e.to_string())?;
                    let workspace = mgpm_workspace::Workspace::discover(&project_root)
                        .map_err(|e| format!("workspace not found: {e}"))?;
                    let members =
                        resolve_workspace_members(&workspace, rec, &flt, sinc.as_deref())?;
                    cmd_exec_recursive(&members, &c, &a, ff)
                })()
            } else {
                exec_command(command, args)
            }
        }
        CliCommand::Store { command: store_cmd } => match store_cmd {
            StoreCommand::Prune => cmd_store_prune(&config),
            StoreCommand::Status => cmd_store_status(&config),
            StoreCommand::CompletionsCache { command: cache_cmd } => {
                cmd_completions_cache(cache_cmd)
            }
        },
        CliCommand::Config {
            command: config_cmd,
        } => handle_config_command(config_cmd),
        CliCommand::Init => cmd_init(),
        CliCommand::Completions { shell } => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
            Ok(())
        }
        CliCommand::Lockfile { command: lockfile_cmd } => cmd_lockfile(lockfile_cmd),
        CliCommand::Daemon { command: daemon_cmd } => cmd_daemon(daemon_cmd),
        CliCommand::Audit { command } => match command {
            AuditCommand::Run { json, severity, remote } => cmd_audit(&config, json, severity, remote).await,
            AuditCommand::Update { force } => tuf::update_advisories(force).await,
        },
        CliCommand::Verify { deep } => {
            if deep {
                cmd_verify_deep(&config)
            } else {
                cmd_verify(&config)
            }
        }
        CliCommand::Import { ref source, ref format } => cmd_import(source, format),
        CliCommand::Export { ref output } => cmd_export(output),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "[ERROR]".red().bold(), e.red());
        std::process::exit(1);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Lockfile commands
// ---------------------------------------------------------------------------

fn cmd_lockfile(command: LockfileSubcommand) -> Result<(), String> {
    match command {
        LockfileSubcommand::Upgrade { to } => cmd_lockfile_upgrade(to),
        LockfileSubcommand::Validate => cmd_lockfile_validate(),
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

// ---------------------------------------------------------------------------
// Daemon commands
// ---------------------------------------------------------------------------

fn daemon_pid_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mgpm")
        .join("daemon.pid")
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let output = std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .output();
        matches!(output, Ok(o) if o.status.success())
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn cmd_daemon(command: DaemonSubcommand) -> Result<(), String> {
    match command {
        DaemonSubcommand::Start => cmd_daemon_start(),
        DaemonSubcommand::Stop => cmd_daemon_stop(),
        DaemonSubcommand::Status => cmd_daemon_status(),
    }
}

fn cmd_daemon_start() -> Result<(), String> {
    let pid_path = daemon_pid_path();
    if pid_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if is_process_running(pid) {
                    return Err(format!("daemon already running (PID: {})", pid));
                }
            }
        }
        std::fs::remove_file(&pid_path).ok();
    }

    if let Some(parent) = pid_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create daemon directory: {e}"))?;
    }

    let pid = std::process::id();
    std::fs::write(&pid_path, pid.to_string())
        .map_err(|e| format!("failed to write PID file: {e}"))?;

    println!("Daemon started (PID: {})", pid);

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let store_path = home.join(".mgpm").join("store");
    let _store = mgpm_store::ContentStore::new(store_path)
        .map_err(|e| format!("failed to open store: {e}"))?;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        if !pid_path.exists() {
            break;
        }
    }

    Ok(())
}

fn cmd_daemon_stop() -> Result<(), String> {
    let pid_path = daemon_pid_path();
    if !pid_path.exists() {
        return Err("daemon is not running (no PID file)".to_string());
    }

    let pid_str = std::fs::read_to_string(&pid_path)
        .map_err(|e| format!("failed to read PID file: {e}"))?;
    let pid: u32 = pid_str
        .trim()
        .parse()
        .map_err(|e| format!("invalid PID in file: {e}"))?;

    if !is_process_running(pid) {
        println!("Daemon is not running (stale PID file)");
        std::fs::remove_file(&pid_path).ok();
        return Ok(());
    }

    #[cfg(unix)]
    {
        let result = std::process::Command::new("kill")
            .arg(pid.to_string())
            .output();
        match result {
            Ok(_) => {
                std::fs::remove_file(&pid_path).ok();
                println!("Daemon stopped (PID: {})", pid);
            }
            Err(e) => return Err(format!("failed to stop daemon: {e}")),
        }
    }
    #[cfg(not(unix))]
    {
        return Err("daemon stop is not supported on this platform".to_string());
    }

    Ok(())
}

fn cmd_daemon_status() -> Result<(), String> {
    let pid_path = daemon_pid_path();
    if !pid_path.exists() {
        println!("Daemon is not running");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_path)
        .map_err(|e| format!("failed to read PID file: {e}"))?;
    let pid: u32 = pid_str
        .trim()
        .parse()
        .map_err(|e| format!("invalid PID in file: {e}"))?;

    if is_process_running(pid) {
        println!("Daemon is running (PID: {})", pid);
    } else {
        println!("Daemon is not running (stale PID: {})", pid);
        std::fs::remove_file(&pid_path).ok();
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Completions cache
// ---------------------------------------------------------------------------

fn cmd_completions_cache(action: CompletionsCacheAction) -> Result<(), String> {
    let cache_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mgpm")
        .join("completions");

    match action {
        CompletionsCacheAction::Warm => {
            std::fs::create_dir_all(&cache_dir)
                .map_err(|e| format!("failed to create completions cache dir: {e}"))?;

            let project_root = std::env::current_dir().map_err(|e| e.to_string())?;
            match mgpm_workspace::Workspace::discover(&project_root) {
                Ok(workspace) => {
                    let members: Vec<String> =
                        workspace.members().iter().map(|m| m.name.clone()).collect();

                    let cache_file = cache_dir.join("workspace_members.json");
                    let json = serde_json::to_string_pretty(&members)
                        .map_err(|e| format!("failed to serialize cache: {e}"))?;
                    std::fs::write(&cache_file, json)
                        .map_err(|e| format!("failed to write cache: {e}"))?;

                    println!(
                        "{} Cached {} workspace member names",
                        "[OK]".green().bold(),
                        members.len()
                    );
                }
                Err(_) => {
                    println!(
                        "  {} No workspace found, cache will be empty",
                        "[WARN]".yellow().bold()
                    );
                    let cache_file = cache_dir.join("workspace_members.json");
                    std::fs::write(&cache_file, "[]")
                        .map_err(|e| format!("failed to write cache: {e}"))?;
                }
            }

            Ok(())
        }
        CompletionsCacheAction::Clear => {
            if cache_dir.exists() {
                std::fs::remove_dir_all(&cache_dir)
                    .map_err(|e| format!("failed to clear completions cache: {e}"))?;
                println!("{} Cleared completion cache", "[OK]".green().bold());
            } else {
                println!("  {} No cache to clear", "[WARN]".yellow().bold());
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
        content =
            std::fs::read_to_string(&npmrc_path).map_err(|e| format!("failed to read .npmrc: {}", e))?;
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
    std::fs::write(&npmrc_path, &content)
        .map_err(|e| format!("failed to write .npmrc: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) =
            std::fs::set_permissions(&npmrc_path, std::fs::Permissions::from_mode(0o600))
        {
            eprintln!("  {} Failed to set permissions on .npmrc: {}", "[WARN]".yellow().bold(), e);
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
    if key.contains("auth") || key.contains("token") || key.contains("password") || key.contains("_auth") {
        auth::redact_auth(value)
    } else {
        value.to_string()
    }
}

fn handle_config_command(cmd: ConfigCommand) -> Result<(), String> {
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
        ConfigCommand::Trusted { command } => handle_config_trusted(command),
    }
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
