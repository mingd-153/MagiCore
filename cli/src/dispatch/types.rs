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
        tag: Option<String>,
        access: Option<String>,
        dry_run: bool,
        json: bool,
        otp: Option<String>,
        force: bool,
        ignore_scripts: bool,
        no_git_checks: bool,
        publish_branch: Option<String>,
        batch: bool,
        report_summary: bool,
        patch: bool,
        minor: bool,
        major: bool,
        registry: Option<String>,
        token: Option<String>,
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
    },
    InstallLib {
        packages: Vec<String>,
        compat_runtime: Option<String>,
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
}

pub fn detect_ecosystem() -> anyhow::Result<Option<String>> {
    let cwd = std::env::current_dir()?;

    // 0. Try core signature marker (.mgc.core) — T9a, ưu tiên cao nhất
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Some(root) = mgc_config::project::ProjectConfig::find_project_root(&cwd)
        && let Ok(Some(core)) = mgc_config::project::ProjectConfig::read_core_marker(&root)
    {
        return Ok(Some(core));
    }

    // 1. Try mgc.toml
    let mgc_toml = cwd.join("mgc.toml");
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if mgc_toml.exists()
        && let Ok(Some(cfg)) = mgc_config::project::ProjectConfig::load(&cwd)
        && !cfg.ecosystem.is_empty()
    {
        return Ok(Some(cfg.ecosystem));
    }

    // 2. Try mgc.lock
    let lock_path = cwd.join("mgc.lock");
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if lock_path.exists()
        && let Ok(content) = std::fs::read_to_string(&lock_path)
    {
        for line in content.lines() {
            let line = line.trim();
            // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
            if let Some(val) = line.strip_prefix("core = \"")
                && let Some(eco) = val.strip_suffix('"')
                && !eco.is_empty()
            {
                return Ok(Some(eco.to_string()));
            }
        }
    }

    // 3. Try Native Manifest Injection (package.json, Cargo.toml, pyproject.toml)
    let package_json_path = cwd.join("package.json");
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if package_json_path.exists()
        && let Ok(content) = std::fs::read_to_string(&package_json_path)
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(eco) = v
            .get("magicore")
            .and_then(|m| m.get("core"))
            .and_then(|c| c.as_str())
    {
        return Ok(Some(eco.to_string()));
    }

    let cargo_toml_path = cwd.join("Cargo.toml");
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if cargo_toml_path.exists()
        && let Ok(content) = std::fs::read_to_string(&cargo_toml_path)
        && let Ok(v) = toml::from_str::<toml::Value>(&content)
        && let Some(eco) = v
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("magicore"))
            .and_then(|mgc| mgc.get("core"))
            .and_then(|c| c.as_str())
    {
        return Ok(Some(eco.to_string()));
    }

    let pyproject_toml_path = cwd.join("pyproject.toml");
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if pyproject_toml_path.exists()
        && let Ok(content) = std::fs::read_to_string(&pyproject_toml_path)
        && let Ok(v) = toml::from_str::<toml::Value>(&content)
        && let Some(eco) = v
            .get("tool")
            .and_then(|t| t.get("magicore"))
            .and_then(|mgc| mgc.get("core"))
            .and_then(|c| c.as_str())
    {
        return Ok(Some(eco.to_string()));
    }

    // Auto-detect from file presence — tự nhận core cho project đơn manifest.
    // Priority is intentionally conservative — ưu tiên manifest phổ biến nhất.
    let auto_detected = if package_json_path.exists() {
        Some("web")
    } else if cargo_toml_path.exists() {
        Some("lib")
    } else if pyproject_toml_path.exists() {
        Some("ai")
    } else {
        None
    };

    if let Some(core) = auto_detected {
        // Auto-save to mgc.toml for future runs — lưu binding nếu thư mục ghi được.
        let cfg = mgc_config::project::ProjectConfig::new(
            cwd.file_name().unwrap_or_default().to_string_lossy(),
            core,
        );
        let _ = cfg.save(&cwd);

        return Ok(Some(core.to_string()));
    }

    Ok(None)
}
