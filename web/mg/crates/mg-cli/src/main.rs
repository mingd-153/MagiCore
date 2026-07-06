use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use std::str::FromStr;

use clap::{CommandFactory, Parser};
use colored::Colorize;
use tokio::sync::mpsc;

use mg_core::config::{ConfigLoader, DefaultConfigLoader, MgpmConfig};
use mg_installer::installer::{InstallOptions as RealInstallOptions, Installer};
use mg_linker::linker::LinkerStrategy;
use mg_registry::{NpmRegistry, RegistryClient};
use mg_resolver::{solver::ResolvedDep, DependencyProvider, Resolver};
use mg_resolver::cache::RegistryCache;

mod advisory_db;
mod auth;
mod commands;
mod importer;
mod profiler;
mod sandbox;
mod tuf;

use commands::*;
use profiler::PhaseProfiler;

/// Registry-backed DependencyProvider for resolver
struct RegistryDependencyProvider {
    registry: Arc<NpmRegistry>,
    cache: RegistryCache,
}

impl RegistryDependencyProvider {
    fn new(registry: NpmRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
            cache: RegistryCache::new(),
        }
    }
}

fn extract_deps_from_json(
    json: &serde_json::Value,
    version_str: &str,
) -> Vec<ResolvedDep> {
    let versions = match json.get("versions").and_then(|v| v.as_object()) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let version_info = match versions.get(version_str) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let mut deps = Vec::new();

    if let Some(reg_deps) = version_info.get("dependencies").and_then(|v| v.as_object()) {
        for (name, version) in reg_deps {
            if let (Ok(pkg_name), Some(spec)) =
                (mg_core::PackageName::new(name), version.as_str())
            {
                deps.push(ResolvedDep {
                    package: pkg_name,
                    spec: spec.to_string(),
                    optional: false,
                    peer: false,
                });
            }
        }
    }

    if let Some(opt_deps) =
        version_info
            .get("optionalDependencies")
            .and_then(|v| v.as_object())
    {
        for (name, version) in opt_deps {
            if let (Ok(pkg_name), Some(spec)) =
                (mg_core::PackageName::new(name), version.as_str())
            {
                if mg_core::platform::is_platform_match(name) {
                    deps.push(ResolvedDep {
                        package: pkg_name,
                        spec: spec.to_string(),
                        optional: true,
                        peer: false,
                    });
                }
            }
        }
    }

    deps
}

impl DependencyProvider for RegistryDependencyProvider {
    fn get_versions(&self, package: &mg_core::PackageName) -> Vec<mg_core::Version> {
        let key = package.as_str().to_string();
        if let Some(versions) = self.cache.get_versions(&key) {
            return versions;
        }

        let t0 = std::time::Instant::now();
        let (versions, json) = match tokio::runtime::Handle::current()
            .block_on(self.registry.get_package_versions_with_metadata(package))
        {
            Ok((v, j)) => (v, j),
            Err(e) => {
                eprintln!(
                    "  {} failed to fetch versions for {}: {}",
                    "[WARN]".yellow().bold(),
                    package,
                    e
                );
                return Vec::new();
            }
        };
        let elapsed = t0.elapsed();
        if elapsed.as_millis() > 200 {
            eprintln!("  {} get_versions({}) took {}ms ({} versions)", "[TIMING]".yellow().bold(), package, elapsed.as_millis(), versions.len());
        }
        self.cache.insert_versions(key.clone(), versions.clone());
        if let Some(json) = json {
            self.cache.insert_metadata(key, json);
        }
        versions
    }

    fn get_dependencies(&self, package_id: &mg_core::PackageId) -> Vec<ResolvedDep> {
        let name_str = package_id.name().as_str().to_string();
        let version_str = package_id.version().to_string();
        let cache_key = format!("{}@{}", name_str, version_str);
        if let Some(deps) = self.cache.get_deps(&cache_key) {
            return deps;
        }

        let t0 = std::time::Instant::now();
        // Try in-memory metadata cache first (avoids MemMapCache deserialization)
        let deps = if let Some(json) = self.cache.get_metadata(&name_str) {
            extract_deps_from_json(&json, &version_str)
        } else {
            // Fall back to registry fetch
            let json = match tokio::runtime::Handle::current()
                .block_on(self.registry.get_package(package_id.name()))
            {
                Ok(j) => j,
                Err(e) => {
                    eprintln!(
                        "  {} failed to fetch dependencies for {}: {}",
                        "[WARN]".yellow().bold(),
                        package_id.name(),
                        e
                    );
                    return Vec::new();
                }
            };
            self.cache.insert_metadata(name_str.clone(), json.clone());
            extract_deps_from_json(&json, &version_str)
        };
        let elapsed = t0.elapsed();
        if elapsed.as_millis() > 100 {
            eprintln!("  {} get_deps({}@{}) took {}ms ({} deps)", "[TIMING]".yellow().bold(), name_str, version_str, elapsed.as_millis(), deps.len());
        }

        self.cache.insert_deps(cache_key, deps.clone());
        deps
    }

    fn prefetch_versions(&self, packages: &[mg_core::PackageName]) -> Vec<(mg_core::PackageName, Vec<mg_core::Version>)> {
        let mut to_fetch: Vec<mg_core::PackageName> = Vec::new();
        let mut results: Vec<(mg_core::PackageName, Vec<mg_core::Version>)> = Vec::new();

        for name in packages {
            let key = name.as_str().to_string();
            if let Some(versions) = self.cache.get_versions(&key) {
                results.push((name.clone(), versions));
            } else {
                to_fetch.push(name.clone());
            }
        }

        if to_fetch.is_empty() {
            return results;
        }

        // Fetch all uncached packages in parallel, also caching full metadata
        let reg = self.registry.clone();
        let cache = self.cache.clone();
        let t0 = std::time::Instant::now();
        let num_to_fetch = to_fetch.len();
        let fetched: Vec<(mg_core::PackageName, Option<Vec<mg_core::Version>>)> = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut handles = Vec::new();
                for name in to_fetch {
                    let r = reg.clone();
                    let c = cache.clone();
                    handles.push(tokio::spawn(async move {
                        let result = r.get_package_versions_with_metadata(&name).await;
                        if let Ok((_, Some(ref json))) = result {
                            c.insert_metadata(name.as_str().to_string(), json.clone());
                        }
                        (name, result.ok().map(|(v, _)| v))
                    }));
                }

                let mut collected = Vec::new();
                for handle in handles {
                    if let Ok((name, versions)) = handle.await {
                        collected.push((name, versions));
                    }
                }
                collected
            })
        });
        let fetch_ms = t0.elapsed().as_millis();
        eprintln!("  {} prefetched {} packages in {}ms", "[TIMING]".yellow().bold(), num_to_fetch, fetch_ms);

        for (name, versions_opt) in fetched {
            let versions = versions_opt.unwrap_or_default();
            if versions.is_empty() {
                eprintln!(
                    "  {} no versions found for {}",
                    "[WARN]".yellow().bold(),
                    name
                );
            }
            self.cache.insert_versions(name.to_string(), versions.clone());
            results.push((name, versions));
        }

        results
    }
}

