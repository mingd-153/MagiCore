#[allow(clippy::large_enum_variant)]
pub enum DispatchCommand {
    Common(CommonCommand),
    Core(CoreCommand),
}

pub enum CommonCommand {
    Init {
        template: Option<String>,
        signature: Option<String>,
    },
    Dev {
        host: Option<String>,
        port: Option<u16>,
        clear: bool,
        compat_runtime: Option<String>,
    },
    Info {
        package: String,
        json: bool,
    },
    Search {
        query: String,
        json: bool,
        exact: bool,
        page: Option<u32>,
    },
    Outdated {
        json: bool,
    },
    Audit {
        cmd: Option<crate::commands::definitions::AuditCmd>,
        fix: bool,
        format: Option<String>,
    },
    SelfUpdate {
        version: Option<String>,
        variant: Option<String>,
        dry_run: bool,
        trust_root: Vec<String>,
        allow_unsigned: bool,
    },
    SignRelease {
        manifest: Option<String>,
        key_hex: Option<String>,
    },
    Config {
        cmd: crate::commands::config::ConfigCmd,
        local: bool,
    },
    Stage {
        dir: Option<std::path::PathBuf>,
    },
    Import {
        dir: Option<std::path::PathBuf>,
        allow_unsigned: bool,
    },
    Migrate {
        cmd: crate::commands::migrate::MigrateCmd,
    },
    Sbom {
        format: Option<String>,
        output: Option<std::path::PathBuf>,
        name: Option<String>,
        version: Option<String>,
        dir: Option<std::path::PathBuf>,
    },
    Run {
        script: String,
        args: Vec<String>,
        compat_runtime: Option<String>,
    },
    Test {
        args: Vec<String>,
        compat_runtime: Option<String>,
    },
    Optimizer {
        force: bool,
    },
    Build {
        target: Option<String>,
        compat_runtime: Option<String>,
    },
    Flash {
        board: Option<String>,
        skip_build: bool,
    },
    Deploy {
        run: bool,
    },
    CiGenerate,
    Verify,
    Start,
    Exec {
        command: String,
        args: Vec<String>,
    },
    Dlx {
        package: String,
        args: Vec<String>,
    },
    Cache {
        action: String,
        target: String,
        yes: bool,
        dry_run: bool,
    },
    Link {
        package: Option<String>,
    },
    Unlink {
        package: Option<String>,
    },
    Why {
        package: String,
    },
    Publish {
        protocol: String,
        package: Option<String>,
        version: Option<String>,
        artifacts: Vec<std::path::PathBuf>,
        image: Option<String>,
        tag: Option<String>,
        access: Option<String>,
        dry_run: bool,
        json: bool,
        otp: Option<String>,
        force: bool,
        ignore_scripts: bool,
        allow_scripts: bool,
        no_git_checks: bool,
        publish_branch: Option<String>,
        batch: bool,
        report_summary: bool,
        patch: bool,
        minor: bool,
        major: bool,
        registry: Option<String>,
        token: Option<String>,
        trusted: bool,
        trusted_audience: Option<String>,
    },
    Patch {
        cmd: crate::commands::patch::PatchCmd,
    },
    Dedupe {
        dry_run: bool,
        prefer_latest: bool,
        json: bool,
    },
    Store {
        cmd: crate::commands::store::StoreCmd,
    },
    Bench {
        args: crate::commands::bench::BenchArgs,
    },
    Trust {
        cmd: crate::commands::trust::TrustCmd,
    },
    Hooks {
        cmd: crate::commands::hooks::HooksCmd,
    },
    Docs {
        output: Option<std::path::PathBuf>,
    },
    Completion {
        shell: crate::commands::completion::CompletionShell,
    },
    Telemetry {
        cmd: crate::commands::telemetry::TelemetryCmd,
    },
    Network {
        cmd: crate::commands::network::NetworkCmd,
    },
    Doctor {
        cmd: crate::commands::doctor::DoctorCmd,
    },
    Template {
        cmd: crate::commands::template::TemplateCmd,
    },
    Workspace {
        cmd: crate::commands::workspace::WorkspaceCmd,
    },
    Login {
        registry: Option<String>,
        username: Option<String>,
        password: Option<String>,
        local: bool,
    },
    Registry {
        cmd: crate::commands::registry::RegistryCmd,
    },
    Model {
        cmd: crate::commands::model::ModelCmd,
    },
    Mcp,
    Capabilities,
}

