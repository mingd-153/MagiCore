use crate::Commands;

#[allow(clippy::large_enum_variant)]
pub enum DispatchCommand {
    Common(CommonCommand),
    Core(CoreCommand),
}

pub enum CommonCommand {
    Init {
        template: Option<String>,
    },
    Dev {
        host: Option<String>,
        port: Option<u16>,
        clear: bool,
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
    },
    SelfUpdate,
    Run {
        script: String,
        args: Vec<String>,
    },
    Build {
        target: Option<String>,
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
    Telemetry {
        cmd: crate::commands::telemetry::TelemetryCmd,
    },
    Network {
        cmd: crate::commands::network::NetworkCmd,
    },
    Doctor {
        cmd: crate::commands::doctor::DoctorCmd,
    },
    Sbom {
        output: Option<String>,
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
    },
    InstallGame {
        packages: Vec<String>,
    },
    InstallAi {
        packages: Vec<String>,
        dry_run: bool,
    },
    InstallClo {
        packages: Vec<String>,
        dry_run: bool,
    },
    InstallCicd {
        packages: Vec<String>,
        dry_run: bool,
    },
    InstallIot {
        packages: Vec<String>,
    },
    InstallApp {
        packages: Vec<String>,
        dry_run: bool,
    },
    InstallLib {
        packages: Vec<String>,
    },
    InstallHardware {
        packages: Vec<String>,
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
    },
    AddGame {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    AddAi {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    AddClo {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    AddCicd {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    AddIot {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    AddApp {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    AddLib {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    AddHardware {
        packages: Vec<String>,
    },
    RemoveWeb {
        packages: Vec<String>,
        install: bool,
    },
    RemoveGame {
        packages: Vec<String>,
    },
    RemoveAi {
        packages: Vec<String>,
    },
    RemoveClo {
        packages: Vec<String>,
    },
    RemoveCicd {
        packages: Vec<String>,
    },
    RemoveIot {
        packages: Vec<String>,
    },
    RemoveApp {
        packages: Vec<String>,
    },
    RemoveLib {
        packages: Vec<String>,
    },
    ListWeb,
    ListGame,
    ListAi,
    ListClo,
    ListCicd,
    ListIot,
    ListApp,
    ListLib,
    ListHardware,
    UpdateWeb {
        packages: Vec<String>,
        install: bool,
    },
    UpdateGame {
        packages: Vec<String>,
        install: bool,
    },
    UpdateAi {
        packages: Vec<String>,
        install: bool,
    },
    UpdateClo {
        packages: Vec<String>,
        install: bool,
    },
    UpdateCicd {
        packages: Vec<String>,
        install: bool,
    },
    UpdateIot {
        packages: Vec<String>,
        install: bool,
    },
    UpdateApp {
        packages: Vec<String>,
        install: bool,
    },
    UpdateLib {
        packages: Vec<String>,
        install: bool,
    },
}

impl TryFrom<Commands> for DispatchCommand {
    type Error = anyhow::Error;

    fn try_from(command: Commands) -> Result<Self, Self::Error> {
        use DispatchCommand::{Common as SomeCommon, Core as SomeCore};

        let common_cmd = match command.clone() {
            Commands::Init { template } => Some(CommonCommand::Init { template }),
            Commands::Dev { host, port, clear } => Some(CommonCommand::Dev { host, port, clear }),
            Commands::Info { package, json } => Some(CommonCommand::Info { package, json }),
            Commands::Search {
                query,
                json,
                exact,
                page,
            } => Some(CommonCommand::Search {
                query,
                json,
                exact,
                page,
            }),
            Commands::Outdated { json } => Some(CommonCommand::Outdated { json }),
            Commands::Audit { fix } => Some(CommonCommand::Audit { fix }),
            Commands::SelfUpdate => Some(CommonCommand::SelfUpdate),
            Commands::Publish {
                tag,
                access,
                dry_run,
                json,
                otp,
                force,
                ignore_scripts,
                no_git_checks,
                publish_branch,
                batch,
                report_summary,
                patch,
                minor,
                major,
                registry,
                token,
            } => Some(CommonCommand::Publish {
                tag,
                access,
                dry_run,
                json,
                otp,
                force,
                ignore_scripts,
                no_git_checks,
                publish_branch,
                batch,
                report_summary,
                patch,
                minor,
                major,
                registry,
                token,
            }),
            Commands::Patch { cmd } => Some(CommonCommand::Patch { cmd }),
            Commands::Dedupe {
                dry_run,
                prefer_latest,
                json,
            } => Some(CommonCommand::Dedupe {
                dry_run,
                prefer_latest,
                json,
            }),
            Commands::Store { cmd } => Some(CommonCommand::Store { cmd }),
            Commands::Bench { args } => Some(CommonCommand::Bench { args }),
            Commands::Trust { cmd } => Some(CommonCommand::Trust { cmd }),
            Commands::Hooks { cmd } => Some(CommonCommand::Hooks { cmd }),
            Commands::Docs { output } => Some(CommonCommand::Docs { output }),
            Commands::Telemetry { cmd } => Some(CommonCommand::Telemetry { cmd }),
            Commands::Network { cmd } => Some(CommonCommand::Network { cmd }),
            Commands::Doctor { cmd } => Some(CommonCommand::Doctor { cmd }),
            Commands::Sbom { output } => Some(CommonCommand::Sbom { output }),
            Commands::Template { cmd } => Some(CommonCommand::Template { cmd }),
            Commands::Workspace { cmd } => Some(CommonCommand::Workspace { cmd }),
            Commands::Login {
                registry,
                username,
                password,
                local,
            } => Some(CommonCommand::Login {
                registry,
                username,
                password,
                local,
            }),
            Commands::Registry { cmd } => Some(CommonCommand::Registry { cmd }),
            Commands::Model { cmd } => Some(CommonCommand::Model { cmd }),
            Commands::Run { script, args } => Some(CommonCommand::Run { script, args }),
            Commands::Build { target } => Some(CommonCommand::Build { target }),
            Commands::Flash { board, skip_build } => {
                Some(CommonCommand::Flash { board, skip_build })
            }
            Commands::Deploy { run } => Some(CommonCommand::Deploy { run }),
            Commands::CiGenerate => Some(CommonCommand::CiGenerate),
            Commands::Verify => Some(CommonCommand::Verify),
            Commands::Start => Some(CommonCommand::Start),
            Commands::Exec { command, args } => Some(CommonCommand::Exec { command, args }),
            Commands::Dlx { package, args } => Some(CommonCommand::Dlx { package, args }),
            Commands::Cache {
                action,
                target,
                yes,
                dry_run,
            } => Some(CommonCommand::Cache {
                action,
                target,
                yes,
                dry_run,
            }),
            Commands::Link { package } => Some(CommonCommand::Link { package }),
            Commands::Unlink { package } => Some(CommonCommand::Unlink { package }),
            Commands::Why { package } => Some(CommonCommand::Why { package }),
            _ => None,
        };

        if let Some(cmd) = common_cmd {
            return Ok(SomeCommon(cmd));
        }

        Ok(match command {
            // ── Bare commands (auto-detect from .megagate/) ──
            Commands::Install {
                packages,
                frozen,
                ignore_scripts,
                allow_scripts,
                prefer_dedupe,
                repair,
                dry_run,
            } => {
                let ecosystem = detect_ecosystem()?;
                match ecosystem.as_deref() {
                    Some("web") => SomeCore(CoreCommand::InstallWeb {
                        packages,
                        frozen,
                        ignore_scripts,
                        allow_scripts,
                        prefer_dedupe,
                        repair,
                    }),
                    Some("game") => SomeCore(CoreCommand::InstallGame { packages }),
                    Some("ai") => SomeCore(CoreCommand::InstallAi { packages, dry_run }),
                    Some("clo") => SomeCore(CoreCommand::InstallClo { packages, dry_run }),
                    Some("cicd") => SomeCore(CoreCommand::InstallCicd { packages, dry_run }),
                    Some("iot") => SomeCore(CoreCommand::InstallIot { packages }),
                    Some("app") => SomeCore(CoreCommand::InstallApp { packages, dry_run }),
                    Some("lib") => SomeCore(CoreCommand::InstallLib { packages }),
                    _ => SomeCore(CoreCommand::InstallWeb {
                        packages,
                        frozen,
                        ignore_scripts,
                        allow_scripts,
                        prefer_dedupe,
                        repair,
                    }),
                }
            }
            Commands::Add {
                packages,
                dev,
                global,
                exact,
                optional,
                peer,
                no_save,
                no_install,
                ..
            } => {
                let ecosystem = detect_ecosystem()?;
                match ecosystem.as_deref() {
                    Some("web") => SomeCore(CoreCommand::AddWeb {
                        packages,
                        dev,
                        exact,
                        optional,
                        peer,
                        no_save,
                        install: !no_install,
                        global,
                    }),
                    Some("game") => SomeCore(CoreCommand::AddGame {
                        packages,
                        dev,
                        exact,
                        optional,
                        peer,
                        no_save,
                        global,
                    }),
                    Some("ai") => SomeCore(CoreCommand::AddAi {
                        packages,
                        dev,
                        exact,
                        optional,
                        peer,
                        no_save,
                        global,
                    }),
                    Some("clo") => SomeCore(CoreCommand::AddClo {
                        packages,
                        dev,
                        exact,
                        optional,
                        peer,
                        no_save,
                        global,
                    }),
                    Some("cicd") => SomeCore(CoreCommand::AddCicd {
                        packages,
                        dev,
                        exact,
                        optional,
                        peer,
                        no_save,
                        global,
                    }),
                    Some("iot") => SomeCore(CoreCommand::AddIot {
                        packages,
                        dev,
                        exact,
                        optional,
                        peer,
                        no_save,
                        global,
                    }),
                    Some("app") => SomeCore(CoreCommand::AddApp {
                        packages,
                        dev,
                        exact,
                        optional,
                        peer,
                        no_save,
                        global,
                    }),
                    Some("lib") => SomeCore(CoreCommand::AddLib {
                        packages,
                        dev,
                        exact,
                        optional,
                        peer,
                        no_save,
                        global,
                    }),
                    _ => SomeCore(CoreCommand::AddWeb {
                        packages,
                        dev,
                        exact,
                        optional,
                        peer,
                        no_save,
                        install: !no_install,
                        global,
                    }),
                }
            }
            Commands::Remove {
                packages,
                no_install,
            } => {
                let ecosystem = detect_ecosystem()?;
                match ecosystem.as_deref() {
                    Some("web") => SomeCore(CoreCommand::RemoveWeb {
                        packages,
                        install: !no_install,
                    }),
                    Some("game") => SomeCore(CoreCommand::RemoveGame { packages }),
                    Some("ai") => SomeCore(CoreCommand::RemoveAi { packages }),
                    Some("clo") => SomeCore(CoreCommand::RemoveClo { packages }),
                    Some("cicd") => SomeCore(CoreCommand::RemoveCicd { packages }),
                    Some("iot") => SomeCore(CoreCommand::RemoveIot { packages }),
                    Some("app") => SomeCore(CoreCommand::RemoveApp { packages }),
                    Some("lib") => SomeCore(CoreCommand::RemoveLib { packages }),
                    _ => SomeCore(CoreCommand::RemoveWeb {
                        packages,
                        install: !no_install,
                    }),
                }
            }
            Commands::List => {
                let ecosystem = detect_ecosystem()?;
                match ecosystem.as_deref() {
                    Some("web") => SomeCore(CoreCommand::ListWeb),
                    Some("game") => SomeCore(CoreCommand::ListGame),
                    Some("ai") => SomeCore(CoreCommand::ListAi),
                    Some("clo") => SomeCore(CoreCommand::ListClo),
                    Some("cicd") => SomeCore(CoreCommand::ListCicd),
                    Some("iot") => SomeCore(CoreCommand::ListIot),
                    Some("app") => SomeCore(CoreCommand::ListApp),
                    Some("lib") => SomeCore(CoreCommand::ListLib),
                    Some("hardware") => SomeCore(CoreCommand::ListHardware),
                    _ => SomeCore(CoreCommand::ListWeb),
                }
            }
            Commands::Update { packages, install } => {
                let ecosystem = detect_ecosystem()?;
                match ecosystem.as_deref() {
                    Some("web") => SomeCore(CoreCommand::UpdateWeb { packages, install }),
                    Some("game") => SomeCore(CoreCommand::UpdateGame { packages, install }),
                    Some("ai") => SomeCore(CoreCommand::UpdateAi { packages, install }),
                    Some("clo") => SomeCore(CoreCommand::UpdateClo { packages, install }),
                    Some("cicd") => SomeCore(CoreCommand::UpdateCicd { packages, install }),
                    Some("iot") => SomeCore(CoreCommand::UpdateIot { packages, install }),
                    Some("app") => SomeCore(CoreCommand::UpdateApp { packages, install }),
                    Some("lib") => SomeCore(CoreCommand::UpdateLib { packages, install }),
                    _ => SomeCore(CoreCommand::UpdateWeb { packages, install }),
                }
            }
            Commands::CreateWeb {
                framework,
                project_name,
                flags,
            } => SomeCore(CoreCommand::CreateWeb {
                framework,
                project_name,
                flags,
            }),
            Commands::CreateGame {
                framework,
                project_name,
            } => SomeCore(CoreCommand::CreateGame {
                framework,
                project_name,
            }),
            Commands::CreateAi {
                framework,
                project_name,
            } => SomeCore(CoreCommand::CreateAi {
                framework,
                project_name,
            }),
            Commands::CreateClo {
                framework,
                project_name,
            } => SomeCore(CoreCommand::CreateClo {
                framework,
                project_name,
            }),
            Commands::CreateCicd {
                framework,
                project_name,
            } => SomeCore(CoreCommand::CreateCicd {
                framework,
                project_name,
            }),
            Commands::CreateIot {
                framework,
                project_name,
            } => SomeCore(CoreCommand::CreateIot {
                framework,
                project_name,
            }),
            Commands::CreateApp {
                framework,
                project_name,
            } => SomeCore(CoreCommand::CreateApp {
                framework,
                project_name,
            }),
            Commands::CreateLib { project_name } => {
                SomeCore(CoreCommand::CreateLib { project_name })
            }
            Commands::InstallWeb {
                packages,
                frozen,
                ignore_scripts,
                allow_scripts,
                prefer_dedupe,
                repair,
            } => SomeCore(CoreCommand::InstallWeb {
                packages,
                frozen,
                ignore_scripts,
                allow_scripts,
                prefer_dedupe,
                repair,
            }),
            Commands::InstallGame { packages } => SomeCore(CoreCommand::InstallGame { packages }),
            Commands::InstallAi { packages, dry_run } => SomeCore(CoreCommand::InstallAi { packages, dry_run }),
            Commands::InstallClo { packages } => SomeCore(CoreCommand::InstallClo {
                packages,
                dry_run: false,
            }),
            Commands::InstallCicd { packages } => SomeCore(CoreCommand::InstallCicd {
                packages,
                dry_run: false,
            }),
            Commands::InstallIot { packages } => SomeCore(CoreCommand::InstallIot { packages }),
            Commands::InstallApp { packages } => SomeCore(CoreCommand::InstallApp {
                packages,
                dry_run: false,
            }),
            Commands::InstallLib { packages } => SomeCore(CoreCommand::InstallLib { packages }),
            Commands::AddWeb {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                no_install,
                global,
            } => SomeCore(CoreCommand::AddWeb {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                install: !no_install,
                global,
            }),
            Commands::AddGame {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            } => SomeCore(CoreCommand::AddGame {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            }),
            Commands::AddAi {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            } => SomeCore(CoreCommand::AddAi {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            }),
            Commands::AddClo {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            } => SomeCore(CoreCommand::AddClo {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            }),
            Commands::AddCicd {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            } => SomeCore(CoreCommand::AddCicd {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            }),
            Commands::AddIot {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            } => SomeCore(CoreCommand::AddIot {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            }),
            Commands::AddApp {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            } => SomeCore(CoreCommand::AddApp {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            }),
            Commands::AddLib {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            } => SomeCore(CoreCommand::AddLib {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            }),
            Commands::RemoveWeb {
                packages,
                no_install,
            } => SomeCore(CoreCommand::RemoveWeb {
                packages,
                install: !no_install,
            }),
            Commands::RemoveGame { packages } => SomeCore(CoreCommand::RemoveGame { packages }),
            Commands::RemoveAi { packages } => SomeCore(CoreCommand::RemoveAi { packages }),
            Commands::RemoveClo { packages } => SomeCore(CoreCommand::RemoveClo { packages }),
            Commands::RemoveCicd { packages } => SomeCore(CoreCommand::RemoveCicd { packages }),
            Commands::RemoveIot { packages } => SomeCore(CoreCommand::RemoveIot { packages }),
            Commands::RemoveApp { packages } => SomeCore(CoreCommand::RemoveApp { packages }),
            Commands::RemoveLib { packages } => SomeCore(CoreCommand::RemoveLib { packages }),
            Commands::ListWeb => SomeCore(CoreCommand::ListWeb),
            Commands::ListGame => SomeCore(CoreCommand::ListGame),
            Commands::ListAi => SomeCore(CoreCommand::ListAi),
            Commands::ListClo => SomeCore(CoreCommand::ListClo),
            Commands::ListCicd => SomeCore(CoreCommand::ListCicd),
            Commands::ListIot => SomeCore(CoreCommand::ListIot),
            Commands::ListApp => SomeCore(CoreCommand::ListApp),
            Commands::ListLib => SomeCore(CoreCommand::ListLib),
            Commands::ListHardware => SomeCore(CoreCommand::ListHardware),
            Commands::UpdateWeb { packages, install } => {
                SomeCore(CoreCommand::UpdateWeb { packages, install })
            }
            Commands::UpdateGame { packages, install } => {
                SomeCore(CoreCommand::UpdateGame { packages, install })
            }
            Commands::UpdateAi { packages, install } => {
                SomeCore(CoreCommand::UpdateAi { packages, install })
            }
            Commands::UpdateClo { packages, install } => {
                SomeCore(CoreCommand::UpdateClo { packages, install })
            }
            Commands::UpdateCicd { packages, install } => {
                SomeCore(CoreCommand::UpdateCicd { packages, install })
            }
            Commands::UpdateIot { packages, install } => {
                SomeCore(CoreCommand::UpdateIot { packages, install })
            }
            Commands::UpdateApp { packages, install } => {
                SomeCore(CoreCommand::UpdateApp { packages, install })
            }
            Commands::UpdateLib { packages, install } => {
                SomeCore(CoreCommand::UpdateLib { packages, install })
            }
            _ => unreachable!("Common commands should be handled by the first match block"),
        })
    }
}

pub fn detect_ecosystem() -> anyhow::Result<Option<String>> {
    let cwd = std::env::current_dir()?;

    // 1. Try mg.toml
    let mg_toml = cwd.join("mg.toml");
    if mg_toml.exists() {
        if let Ok(cfg) = mg_config::project::ProjectConfig::load(&cwd) {
            if let Some(cfg) = cfg {
                if !cfg.ecosystem.is_empty() {
                    return Ok(Some(cfg.ecosystem));
                }
            }
        }
    }

    // 2. Try mg.lock
    let lock_path = cwd.join("mg.lock");
    if lock_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&lock_path) {
            for line in content.lines() {
                let line = line.trim();
                if let Some(val) = line.strip_prefix("core = \"") {
                    if let Some(eco) = val.strip_suffix('"') {
                        if !eco.is_empty() {
                            return Ok(Some(eco.to_string()));
                        }
                    }
                }
            }
        }
    }

    // 3. Try Native Manifest Injection (package.json, Cargo.toml, pyproject.toml)
    let package_json_path = cwd.join("package.json");
    if package_json_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&package_json_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(eco) = v
                    .get("megagate")
                    .and_then(|m| m.get("core"))
                    .and_then(|c| c.as_str())
                {
                    return Ok(Some(eco.to_string()));
                }
            }
        }
    }

    let cargo_toml_path = cwd.join("Cargo.toml");
    if cargo_toml_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&cargo_toml_path) {
            if let Ok(v) = toml::from_str::<toml::Value>(&content) {
                if let Some(eco) = v
                    .get("package")
                    .and_then(|p| p.get("metadata"))
                    .and_then(|m| m.get("megagate"))
                    .and_then(|mg| mg.get("core"))
                    .and_then(|c| c.as_str())
                {
                    return Ok(Some(eco.to_string()));
                }
            }
        }
    }

    let pyproject_toml_path = cwd.join("pyproject.toml");
    if pyproject_toml_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&pyproject_toml_path) {
            if let Ok(v) = toml::from_str::<toml::Value>(&content) {
                if let Some(eco) = v
                    .get("tool")
                    .and_then(|t| t.get("megagate"))
                    .and_then(|mg| mg.get("core"))
                    .and_then(|c| c.as_str())
                {
                    return Ok(Some(eco.to_string()));
                }
            }
        }
    }

    // 4. Interactive prompt for missing ecosystem
    if package_json_path.exists() || cargo_toml_path.exists() || pyproject_toml_path.exists() {
        let items = crate::factory::available_cores();
        let display_items: Vec<&str> = items.iter().map(|(_, label)| *label).collect();

        mg_ui::blank_line();
        mg_ui::info("MegaGate detected a project without a bound core.");
        let selected_idx = mg_ui::prompt::select(
            "Which ecosystem does this project belong to?",
            &display_items,
        )?;
        let selected_core = items[selected_idx].0;

        let cfg = mg_config::project::ProjectConfig::new(
            cwd.file_name().unwrap_or_default().to_string_lossy(),
            selected_core,
        );
        cfg.save(&cwd)?;
        mg_ui::info("Saved core binding to mg.toml");

        return Ok(Some(selected_core.to_string()));
    }

    Ok(None)
}
