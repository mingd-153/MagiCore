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
    Audit,
    SelfUpdate,
    Run {
        script: String,
        args: Vec<String>,
    },
    Build,
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
    InstallWeb {
        packages: Vec<String>,
        frozen: bool,
        ignore_scripts: bool,
    },
    InstallGame {
        packages: Vec<String>,
    },
    InstallAi {
        packages: Vec<String>,
    },
    InstallClo {
        packages: Vec<String>,
    },
    InstallCicd {
        packages: Vec<String>,
    },
    InstallIot {
        packages: Vec<String>,
    },
    InstallApp {
        packages: Vec<String>,
    },
    InstallLib {
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
    RemoveWeb {
        package: String,
        install: bool,
    },
    RemoveGame {
        package: String,
    },
    RemoveAi {
        package: String,
    },
    RemoveClo {
        package: String,
    },
    RemoveCicd {
        package: String,
    },
    RemoveIot {
        package: String,
    },
    RemoveApp {
        package: String,
    },
    RemoveLib {
        package: String,
    },
    ListWeb,
    ListGame,
    ListAi,
    ListClo,
    ListCicd,
    ListIot,
    ListApp,
    ListLib,
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
            Commands::Audit => Some(CommonCommand::Audit),
            Commands::SelfUpdate => Some(CommonCommand::SelfUpdate),
            Commands::Run { script, args } => Some(CommonCommand::Run { script, args }),
            Commands::Build => Some(CommonCommand::Build),
            Commands::Start => Some(CommonCommand::Start),
            Commands::Exec { command, args } => Some(CommonCommand::Exec { command, args }),
            Commands::Dlx { package, args } => Some(CommonCommand::Dlx { package, args }),
            Commands::Cache {
                action,
                target,
                yes,
            } => Some(CommonCommand::Cache {
                action,
                target,
                yes,
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
            } => {
                let ecosystem = detect_ecosystem()?;
                match ecosystem.as_deref() {
                    Some("web") => SomeCore(CoreCommand::InstallWeb {
                        packages,
                        frozen,
                        ignore_scripts,
                    }),
                    Some("game") => SomeCore(CoreCommand::InstallGame { packages }),
                    Some("ai") => SomeCore(CoreCommand::InstallAi { packages }),
                    Some("clo") => SomeCore(CoreCommand::InstallClo { packages }),
                    Some("cicd") => SomeCore(CoreCommand::InstallCicd { packages }),
                    Some("iot") => SomeCore(CoreCommand::InstallIot { packages }),
                    Some("app") => SomeCore(CoreCommand::InstallApp { packages }),
                    Some("lib") => SomeCore(CoreCommand::InstallLib { packages }),
                    _ => SomeCore(CoreCommand::InstallWeb {
                        packages,
                        frozen,
                        ignore_scripts,
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
                package,
                no_install,
            } => {
                let ecosystem = detect_ecosystem()?;
                match ecosystem.as_deref() {
                    Some("web") => SomeCore(CoreCommand::RemoveWeb {
                        package,
                        install: !no_install,
                    }),
                    Some("game") => SomeCore(CoreCommand::RemoveGame { package }),
                    Some("ai") => SomeCore(CoreCommand::RemoveAi { package }),
                    Some("clo") => SomeCore(CoreCommand::RemoveClo { package }),
                    Some("cicd") => SomeCore(CoreCommand::RemoveCicd { package }),
                    Some("iot") => SomeCore(CoreCommand::RemoveIot { package }),
                    Some("app") => SomeCore(CoreCommand::RemoveApp { package }),
                    Some("lib") => SomeCore(CoreCommand::RemoveLib { package }),
                    _ => SomeCore(CoreCommand::RemoveWeb {
                        package,
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
            } => SomeCore(CoreCommand::InstallWeb {
                packages,
                frozen,
                ignore_scripts,
            }),
            Commands::InstallGame { packages } => SomeCore(CoreCommand::InstallGame { packages }),
            Commands::InstallAi { packages } => SomeCore(CoreCommand::InstallAi { packages }),
            Commands::InstallClo { packages } => SomeCore(CoreCommand::InstallClo { packages }),
            Commands::InstallCicd { packages } => SomeCore(CoreCommand::InstallCicd { packages }),
            Commands::InstallIot { packages } => SomeCore(CoreCommand::InstallIot { packages }),
            Commands::InstallApp { packages } => SomeCore(CoreCommand::InstallApp { packages }),
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
                package,
                no_install,
            } => SomeCore(CoreCommand::RemoveWeb {
                package,
                install: !no_install,
            }),
            Commands::RemoveGame { package } => SomeCore(CoreCommand::RemoveGame { package }),
            Commands::RemoveAi { package } => SomeCore(CoreCommand::RemoveAi { package }),
            Commands::RemoveClo { package } => SomeCore(CoreCommand::RemoveClo { package }),
            Commands::RemoveCicd { package } => SomeCore(CoreCommand::RemoveCicd { package }),
            Commands::RemoveIot { package } => SomeCore(CoreCommand::RemoveIot { package }),
            Commands::RemoveApp { package } => SomeCore(CoreCommand::RemoveApp { package }),
            Commands::RemoveLib { package } => SomeCore(CoreCommand::RemoveLib { package }),
            Commands::ListWeb => SomeCore(CoreCommand::ListWeb),
            Commands::ListGame => SomeCore(CoreCommand::ListGame),
            Commands::ListAi => SomeCore(CoreCommand::ListAi),
            Commands::ListClo => SomeCore(CoreCommand::ListClo),
            Commands::ListCicd => SomeCore(CoreCommand::ListCicd),
            Commands::ListIot => SomeCore(CoreCommand::ListIot),
            Commands::ListApp => SomeCore(CoreCommand::ListApp),
            Commands::ListLib => SomeCore(CoreCommand::ListLib),
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

        println!("");
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