#[allow(clippy::large_enum_variant)]
pub enum CoreCommand {
    CreateWeb {
        framework: String,
        project_name: String,
        flags: crate::commands::core::scaffold_flags::ScaffoldFlags,
    },
    CreateGame {
        framework: String,
        project_name: String,
    },
    CreateAi {
        framework: String,
        project_name: String,
    },
    CreateClo {
        framework: String,
        project_name: String,
    },
    CreateCicd {
        framework: String,
        project_name: String,
    },
    CreateIot {
        framework: String,
        project_name: String,
    },
    CreateApp {
        framework: String,
        project_name: String,
    },
    CreateLib {
        framework: String,
        project_name: String,
    },
    CreateHardware {
        framework: String,
        project_name: String,
    },
    InstallWeb {
        packages: Vec<String>,
        frozen: bool,
        ignore_scripts: bool,
        allow_scripts: bool,
        prefer_dedupe: bool,
        repair: bool,
        offline: bool,
        compat_runtime: Option<String>,
    },
    InstallGame {
        packages: Vec<String>,
        compat_runtime: Option<String>,
    },
    InstallAi {
        packages: Vec<String>,
        dry_run: bool,
        compat_runtime: Option<String>,
        frozen: bool,
    },
    InstallClo {
        packages: Vec<String>,
        dry_run: bool,
        compat_runtime: Option<String>,
    },
    InstallCicd {
        packages: Vec<String>,
        dry_run: bool,
        compat_runtime: Option<String>,
    },
    InstallIot {
        packages: Vec<String>,
        compat_runtime: Option<String>,
    },
    InstallApp {
        packages: Vec<String>,
        dry_run: bool,
        compat_runtime: Option<String>,
        frozen: bool,
    },
    InstallLib {
        packages: Vec<String>,
        compat_runtime: Option<String>,
        frozen: bool,
        offline: bool,
    },
    InstallHardware {
        packages: Vec<String>,
        compat_runtime: Option<String>,
    },
    AddWeb {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        install: bool,
        global: bool,
        compat_runtime: Option<String>,
        version: Option<String>,
    },
    AddGame {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
        compat_runtime: Option<String>,
        version: Option<String>,
    },
    AddAi {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
        compat_runtime: Option<String>,
        version: Option<String>,
    },
    AddClo {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
        compat_runtime: Option<String>,
        version: Option<String>,
    },
    AddCicd {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
        compat_runtime: Option<String>,
        version: Option<String>,
    },
    AddIot {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
        compat_runtime: Option<String>,
        version: Option<String>,
    },
    AddApp {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
        compat_runtime: Option<String>,
        version: Option<String>,
    },
    AddLib {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
        compat_runtime: Option<String>,
        version: Option<String>,
    },
    AddHardware {
        packages: Vec<String>,
        compat_runtime: Option<String>,
        version: Option<String>,
    },
    RemoveWeb {
        packages: Vec<String>,
        install: bool,
        compat_runtime: Option<String>,
    },
    RemoveGame {
        packages: Vec<String>,
        compat_runtime: Option<String>,
    },
    RemoveAi {
        packages: Vec<String>,
        compat_runtime: Option<String>,
    },
    RemoveClo {
        packages: Vec<String>,
        compat_runtime: Option<String>,
    },
    RemoveCicd {
        packages: Vec<String>,
        compat_runtime: Option<String>,
    },
    RemoveIot {
        packages: Vec<String>,
        compat_runtime: Option<String>,
    },
    RemoveApp {
        packages: Vec<String>,
        compat_runtime: Option<String>,
    },
    RemoveLib {
        packages: Vec<String>,
        compat_runtime: Option<String>,
    },
    RemoveHardware {
        packages: Vec<String>,
        compat_runtime: Option<String>,
    },
    ListWeb {
        compat_runtime: Option<String>,
    },
    ListGame {
        compat_runtime: Option<String>,
    },
    ListAi {
        compat_runtime: Option<String>,
    },
    ListClo {
        compat_runtime: Option<String>,
    },
    ListCicd {
        compat_runtime: Option<String>,
    },
    ListIot {
        compat_runtime: Option<String>,
    },
    ListApp {
        compat_runtime: Option<String>,
    },
    ListLib {
        compat_runtime: Option<String>,
    },
    ListHardware {
        compat_runtime: Option<String>,
    },
    UpdateWeb {
        packages: Vec<String>,
        install: bool,
        compat_runtime: Option<String>,
    },
    UpdateGame {
        packages: Vec<String>,
        install: bool,
        compat_runtime: Option<String>,
    },
    UpdateAi {
        packages: Vec<String>,
        install: bool,
        compat_runtime: Option<String>,
    },
    UpdateClo {
        packages: Vec<String>,
        install: bool,
        compat_runtime: Option<String>,
    },
    UpdateCicd {
        packages: Vec<String>,
        install: bool,
        compat_runtime: Option<String>,
    },
    UpdateIot {
        packages: Vec<String>,
        install: bool,
        compat_runtime: Option<String>,
    },
    UpdateApp {
        packages: Vec<String>,
        install: bool,
        compat_runtime: Option<String>,
    },
    UpdateLib {
        packages: Vec<String>,
        install: bool,
        compat_runtime: Option<String>,
    },
    UpdateHardware {
        packages: Vec<String>,
        install: bool,
        compat_runtime: Option<String>,
    },
}