#[derive(Parser)]
#[command(name = "mg", version, about = "MegaGate Package Manager")]
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
    #[arg(
        long,
        global = true,
        help = "Path to config file (mg.yaml/mg.toml)"
    )]
    config: Option<PathBuf>,
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
    /// Check for outdated packages (compare installed vs latest)
    Outdated {
        /// Include devDependencies (default: true)
        #[arg(short, long, default_value_t = true)]
        dev: bool,
    },
    Run {
        script: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Start the dev server (runs `scripts.dev` from package.json)
    Dev {
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
    /// Scaffold a new React SPA (Vite + Router + Zustand)
    #[command(name = "create-react")]
    CreateReact {
        /// Project directory name
        name: String,
        /// Add TypeScript support (types, tsconfig, eslint)
        #[arg(long)]
        ts: bool,
    },
    /// Scaffold a new web project
    ///
    /// Vanilla: mg create-web <name> [--flags...]
    ///     → HTML+CSS+JS, no framework
    ///
    /// Framework: mg create-web <framework>[@<version>] <name> [--flags...]
    ///     → Uses framework template (react, vue, next-app, ...)
    #[command(name = "create-web")]
    CreateWeb {
        /// Framework name (e.g. react, vue) + optional @version — omit for vanilla
        #[arg()]
        args: Vec<String>,
        /// Add TypeScript (auto-enables --vite for vanilla)
        #[arg(long)]
        ts: bool,
        /// Add Vite bundler (dev server, HMR, build)
        #[arg(long)]
        vite: bool,
        /// Add Tailwind CSS (auto-enables --vite)
        #[arg(long)]
        tailwindcss: bool,
        /// Add Bootstrap CSS
        #[arg(long)]
        bootstrap: bool,
        /// Add NUI component kit (web components)
        #[arg(long)]
        nui: bool,
        /// Add Sass/SCSS support (auto-enables --vite)
        #[arg(long)]
        sass: bool,
        /// Add API client utility
        #[arg(long)]
        api: bool,
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
    /// Explain why a package is installed
    Why {
        /// Package name to investigate
        package: String,
    },
    /// Display package metadata from the registry
    Info {
        /// Package name
        package: String,
    },
    /// List installed packages from mg.lock
    #[command(alias = "ls")]
    List,
    /// Link a local package to node_modules
    Link {
        /// Package name or local path
        package: String,
    },
    /// Unlink a linked package from node_modules
    Unlink {
        /// Package name to unlink
        package: String,
    },
    /// Upgrade mg itself to the latest version
    Upgrade,
    /// Download + run a package binary without installing (alias: mg x)
    #[command(aliases = &["x"])]
    Dlx {
        /// Package to run (e.g. "tsx" or "typescript@5.7")
        package: String,
        /// Arguments to pass to the binary
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Publish a package to the npm registry
    Publish {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Prepare a package for patching
    Patch {
        /// Package to patch (e.g. "lodash" or "lodash@4.17.21")
        package: String,
        /// Commit the patch after editing
        #[arg(long)]
        commit: bool,
    },
}

// ---------------------------------------------------------------------------
// Config loading with precedence: project config > user config > env > defaults
// ---------------------------------------------------------------------------

fn load_config(config_path: Option<&PathBuf>) -> (MgpmConfig, Option<PathBuf>) {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let uc_path = home.join(".config").join("mg").join("config.toml");
    let mut user_config_path: Option<PathBuf> = None;

    let mut config = MgpmConfig::default();

    if let Some(cfg_path) = config_path {
        // Use explicit config path
        if cfg_path.exists() {
            if let Ok(pc) = MgpmConfig::load(cfg_path) {
                merge_into(&mut config, pc);
            }
        }
    } else {
        // Fallback: check default locations
        if uc_path.exists() {
            user_config_path = Some(uc_path.clone());
            if let Ok(uc) = MgpmConfig::load(&uc_path) {
                merge_into(&mut config, uc);
            }
        }

        for path in &[
            PathBuf::from("mg.yaml"),
            PathBuf::from("mg.yml"),
            PathBuf::from("mg.toml"),
        ] {
            if path.exists() {
                if let Ok(pc) = MgpmConfig::load(path) {
                    merge_into(&mut config, pc);
                }
                break;
            }
        }
    }

    apply_npmrc_config(&mut config);
    DefaultConfigLoader::apply_env(&mut config);
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
        .join("mg")
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
    workspace: &'a mg_workspace::Workspace,
    recursive: bool,
    filter: &[String],
    since: Option<&str>,
) -> Result<Vec<&'a mg_workspace::WorkspaceMember>, String> {
    use std::collections::HashSet;

    // Build filter selectors from --filter strings
    let mut selectors: Vec<mg_workspace::FilterSelector> = Vec::new();
    for f in filter {
        match mg_workspace::FilterEngine::parse_selector(f) {
            Ok(s) => selectors.push(s),
            Err(_) => {
                selectors.push(mg_workspace::FilterSelector::NameGlob(format!("*{}*", f)));
            }
        }
    }

    // Add --since as ChangedSince selector
    if let Some(ref_) = since {
        selectors.push(mg_workspace::FilterSelector::ChangedSince {
            path_filter: None,
            git_ref: ref_.to_string(),
        });
    }

    // Get matching members
    let mut members: Vec<&mg_workspace::WorkspaceMember> = if selectors.is_empty() {
        if recursive {
            workspace
                .topological_sort()
                .map_err(|e| format!("topological sort failed: {e}"))?
        } else {
            workspace.members().iter().collect()
        }
    } else {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut result: Vec<&mg_workspace::WorkspaceMember> = Vec::new();
        for selector in &selectors {
            for m in workspace.filter(selector) {
                if seen.insert(m.name.as_str()) {
                    result.push(m);
                }
            }
        }
        result
    };

    // If recursive, sort matched members in topological order
    if recursive {
        let sorted = workspace
            .topological_sort()
            .map_err(|e| format!("topological sort failed: {e}"))?;
        let selected: HashSet<&str> = members.iter().map(|m| m.name.as_str()).collect();
        members = sorted
            .into_iter()
            .filter(|m| selected.contains(m.name.as_str()))
            .collect();
    }

    Ok(members)
}

fn run_on_members(
    members: &[&mg_workspace::WorkspaceMember],
    command_label: &str,
    fail_fast: bool,
    mut f: impl FnMut(&mg_workspace::WorkspaceMember) -> Result<(), String>,
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
    let workspace = mg_workspace::Workspace::discover(&project_root)
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

        let result = cmd_install(
            config,
            offline,
            production,
            hoist,
            profile,
            timings,
            linker.clone(),
        )
        .await;

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
    config: &MgpmConfig,
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
    let workspace = mg_workspace::Workspace::discover(&project_root)
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

        let result = cmd_add(packages.to_vec(), dev, peer, optional, exact, config, false, false, false, false, false, "hoisted".to_string()).await;

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
    config: &MgpmConfig,
    recursive: bool,
    filter: &[String],
    since: Option<&str>,
    fail_fast: bool,
    packages: &[String],
) -> Result<(), String> {
    let project_root = std::env::current_dir().map_err(|e| e.to_string())?;
    let workspace = mg_workspace::Workspace::discover(&project_root)
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

        let result = cmd_remove(packages.to_vec(), config, false, false, false, false, false, "hoisted".to_string()).await;

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
    config: &MgpmConfig,
    recursive: bool,
    filter: &[String],
    since: Option<&str>,
    fail_fast: bool,
    latest: bool,
) -> Result<(), String> {
    let project_root = std::env::current_dir().map_err(|e| e.to_string())?;
    let workspace = mg_workspace::Workspace::discover(&project_root)
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

        let result = cmd_update(latest, config, false, false).await;

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
        let opt_deps = pkg.get("optionalDependencies").and_then(|d| d.as_object());
        let dev_deps = pkg.get("devDependencies").and_then(|d| d.as_object());

        let mut wanted: Vec<mg_lockfile::WantedDependency> = Vec::new();
        if let Some(d) = deps {
            for (name, version) in d {
                let v = version.as_str().unwrap_or("*");
                wanted.push(mg_lockfile::WantedDependency {
                    name: mg_core::PackageName::new(name)
                        .map_err(|e| format!("invalid package name '{}': {}", name, e))?,
                    version_req: v.to_string(),
                    dev: false,
                    optional: false,
                });
            }
        }
        if let Some(d) = opt_deps {
            for (name, version) in d {
                if !mg_core::platform::is_platform_match(name) {
                    continue;
                }
                let v = version.as_str().unwrap_or("*");
                wanted.push(mg_lockfile::WantedDependency {
                    name: mg_core::PackageName::new(name)
                        .map_err(|e| format!("invalid package name '{}': {}", name, e))?,
                    version_req: v.to_string(),
                    dev: false,
                    optional: true,
                });
            }
        }
        if !production {
            if let Some(d) = dev_deps {
                for (name, version) in d {
                    let v = version.as_str().unwrap_or("*");
                    wanted.push(mg_lockfile::WantedDependency {
                        name: mg_core::PackageName::new(name)
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

    let lockfile = if Path::new("mg.lock").exists() {
        mg_lockfile::text::read_text(Path::new("mg.lock"))
            .map_err(|e| format!("failed to read lockfile: {}", e))?
    } else {
        eprintln!(
            "  {} No lockfile found, generating from registry...",
            "[INFO]".cyan().bold()
        );

        let npm_registry = NpmRegistry::new("https://registry.npmjs.org");
        let provider = RegistryDependencyProvider::new(npm_registry);
        let resolver = Resolver::new(std::sync::Arc::new(provider));
        let config = mg_lockfile::ResolutionConfig::default();
        let pipeline = mg_lockfile::ResolutionPipeline::new(resolver, config);
        let registry_client = RegistryClient::new();

        let lockfile = pipeline
            .resolve_and_lock(
                &wanted_deps,
                &std::env::current_dir().unwrap(),
                Some(&registry_client),
            )
            .await
            .map_err(|e| format!("failed to generate lockfile: {}", e))?;

        lockfile
    };

    // Verify lockfile integrity if reading from an existing lockfile
    if Path::new("mg.lock").exists() && !lockfile.verify_content_hash() {
        return Err("lockfile content hash mismatch — aborting install for security. Run `mg lockfile validate` for details.".to_string());
    }

    profiler.end("resolve");

    // Dependency confusion check
    {
        let workspace_packages: Vec<String> = if let Ok(root) = std::env::current_dir() {
            if let Ok(ws) = mg_workspace::Workspace::discover(&root) {
                ws.members().iter().map(|m| m.name.clone()).collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let dep_infos: Vec<mg_resolver::solver::DepInfo> = wanted_deps
            .iter()
            .map(|d| mg_resolver::solver::DepInfo {
                name: d.name.as_str().to_string(),
                version: Some(d.version_req.clone()),
                registry: config.registries.first().map(|r| r.url.clone()),
            })
            .collect();

        let warnings = mg_resolver::solver::check_dependency_confusion(
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
        virtual_store_path: PathBuf::from(".mg"),
        hoisted_node_modules: hoist || config.install.hoist,
        hoist_pattern: config.install.hoist_pattern.clone(),
        offline: offline || config.cli.dry_run,
        dry_run: config.cli.dry_run,
        project_root: std::env::current_dir().unwrap_or_default(),
        sqlite_path: dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".mg")
            .join("mg.db"),
        jsonl_log: false,
        linker_strategy: LinkerStrategy::from_str(&linker)
            .map_err(|e| format!("invalid linker strategy: {}", e))?,
        gvs_root: dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".mg")
            .join("gvs")
            .join("v1"),
    };

    let (tx, mut rx) = mpsc::channel::<mg_installer::installer::InstallProgress>(256);

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
        let timings_dir = PathBuf::from(".mg").join("timings");
        std::fs::create_dir_all(&timings_dir)
            .map_err(|e| format!("failed to create timings dir: {e}"))?;

        let latest_path = timings_dir.join("latest.json");
        std::fs::write(&latest_path, &json).map_err(|e| format!("failed to write timings: {e}"))?;

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

/// Parse a package spec like `zod`, `zod@3.22`, `@types/react@^19`, `zod@latest`
/// into (package_name, version_or_range).
/// Returns None for the version when only the package name is given (e.g. `zod`).
fn parse_package_spec(spec: &str) -> Result<(String, Option<String>), String> {
    // Scoped packages: @scope/name@version — need to split after the scope
    if let Some(rest) = spec.strip_prefix('@') {
        if let Some(at) = rest.find('@') {
            let name = format!("@{}", &rest[..at]); // @scope/name
            let version = rest[at + 1..].to_string();
            Ok((name, Some(version)))
        } else {
            let name = format!("@{}", rest);
            Ok((name, None))
        }
    } else if let Some(at) = spec.find('@') {
        let name = spec[..at].to_string();
        let version = spec[at + 1..].to_string();
        Ok((name, Some(version)))
    } else {
        Ok((spec.to_string(), None))
    }
}

/// Resolve actual version from npm registry
async fn resolve_latest_version(name: &str) -> Result<String, String> {
    use mg_registry::NpmRegistry;
    let npm = NpmRegistry::new("https://registry.npmjs.org");
    let pkg_name = mg_core::PackageName::new(name)
        .map_err(|e| format!("invalid package name '{name}': {e}"))?;
    let versions = npm.get_package_versions(&pkg_name).await
        .map_err(|e| format!("failed to fetch versions for '{name}': {e}"))?;
    versions.last()
        .map(|v| v.to_string())
        .ok_or_else(|| format!("no versions found for '{name}'"))
}

fn update_package_json_deps(
    pkg: &mut serde_json::Value,
    entries: &[(String, String)],   // (package_name, version_or_range)
    dev: bool,
    add: bool,
) -> Result<(), String> {
    let section = if dev { "devDependencies" } else { "dependencies" };
    if pkg.get(section).and_then(|d| d.as_object()).is_none() {
        pkg[section] = serde_json::Value::Object(serde_json::Map::new());
    }

    if add {
        let map = pkg[section].as_object_mut().unwrap();
        for (pkg_name, version) in entries {
            map.insert(pkg_name.clone(), serde_json::Value::String(version.clone()));
        }
    } else {
        let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
        for s in ["dependencies", "devDependencies", "optionalDependencies", "peerDependencies"] {
            if let Some(map) = pkg.get_mut(s).and_then(|d| d.as_object_mut()) {
                for pkg_name in &names {
                    map.remove(*pkg_name);
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_add(
    packages: Vec<String>,
    dev: bool,
    peer: bool,
    _optional: bool,
    exact: bool,
    config: &MgpmConfig,
    offline: bool,
    production: bool,
    hoist: bool,
    profile: bool,
    timings: bool,
    linker: String,
) -> Result<(), String> {
    if packages.is_empty() {
        return Err("No packages specified. Usage: mg add <package> [<package>...]".to_string());
    }

    let mut pkg = load_package_json(Path::new("package.json"))?;

    // Parse each spec into (name, version)
    let mut entries: Vec<(String, String)> = Vec::new();
    for spec in &packages {
        let (name, version_spec) = parse_package_spec(spec)?;
        let version = match version_spec {
            Some(v) if exact => {
                // User provided version + --exact → use as-is (could be exact or already semver)
                v
            }
            Some(v) => {
                // User provided version → prefix with ^ if it's a plain number
                if v.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    format!("^{}", v)
                } else {
                    v // already has a range prefix like ^, ~, >=, etc.
                }
            }
            None if exact => {
                // No version + --exact → resolve exact from registry
                let latest = resolve_latest_version(&name).await?;
                latest
            }
            None => {
                // No version → resolve latest and prefix with ^
                match resolve_latest_version(&name).await {
                    Ok(v) => format!("^{}", v),
                    Err(_) => "*".to_string(), // fallback
                }
            }
        };
        entries.push((name, version));
    }

    update_package_json_deps(&mut pkg, &entries, dev || peer, true)?;

    let json_str = serde_json::to_string_pretty(&pkg)
        .map_err(|e| format!("failed to serialize package.json: {e}"))?;
    std::fs::write("package.json", json_str)
        .map_err(|e| format!("failed to write package.json: {e}"))?;

    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    eprintln!(
        "{} Added {} to {}",
        "[OK]".green().bold(),
        names.join(", ").green(),
        if dev || peer { "devDependencies" } else { "dependencies" }.green()
    );

    // Force lockfile regeneration to include new packages
    let _ = std::fs::remove_file("mg.lock");
    let _ = std::fs::remove_file("mg.lockb");

    cmd_install(config, offline, production, hoist, profile, timings, linker).await
}

#[allow(clippy::too_many_arguments)]
async fn cmd_remove(
    packages: Vec<String>,
    config: &MgpmConfig,
    offline: bool,
    production: bool,
    hoist: bool,
    profile: bool,
    timings: bool,
    linker: String,
) -> Result<(), String> {
    if packages.is_empty() {
        return Err("No packages specified. Usage: mg remove <package> [<package>...]".to_string());
    }

    let mut pkg = load_package_json(Path::new("package.json"))?;

    // Parse specs to get just the package names
    let mut entries: Vec<(String, String)> = Vec::new();
    for spec in &packages {
        let (name, _) = parse_package_spec(spec)?;
        entries.push((name, String::new()));
    }

    update_package_json_deps(&mut pkg, &entries, false, false)?;

    let json_str = serde_json::to_string_pretty(&pkg)
        .map_err(|e| format!("failed to serialize package.json: {e}"))?;
    std::fs::write("package.json", json_str)
        .map_err(|e| format!("failed to write package.json: {e}"))?;

    eprintln!(
        "{} Removed {} from package.json",
        "[OK]".green().bold(),
        packages.join(", ").green()
    );

    // Force lockfile regeneration to reflect removed packages
    let _ = std::fs::remove_file("mg.lock");
    let _ = std::fs::remove_file("mg.lockb");

    cmd_install(config, offline, production, hoist, profile, timings, linker).await
}

async fn cmd_outdated(include_dev: bool) -> Result<(), String> {
    let lockfile_path = Path::new("mg.lock");
    if !lockfile_path.exists() {
        return Err("no mg.lock found — run `mg install` first".into());
    }
    let lockfile = mg_lockfile::text::read_text(lockfile_path)
        .map_err(|e| format!("failed to read mg.lock: {e}"))?;

    // Read package.json for wanted versions
    let pkg_json: serde_json::Value = {
        let content = std::fs::read_to_string("package.json")
            .map_err(|e| format!("failed to read package.json: {e}"))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse package.json: {e}"))?
    };

    let deps: Vec<(String, String)> = {
        let mut all = Vec::new();
        if let Some(deps) = pkg_json.get("dependencies").and_then(|v| v.as_object()) {
            for (name, ver) in deps {
                if let Some(v) = ver.as_str() {
                    all.push((name.clone(), v.to_string()));
                }
            }
        }
        if include_dev {
            if let Some(devs) = pkg_json.get("devDependencies").and_then(|v| v.as_object()) {
                for (name, ver) in devs {
                    if let Some(v) = ver.as_str() {
                        all.push((name.clone(), v.to_string()));
                    }
                }
            }
        }
        all
    };

    // Build map: name -> installed version from lockfile
    let installed: std::collections::HashMap<&str, &str> = lockfile
        .packages
        .iter()
        .map(|p| (p.name.as_str(), p.version.as_str()))
        .collect();

    let npm = NpmRegistry::new("https://registry.npmjs.org");
    let mut outdated = Vec::new();
    let mut current = Vec::new();

    for (name, wanted) in &deps {
        let installed_ver = installed.get(name.as_str()).copied().unwrap_or("-");
        let pkg_name = mg_core::PackageName::new(name)
            .map_err(|e| format!("invalid package name '{name}': {e}"))?;
        let latest_ver = match npm.get_package_versions(&pkg_name).await {
            Ok(versions) => versions
                .last()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "?".to_string()),
            Err(_) => "ERR".to_string(),
        };

        // Normalize semver: strip leading ^ ~ for comparison
        let installed_normalized = installed_ver.trim_start_matches('^').trim_start_matches('~');

        // Skip if installed version matches latest
        if installed_normalized == latest_ver.trim_start_matches('^').trim_start_matches('~') {
            current.push((name.clone(), installed_ver.to_string(), latest_ver));
        } else {
            outdated.push((
                name.clone(),
                installed_ver.to_string(),
                wanted.clone(),
                latest_ver,
            ));
        }
    }

    // Print results
    if outdated.is_empty() {
        eprintln!("{} All packages are up-to-date", "[OK]".green().bold());
        return Ok(());
    }

    eprintln!(
        "{} Found {} outdated package(s)\n",
        "!!".yellow().bold(),
        outdated.len()
    );

    // Header
    eprintln!(
        "{:<25} {:<14} {:<14} {:<14}",
        "Package", "Installed", "Wanted", "Latest"
    );
    eprintln!("{}", "-".repeat(67));

    for (name, installed, wanted, latest) in &outdated {
        eprintln!(
            "{:<25} {:<14} {:<14} {:<14}",
            name.red(),
            installed.yellow(),
            wanted,
            latest.green()
        );
    }

    eprintln!();
    if !current.is_empty() {
        eprintln!(
            "{} {} package(s) up-to-date (not shown)",
            "[OK]".green().bold(),
            current.len()
        );
    }

    Ok(())
}

async fn cmd_why(package: &str) -> Result<(), String> {
    let lockfile_path = Path::new("mg.lock");
    if !lockfile_path.exists() {
        return Err("no mg.lock found — run `mg install` first".into());
    }
    let lockfile = mg_lockfile::text::read_text(lockfile_path)
        .map_err(|e| format!("failed to read mg.lock: {e}"))?;

    let target = lockfile.packages.iter().find(|p| p.name == package)
        .ok_or_else(|| format!("package '{}' not found in mg.lock", package))?;

    let mut dependents: Vec<&mg_lockfile::LockfilePackage> = Vec::new();
    for pkg in &lockfile.packages {
        for dep in &pkg.dependencies {
            if dep.starts_with(package) {
                dependents.push(pkg);
                break;
            }
        }
    }

    println!("{} {}@{}", "─".red(), package.green().bold(), target.version.green());
    if dependents.is_empty() {
        println!("   {} This is a direct dependency or has no dependents", "└─".yellow());
    } else {
        println!("   {} Required by:", "└─".yellow());
        for dep in &dependents {
            println!("      {} {}@{}", "├─".cyan(), dep.name.yellow(), dep.version.yellow());
        }
    }
    Ok(())
}

async fn cmd_info(package: &str) -> Result<(), String> {
    let npm = NpmRegistry::new("https://registry.npmjs.org");
    let pkg_name = mg_core::PackageName::new(package)
        .map_err(|e| format!("invalid package name '{package}': {e}"))?;

    let info = npm.get_package(&pkg_name).await
        .map_err(|e| format!("failed to fetch info for '{package}': {e}"))?;

    let name = info["name"].as_str().unwrap_or(package);
    let description = info["description"].as_str().unwrap_or("");
    let latest_ver = info["dist-tags"]["latest"].as_str().unwrap_or("?");
    let license = info["license"].as_str().unwrap_or("unknown");
    let version_count = info["versions"].as_object().map(|v| v.len()).unwrap_or(0);

    println!("{}", name.bold().green());
    if !description.is_empty() {
        println!("  {description}");
    }
    println!();
    println!("  {}   {}", "Latest version:".bold(), latest_ver.cyan());
    println!("  {}   {}", "License:".bold(), license);
    println!("  {}   {}", "Versions:".bold(), version_count.to_string().cyan());

    if let Some(homepage) = info["homepage"].as_str() {
        if !homepage.is_empty() {
            println!("  {}   {}", "Homepage:".bold(), homepage);
        }
    }
    if let Some(repository) = info["repository"]["url"].as_str() {
        println!("  {}   {}", "Repository:".bold(), repository);
    }
    if let Some(maintainers) = info["maintainers"].as_array() {
        if !maintainers.is_empty() {
            let names: Vec<&str> = maintainers.iter()
                .filter_map(|m| m["name"].as_str())
                .collect();
            println!("  {}   {}", "Maintainers:".bold(), names.join(", "));
        }
    }
    if let Some(keywords) = info["keywords"].as_array() {
        if !keywords.is_empty() {
            let kw: Vec<&str> = keywords.iter().filter_map(|k| k.as_str()).collect();
            println!("  {}   {}", "Keywords:".bold(), kw.join(", "));
        }
    }
    Ok(())
}

async fn cmd_list() -> Result<(), String> {
    let lockfile_path = Path::new("mg.lock");
    if !lockfile_path.exists() {
        return Err("no mg.lock found — run `mg install` first".into());
    }
    let lockfile = mg_lockfile::text::read_text(lockfile_path)
        .map_err(|e| format!("failed to read mg.lock: {e}"))?;

    let pkg_json: serde_json::Value = {
        let content = std::fs::read_to_string("package.json")
            .map_err(|e| format!("failed to read package.json: {e}"))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse package.json: {e}"))?
    };

    let direct_deps: std::collections::HashSet<String> = {
        let mut set = std::collections::HashSet::new();
        if let Some(deps) = pkg_json.get("dependencies").and_then(|v| v.as_object()) {
            for name in deps.keys() {
                set.insert(name.clone());
            }
        }
        if let Some(devs) = pkg_json.get("devDependencies").and_then(|v| v.as_object()) {
            for name in devs.keys() {
                set.insert(name.clone());
            }
        }
        set
    };

    let direct_names: Vec<&mg_lockfile::LockfilePackage> = lockfile.packages.iter()
        .filter(|p| direct_deps.contains(&p.name))
        .collect();

    eprintln!("{} {} packages installed", "[mg]".green().bold(), lockfile.packages.len());
    eprintln!();

    if !direct_names.is_empty() {
        eprintln!("{}", "dependencies:".bold().underline());
        for pkg in &direct_names {
            eprintln!("  {} {}", pkg.name.cyan(), pkg.version);
        }
        eprintln!();
    }

    let indirect = lockfile.packages.len() - direct_names.len();
    eprintln!("{} indirect dependencies", indirect);
    Ok(())
}

fn cmd_link(package: &str) -> Result<(), String> {
    let path = Path::new(package);
    if !path.is_dir() {
        return Err(format!("'{}' is not a valid directory. Usage: mg link <local-path>", package));
    }
    let pkg_json_path = path.join("package.json");
    if !pkg_json_path.exists() {
        return Err(format!("no package.json found at '{}'", path.display()));
    }
    let content = std::fs::read_to_string(&pkg_json_path)
        .map_err(|e| format!("failed to read package.json: {e}"))?;
    let pkg: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("failed to parse package.json: {e}"))?;
    let target_name = pkg["name"].as_str()
        .ok_or_else(|| format!("no 'name' field in package.json at '{}'", path.display()))?
        .to_string();
    let target_path = std::fs::canonicalize(path)
        .map_err(|e| format!("failed to resolve path '{}': {e}", path.display()))?;

    let node_modules = Path::new("node_modules");
    if !node_modules.exists() {
        std::fs::create_dir_all(node_modules)
            .map_err(|e| format!("failed to create node_modules: {e}"))?;
    }

    let link_path = node_modules.join(&target_name);
    if link_path.exists() {
        std::fs::remove_file(&link_path).ok();
        std::fs::remove_dir_all(&link_path).ok();
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_path, &link_path)
        .map_err(|e| format!("failed to create symlink: {e}"))?;
    #[cfg(not(unix))]
    std::os::windows::fs::symlink_dir(&target_path, &link_path)
        .map_err(|e| format!("failed to create symlink: {e}"))?;

    eprintln!("{} Linked {} → {}", "[OK]".green().bold(), target_name.green(), target_path.display().to_string().cyan());
    Ok(())
}

fn cmd_unlink(package: &str) -> Result<(), String> {
    let link_path = Path::new("node_modules").join(package);
    if !link_path.exists() {
        return Err(format!("package '{}' is not linked in node_modules", package));
    }
    if link_path.is_symlink() {
        std::fs::remove_file(&link_path)
            .map_err(|e| format!("failed to remove symlink: {e}"))?;
    } else {
        std::fs::remove_dir_all(&link_path)
            .map_err(|e| format!("failed to remove directory: {e}"))?;
    }
    eprintln!("{} Unlinked {}", "[OK]".green().bold(), package.green());
    Ok(())
}

fn cmd_upgrade() -> Result<(), String> {
    eprintln!("{} Checking for mg upgrade...", "[INFO]".cyan().bold());
    eprintln!("{} To upgrade, run: curl -fsSL https://mgpm.sh/install.sh | sh", "[HINT]".yellow().bold());
    eprintln!("   Or build from source: cargo install mg-cli");
    Ok(())
}

async fn cmd_dlx(package: &str, args: &[String]) -> Result<(), String> {
    let (pkg_name, pkg_ver) = if let Some(at) = package.rfind('@') {
        (package[..at].to_string(), Some(package[at + 1..].to_string()))
    } else {
        (package.to_string(), None)
    };

    let npm = NpmRegistry::new("https://registry.npmjs.org");
    let name = mg_core::PackageName::new(&pkg_name)
        .map_err(|e| format!("invalid package name '{pkg_name}': {e}"))?;

    let version = if let Some(ver) = pkg_ver {
        if ver == "latest" {
            npm.get_package_versions(&name).await
                .map_err(|e| format!("failed to fetch versions: {e}"))?
                .last()
                .ok_or_else(|| format!("no versions found for '{pkg_name}'"))?
                .to_string()
        } else {
            ver
        }
    } else {
        npm.get_package_versions(&name).await
            .map_err(|e| format!("failed to fetch versions: {e}"))?
            .last()
            .ok_or_else(|| format!("no versions found for '{pkg_name}'"))?
            .to_string()
    };

    let spec = format!("{}@{}", pkg_name, version);
    eprintln!("{} Installing {} in temp sandbox...", "[dlx]".cyan().bold(), spec.cyan());

    // Create a temp project with the package as dependency
    let temp_dir = std::env::temp_dir().join(format!("mg-dlx-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("failed to create temp dir: {e}"))?;

    let pkg_json = serde_json::json!({
        "name": "mg-dlx-temp",
        "private": true,
        "dependencies": {
            &pkg_name: &format!("^{}", version)
        }
    });
    let pkg_json_str = serde_json::to_string_pretty(&pkg_json)
        .map_err(|e| format!("failed to serialize: {e}"))?;
    std::fs::write(temp_dir.join("package.json"), &pkg_json_str)
        .map_err(|e| format!("failed to write package.json: {e}"))?;

    // Run mg install in temp dir
    let prev = std::env::current_dir().ok();
    std::env::set_current_dir(&temp_dir)
        .map_err(|e| format!("failed to enter temp dir: {e}"))?;

    let install_result = cmd_install(
        &MgpmConfig::default(), false, false, false, false, false, "hoisted".to_string()
    ).await;

    if let Some(p) = prev {
        std::env::set_current_dir(p).ok();
    }

    if let Err(e) = install_result {
        return Err(format!("failed to install {}: {e}", spec));
    }

    // Find the binary in node_modules/.bin
    let bin_dir = temp_dir.join("node_modules").join(".bin");
    let bin_name = pkg_name.split('/').next_back().unwrap_or(&pkg_name);
    let bin_candidates = [
        bin_dir.join(bin_name),
        bin_dir.join(format!("{}.cmd", bin_name)),
    ];

    let bin_path = bin_candidates.iter().find(|p| p.exists())
        .cloned()
        .or_else(|| {
            // Look in the package's node_modules/.bin
            let pkg_bin = temp_dir.join("node_modules").join(&pkg_name).join("package.json");
            if let Ok(content) = std::fs::read_to_string(&pkg_bin) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(bin) = json.get("bin") {
                        if let Some(s) = bin.as_str() {
                            return Some(temp_dir.join("node_modules").join(&pkg_name).join(s));
                        }
                        if let Some(obj) = bin.as_object() {
                            if let Some(first) = obj.values().next() {
                                if let Some(v) = first.as_str() {
                                    return Some(temp_dir.join("node_modules").join(&pkg_name).join(v));
                                }
                            }
                        }
                    }
                }
            }
            None
        })
        .ok_or_else(|| format!("no binary found for '{}'", pkg_name))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin_path, std::fs::Permissions::from_mode(0o755)).ok();
    }

    let status = std::process::Command::new(&bin_path)
        .args(args)
        .env("PATH", format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap_or_default()))
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| format!("failed to execute: {e}"))?;

    let _ = std::fs::remove_dir_all(&temp_dir);

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

fn cmd_publish(args: &[String]) -> Result<(), String> {
    if !Path::new("package.json").exists() {
        return Err("no package.json found in current directory".into());
    }

    eprintln!("{} Publishing package...", "[INFO]".cyan().bold());
    eprintln!("{} Delegating to `npm publish`...", "[DLG]".yellow().bold());

    let status = std::process::Command::new("npm")
        .arg("publish")
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| format!("failed to run npm publish: {e}"))?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

fn cmd_patch(package: &str, commit: bool) -> Result<(), String> {
    let (pkg_name, _pkg_ver) = if let Some(at) = package.rfind('@') {
        (package[..at].to_string(), Some(package[at + 1..].to_string()))
    } else {
        (package.to_string(), None)
    };

    let target_dir = Path::new("patches").join(&pkg_name);
    if commit {
        if !target_dir.exists() {
            return Err(format!("no patch found for '{pkg_name}' in patches/"));
        }
        eprintln!("{} Committing patch for {}...", "[INFO]".cyan().bold(), pkg_name.green());
        // TODO: apply the patched files back to node_modules
        eprintln!("{} Patch committed. Run `mg install` to apply.", "[OK]".green().bold());
        return Ok(());
    }

    if target_dir.exists() {
        eprintln!("{} Patch directory already exists at {}", "[WARN]".yellow().bold(), target_dir.display());
        eprintln!("   Edit files there, then run `mg patch {} --commit`", package);
        return Ok(());
    }

    // Resolve from node_modules if already installed
    let nm_path = Path::new("node_modules").join(&pkg_name);
    if nm_path.exists() {
        eprintln!("{} Preparing patch for {} from node_modules...", "[INFO]".cyan().bold(), pkg_name.cyan());
        let _ = std::fs::remove_dir_all(&target_dir);
        copy_dir(&nm_path, &target_dir)
            .map_err(|e| format!("failed to copy package: {e}"))?;
    } else {
        return Err(format!("package '{pkg_name}' not found in node_modules. Run `mg install` first."));
    }

    eprintln!("{} Patch prepared at {}", "[OK]".green().bold(), target_dir.display().to_string().green());
    eprintln!("   Edit files in {} then run `mg patch {} --commit`", target_dir.display().to_string().cyan(), package);
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

async fn cmd_update(latest: bool, config: &MgpmConfig, profiling: bool, timings: bool) -> Result<(), String> {
    if !Path::new("package.json").exists() {
        return Err("no package.json found".into());
    }

    if latest {
        let mut pkg: serde_json::Value = {
            let content = std::fs::read_to_string("package.json")
                .map_err(|e| format!("failed to read package.json: {e}"))?;
            serde_json::from_str(&content)
                .map_err(|e| format!("failed to parse package.json: {e}"))?
        };

        let mut deps_to_update: Vec<String> = Vec::new();
        if let Some(deps) = pkg.get("dependencies").and_then(|v| v.as_object()) {
            for name in deps.keys() {
                deps_to_update.push(name.clone());
            }
        }
        if let Some(devs) = pkg.get("devDependencies").and_then(|v| v.as_object()) {
            for name in devs.keys() {
                if !deps_to_update.contains(name) {
                    deps_to_update.push(name.clone());
                }
            }
        }

        let npm = NpmRegistry::new("https://registry.npmjs.org");
        for name in &deps_to_update {
            let pkg_name = mg_core::PackageName::new(name)
                .map_err(|e| format!("invalid package name '{name}': {e}"))?;
            match npm.get_package_versions(&pkg_name).await {
                Ok(versions) => {
                    if let Some(latest_ver) = versions.last() {
                        if let Some(deps) = pkg.get_mut("dependencies").and_then(|v| v.as_object_mut()) {
                            if let Some(v) = deps.get_mut(name.as_str()) {
                                *v = serde_json::Value::String(format!("^{}", latest_ver));
                            }
                        }
                        if let Some(devs) = pkg.get_mut("devDependencies").and_then(|v| v.as_object_mut()) {
                            if let Some(v) = devs.get_mut(name.as_str()) {
                                *v = serde_json::Value::String(format!("^{}", latest_ver));
                            }
                        }
                        eprintln!("  {} {} → ^{}", "[UPD]".cyan().bold(), name.cyan(), latest_ver);
                    }
                }
                Err(e) => {
                    eprintln!("  {} failed to fetch '{}': {}", "[WARN]".yellow().bold(), name, e);
                }
            }
        }

        let json_str = serde_json::to_string_pretty(&pkg)
            .map_err(|e| format!("failed to serialize package.json: {e}"))?;
        std::fs::write("package.json", json_str)
            .map_err(|e| format!("failed to write package.json: {e}"))?;

        eprintln!("{} Updated package.json to latest versions", "[OK]".green().bold());
    } else {
        eprintln!("{} Re-resolving dependencies (use --latest to bump versions)", "[INFO]".cyan().bold());
    }

    let _ = std::fs::remove_file("mg.lock");
    let _ = std::fs::remove_file("mg.lockb");

    cmd_install(config, false, false, false, profiling, timings, "hoisted".to_string()).await
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
    mg_core::init_tracing(Default::default());

    let cli = Cli::parse();
    let rec = cli.recursive;
    let flt = cli.filter.clone();
    let sinc = cli.since.clone();
    let ff = cli.fail_fast;
    let profiling = cli.profile;
    let timings = cli.timings;
    let (config, _user_config_path) = load_config(cli.config.as_ref());

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
                cmd_install(
                    &config,
                    offline,
                    production,
                    hoist,
                    profiling,
                    timings,
                    linker.clone(),
                )
                .await
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
                cmd_add(
                    packages.clone(),
                    dev,
                    peer,
                    optional,
                    exact,
                    &config,
                    false,
                    false,
                    false,
                    profiling,
                    timings,
                    "hoisted".to_string(),
                )
                .await
            }
        }
        CliCommand::Remove { ref packages } => {
            if is_workspace_context(rec, &flt, sinc.as_deref()) {
                cmd_remove_recursive(&config, rec, &flt, sinc.as_deref(), ff, packages).await
            } else {
                cmd_remove(
                    packages.clone(),
                    &config,
                    false,
                    false,
                    false,
                    profiling,
                    timings,
                    "hoisted".to_string(),
                )
                .await
            }
        }
        CliCommand::Outdated { dev } => {
            cmd_outdated(dev).await
        }
        CliCommand::Update { latest } => {
            if is_workspace_context(rec, &flt, sinc.as_deref()) {
                cmd_update_recursive(&config, rec, &flt, sinc.as_deref(), ff, latest).await
            } else {
                cmd_update(latest, &config, profiling, timings).await
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
                    let workspace = mg_workspace::Workspace::discover(&project_root)
                        .map_err(|e| format!("workspace not found: {e}"))?;
                    let members =
                        resolve_workspace_members(&workspace, rec, &flt, sinc.as_deref())?;
                    cmd_run_recursive(&members, &s, &a, ff)
                })()
            } else {
                run_script(script)
            }
        }
        CliCommand::Dev { ref args } => {
            commands::run::run_dev(args, &config).map_err(|e| format!("{e}"))
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
                    let workspace = mg_workspace::Workspace::discover(&project_root)
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
        CliCommand::Lockfile {
            command: lockfile_cmd,
        } => cmd_lockfile(lockfile_cmd),
        CliCommand::Daemon {
            command: daemon_cmd,
        } => cmd_daemon(daemon_cmd),
        CliCommand::Audit { command } => match command {
            AuditCommand::Run {
                json,
                severity,
                remote,
            } => cmd_audit(&config, json, severity, remote).await,
            AuditCommand::Update { force } => tuf::update_advisories(force).await,
        },
        CliCommand::CreateReact { name, ts } => {
            use mg_scaffold::templates::TemplateRegistry;
            use mg_scaffold::ScaffoldContext;
            use std::collections::HashMap;

            let mut registry = TemplateRegistry::new();
            registry.register_defaults();

            let tpl = registry
                .find_by_command("react")
                .ok_or_else(|| "React template not found".to_string())?;

            let dest = std::env::current_dir()
                .map_err(|e| format!("Cannot get current directory: {e}"))?
                .join(&name);

            let engine = (tpl.create_engine)();

            let mut vars = HashMap::new();
            vars.insert("name".to_string(), name.clone());
            vars.insert("version".to_string(), "1.0.0".to_string());

            let mut features = Vec::new();
            if ts {
                features.push("typescript".to_string());
            }

            let ctx = ScaffoldContext::new(&name, dest.clone())
                .with_vars(vars)
                .with_features(features);

            let result = engine
                .create_project(&ctx, false)
                .map_err(|e| format!("Failed to create React project: {e}"))?;

            println!(
                "{} Created React project '{}' at {}",
                "[OK]".green(),
                name.green(),
                dest.display().to_string().green()
            );
            println!("   {} files", result.files_created.len());
            for f in &result.files_created {
                println!("   {}", f.display());
            }

            if result.files_created.iter().any(|f| f.file_name().map(|n| n == "package.json").unwrap_or(false)) {
                println!("{} Project has package.json — run `mg install` to install dependencies", "[INFO]".blue());
            }

            Ok(())
        }
        CliCommand::CreateWeb {
            args,
            ts,
            vite,
            tailwindcss,
            bootstrap,
            nui,
            sass,
            api,
        } => {
            use mg_scaffold::templates::TemplateRegistry;
            use mg_scaffold::ScaffoldContext;
            use std::collections::HashMap;

            if args.is_empty() || args.len() > 2 {
                return Err("Usage: mg create-web [<framework>[@<version>]] <name> [--flags...]".into());
            }

            let mut registry = TemplateRegistry::new();
            registry.register_defaults();

            let (framework_name, framework_version, project_name) = if args.len() == 1 {
                // Vanilla: mg create-web myapp
                (None, None, args[0].clone())
            } else {
                // Framework: mg create-web react myapp  or  mg create-web react@latest myapp
                let fw = &args[0];
                let (fw_name, fw_ver) = if let Some(at) = fw.rfind('@') {
                    (fw[..at].to_string(), Some(fw[at + 1..].to_string()))
                } else {
                    (fw.clone(), None)
                };
                (Some(fw_name), fw_ver, args[1].clone())
            };

            let (tpl, template_entry) = if let Some(ref fw) = framework_name {
                let template = registry
                    .find_by_command(fw)
                    .or_else(|| registry.get(fw))
                    .ok_or_else(|| format!("Unknown framework '{fw}'. Available: {}. Use 'mg create-web <name>' for vanilla.", {
                        let names: Vec<&str> = registry.list().iter().filter(|t| t.name != "vanilla").map(|t| t.name).collect();
                        names.join(", ")
                    }))?;
                ((template.create_engine)(), template)
            } else {
                let template = registry
                    .get("vanilla")
                    .ok_or_else(|| "Vanilla template not found".to_string())?;
                ((template.create_engine)(), template)
            };
            let dest = std::env::current_dir()
                .map_err(|e| format!("Cannot get current directory: {e}"))?
                .join(&project_name);

            // Warn about unsupported flags for this template
            let flag_map: [(&str, bool); 6] = [
                ("typescript", ts),
                ("tailwindcss", tailwindcss),
                ("bootstrap", bootstrap),
                ("nui", nui),
                ("sass", sass),
                ("api", api),
            ];
            for (flag_name, flag_val) in &flag_map {
                if *flag_val && !template_entry.supported_flags.contains(flag_name) {
                    eprintln!("  {} flag '--{}' is not supported by template '{}' (ignored)",
                        "[WARN]".yellow().bold(), flag_name, template_entry.name);
                }
            }

            let mut vars = HashMap::new();
            vars.insert("name".to_string(), project_name.clone());
            vars.insert("version".to_string(), framework_version.clone().unwrap_or_else(|| "1.0.0".to_string()));

            // TS, Tailwind, Sass auto-enable Vite (need compilation) — vanilla only
            let vite = vite || ts || tailwindcss || sass;

            let mut features = Vec::new();
            if vite {
                features.push("vite".to_string());
            }
            if ts {
                features.push("typescript".to_string());
            }
            if tailwindcss {
                features.push("tailwindcss".to_string());
            }
            if bootstrap {
                features.push("bootstrap".to_string());
            }
            if nui {
                features.push("nui".to_string());
            }
            if sass {
                features.push("sass".to_string());
            }
            if api {
                features.push("api".to_string());
            }

            let ctx = ScaffoldContext::new(&project_name, dest.clone())
                .with_vars(vars)
                .with_features(features);

            let result = tpl.create_project(&ctx, false)
                .map_err(|e| format!("Failed to create project: {e}"))?;

            // If @version was specified (e.g. @latest), resolve from registry and update package.json
            let fw_version = framework_version.clone();
            if let Some(ref fw_ver) = fw_version {
                let pkg_json_path = dest.join("package.json");
                if pkg_json_path.exists() {
                    let content = std::fs::read_to_string(&pkg_json_path)
                        .map_err(|e| format!("failed to read package.json: {e}"))?;
                    if let Ok(mut pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(ref fw) = framework_name {
                            let resolve_name = fw.as_str();
                            let npm = NpmRegistry::new("https://registry.npmjs.org");
                            if let Ok(pkg_name) = mg_core::PackageName::new(resolve_name) {
                                if let Ok(versions) = npm.get_package_versions(&pkg_name).await {
                                    let target_ver = if fw_ver == "latest" || fw_ver.is_empty() {
                                        versions.last().map(|v| v.to_string())
                                    } else {
                                        versions.iter().rev().find(|v| v.to_string().starts_with(fw_ver))
                                            .map(|v| v.to_string())
                                    };
                                    if let Some(actual_ver) = target_ver {
                                        if let Some(deps) = pkg.get_mut("dependencies").and_then(|v| v.as_object_mut()) {
                                            if let Some(v) = deps.get_mut(resolve_name) {
                                                *v = serde_json::Value::String(format!("^{}", actual_ver));
                                            }
                                        }
                                        if let Some(devs) = pkg.get_mut("devDependencies").and_then(|v| v.as_object_mut()) {
                                            if let Some(v) = devs.get_mut(resolve_name) {
                                                *v = serde_json::Value::String(format!("^{}", actual_ver));
                                            }
                                        }
                                        let json_str = serde_json::to_string_pretty(&pkg)
                                            .map_err(|e| format!("failed to serialize: {e}"))?;
                                        std::fs::write(&pkg_json_path, json_str)
                                            .map_err(|e| format!("failed to write package.json: {e}"))?;
                                        eprintln!("   {} Resolved {}@{}", "[OK]".green().bold(), fw.green(), actual_ver.green());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            println!(
                "{} Created project '{}' at {}",
                "[OK]".green(),
                project_name.green(),
                dest.display().to_string().green()
            );
            println!("   {} files", result.files_created.len());
            for f in &result.files_created {
                println!("   {}", f.display());
            }

            // Auto-install if package.json was created
            if result.files_created.iter().any(|f| f.file_name().map(|n| n == "package.json").unwrap_or(false)) {
                println!("{} Project has package.json — run `mg install` to install dependencies", "[INFO]".blue());
            }

            Ok(())
        }
        CliCommand::Verify { deep } => {
            if deep {
                cmd_verify_deep(&config)
            } else {
                cmd_verify(&config)
            }
        }
        CliCommand::Import {
            ref source,
            ref format,
        } => cmd_import(source, format),
        CliCommand::Export { ref output } => cmd_export(output),
        CliCommand::Why { ref package } => cmd_why(package).await,
        CliCommand::Info { ref package } => cmd_info(package).await,
        CliCommand::List => cmd_list().await,
        CliCommand::Link { ref package } => cmd_link(package),
        CliCommand::Unlink { ref package } => cmd_unlink(package),
        CliCommand::Upgrade => cmd_upgrade(),
        CliCommand::Dlx {
            ref package,
            ref args,
        } => cmd_dlx(package, args).await,
        CliCommand::Publish { ref args } => cmd_publish(args),
        CliCommand::Patch { ref package, commit } => cmd_patch(package, commit),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "[ERROR]".red().bold(), e.red());
        std::process::exit(1);
    }
    Ok(())
}
