use std::io;
use std::path::{Path, PathBuf};

use std::str::FromStr;

use clap::{CommandFactory, Parser};
use colored::Colorize;
use tokio::sync::mpsc;

use mgpm_core::config::MgpmConfig;
use mgpm_installer::installer::{InstallOptions as RealInstallOptions, Installer};
use mgpm_linker::linker::LinkerStrategy;
use mgpm_resolver::{DependencyProvider, Resolver, solver::ResolvedDep};
use mgpm_registry::{NpmRegistry, RegistryClient};


mod profiler;
mod advisory_db;
mod tuf;
mod importer;
mod sandbox;
mod auth;
mod commands;

use profiler::PhaseProfiler;
use commands::*;

/// Registry-backed DependencyProvider for resolver
struct RegistryDependencyProvider {
    registry: NpmRegistry,
}

impl DependencyProvider for RegistryDependencyProvider {
    fn get_versions(&self, package: &mgpm_core::PackageName) -> Vec<mgpm_core::Version> {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(self.registry.get_package_versions(package))
            .unwrap_or_default()
    }

    fn get_dependencies(&self, package_id: &mgpm_core::PackageId) -> Vec<ResolvedDep> {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let json = match self.registry.get_package(package_id.name()).await {
                Ok(j) => j,
                Err(_) => return Vec::new(),
            };
            let version_str = package_id.version().to_string();
            let versions = match json.get("versions").and_then(|v| v.as_object()) {
                Some(v) => v,
                None => return Vec::new(),
            };
            let version_info = match versions.get(&version_str) {
                Some(v) => v,
                None => return Vec::new(),
            };
            let deps = match version_info.get("dependencies").and_then(|v| v.as_object()) {
                Some(d) => d,
                None => return Vec::new(),
            };
            deps.iter()
                .filter_map(|(name, version)| {
                    Some(ResolvedDep {
                        package: mgpm_core::PackageName::new(name).ok()?,
                        spec: version.as_str().unwrap_or("*").to_string(),
                        optional: false,
                        peer: false,
                    })
                })
                .collect()
        })
    }
}

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
        /// Linker strategy: hoisted, isolated, pnp
        #[arg(long, default_value = "hoisted")]
        linker: String,
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
    if doc.as_table().is_none_or(|t| t.is_empty()) {
        Ok(format!("{}\n  (empty)", header))
    } else {
        let body = toml::to_string(&doc).map_err(|e| format!("serialize error: {}", e))?;
        Ok(format!("{}\n{}", header, body.trim()))
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


#[allow(clippy::too_many_arguments)]
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
    linker: String,
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

        let result = cmd_install(config, offline, production, hoist, profile, timings, linker.clone()).await;

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

#[allow(clippy::too_many_arguments)]
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
    linker: String,
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
        eprintln!("  {} No lockfile found, generating from registry...", "[INFO]".cyan().bold());
        
        let npm_registry = NpmRegistry::new("https://registry.npmjs.org");
        let provider = RegistryDependencyProvider { registry: npm_registry };
        let resolver = Resolver::new(Box::new(provider));
        let config = mgpm_lockfile::ResolutionConfig::default();
        let pipeline = mgpm_lockfile::ResolutionPipeline::new(resolver, config);
        let registry_client = RegistryClient::new();
        
        let lockfile = pipeline
            .resolve_and_lock(&wanted_deps, &std::env::current_dir().unwrap(), Some(&registry_client))
            .await
            .map_err(|e| format!("failed to generate lockfile: {}", e))?;
        
        lockfile
    };

    // Verify lockfile integrity if reading from an existing lockfile
    if Path::new("mgpm.lock").exists() && !lockfile.verify_content_hash() {
        return Err("lockfile content hash mismatch — aborting install for security. Run `mgpm lockfile validate` for details.".to_string());
    }

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
        linker_strategy: LinkerStrategy::from_str(&linker)
            .map_err(|e| format!("invalid linker strategy: {}", e))?,
        gvs_root: dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".mgpm")
            .join("gvs")
            .join("v1"),
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
            linker,
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
                    linker.clone(),
                )
                .await
            } else {
                cmd_install(&config, offline, production, hoist, profiling, timings, linker.clone()).await
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
            StoreCommand::Verify { fix } => cmd_store_verify(&config, fix),
            StoreCommand::Status => cmd_store_status(&config),
            StoreCommand::Prune { dry_run } => cmd_store_prune(&config, dry_run),
            StoreCommand::Gvs { command: gvs_cmd } => cmd_store_gvs(&config, gvs_cmd),
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