pub fn detect_ecosystem() -> anyhow::Result<Option<String>> {
    let cwd = std::env::current_dir()?;
    detect_ecosystem_at(&cwd)
}

#[cfg(test)]
#[path = "test/types.rs"]
mod tests;

fn detect_ecosystem_at(cwd: &std::path::Path) -> anyhow::Result<Option<String>> {
    use mgc_config::project::{ProjectConfig, read_regular_project_text};

    let root = ProjectConfig::find_project_root(cwd).unwrap_or_else(|| cwd.to_path_buf());

    // Explicit MGC identity always wins, but malformed/linked identity must
    // fail closed rather than falling through to a native manifest guess.
    if let Some(core) = ProjectConfig::read_core_marker(&root)? {
        return Ok(Some(core));
    }
    if let Some(config) = ProjectConfig::load(&root)?
        && !config.ecosystem.is_empty()
    {
        return Ok(Some(config.ecosystem));
    }

    // Validate a present lock using TOML syntax without guessing ownership
    // from an arbitrary `core = ...` line. The lock's package ownership is
    // multi-core data, not a single-core dispatch authority.
    let lock_path = root.join("mgc.lock");
    if let Some(content) = read_regular_project_text(&lock_path, "lockfile")? {
        let _: toml::Value = toml::from_str(&content)?;
    }

    // Preserve legacy manifest metadata, but parse every present manifest and
    // reject conflicting explicit declarations instead of choosing by order.
    let mut declared_cores = std::collections::BTreeSet::new();
    let package_json_path = root.join("package.json");
    if let Some(content) = read_regular_project_text(&package_json_path, "package manifest")? {
        let value: serde_json::Value = serde_json::from_str(&content)?;
        if let Some(core) = value
            .get("magicore")
            .and_then(|magicore| magicore.get("core"))
            .and_then(serde_json::Value::as_str)
        {
            declared_cores.insert(canonical_core_hint(core));
        }
    }

    for (manifest, label, is_cargo) in [
        (root.join("Cargo.toml"), "Cargo manifest", true),
        (root.join("pyproject.toml"), "Python manifest", false),
    ] {
        if let Some(content) = read_regular_project_text(&manifest, label)? {
            let value: toml::Value = toml::from_str(&content)?;
            let declared = if is_cargo {
                value
                    .get("package")
                    .and_then(|package| package.get("metadata"))
                    .and_then(|metadata| metadata.get("magicore"))
                    .and_then(|magicore| magicore.get("core"))
            } else {
                value
                    .get("tool")
                    .and_then(|tool| tool.get("magicore"))
                    .and_then(|magicore| magicore.get("core"))
            };
            if let Some(core) = declared.and_then(toml::Value::as_str) {
                declared_cores.insert(canonical_core_hint(core));
            }
        }
    }

    if declared_cores.len() > 1 {
        anyhow::bail!(
            "conflicting MagiCore core declarations in project manifests: {}. Set one authoritative `.mgc.core` marker or `mgc.toml` ecosystem before running core-aware commands.",
            declared_cores.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
    if let Some(core) = declared_cores.into_iter().next() {
        if !ProjectConfig::KNOWN_CORES.contains(&core.as_str()) {
            anyhow::bail!(
                "manifest declares unknown MagiCore core '{}'; use `.mgc.core` or `mgc.toml` with a supported core",
                core
            );
        }
        return Ok(Some(core));
    }

    // This shared detector maps a single unambiguous manifest, and returns an
    // error for mixed ecosystems. Detection never writes mgc.toml implicitly.
    ProjectConfig::detect_core(&root)
}

fn canonical_core_hint(core: &str) -> String {
    let core = core.trim().to_ascii_lowercase();
    if core == "cloud" {
        "clo".to_string()
    } else {
        core
    }
}
