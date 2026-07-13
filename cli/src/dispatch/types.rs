use crate::Commands;

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
    },
    Info {
        package: String,
    },
    Search {
        query: String,
    },
    Outdated,
    Audit,
}

pub enum CoreCommand {
    #[cfg(all(
        feature = "web",
        not(any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        ))
    ))]
    Install { packages: Vec<String>, frozen: bool },
    #[cfg(all(
        feature = "web",
        not(any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        ))
    ))]
    Add {
        packages: Vec<String>,
        version: Option<String>,
        dev: bool,
        global: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
    },
    #[cfg(all(
        feature = "web",
        not(any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        ))
    ))]
    Remove { package: String },
    #[cfg(all(
        feature = "web",
        not(any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        ))
    ))]
    Update { packages: Vec<String> },
    #[cfg(all(
        feature = "web",
        not(any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        ))
    ))]
    List,
    #[cfg(feature = "web")]
    CreateWeb {
        framework: String,
        project_name: String,
        ts: bool,
        tailwindcss: bool,
        monorepo: bool,
        backend: Option<String>,
        features: Vec<String>,
    },
    #[cfg(feature = "game")]
    CreateGame {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "ai")]
    CreateAi {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "clo")]
    CreateClo {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "cicd")]
    CreateCicd {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "iot")]
    CreateIot {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "app")]
    CreateApp {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "lib")]
    CreateLib { project_name: String },
    #[cfg(all(
        feature = "web",
        any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        )
    ))]
    InstallWeb { packages: Vec<String>, frozen: bool },
    #[cfg(feature = "game")]
    InstallGame { packages: Vec<String> },
    #[cfg(feature = "ai")]
    InstallAi { packages: Vec<String> },
    #[cfg(feature = "clo")]
    InstallClo { packages: Vec<String> },
    #[cfg(feature = "cicd")]
    InstallCicd { packages: Vec<String> },
    #[cfg(feature = "iot")]
    InstallIot { packages: Vec<String> },
    #[cfg(feature = "app")]
    InstallApp { packages: Vec<String> },
    #[cfg(feature = "lib")]
    InstallLib { packages: Vec<String> },
    #[cfg(all(
        feature = "web",
        any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        )
    ))]
    AddWeb {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    #[cfg(feature = "game")]
    AddGame {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    #[cfg(feature = "ai")]
    AddAi {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    #[cfg(feature = "clo")]
    AddClo {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    #[cfg(feature = "cicd")]
    AddCicd {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    #[cfg(feature = "iot")]
    AddIot {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    #[cfg(feature = "app")]
    AddApp {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    #[cfg(feature = "lib")]
    AddLib {
        packages: Vec<String>,
        dev: bool,
        exact: bool,
        optional: bool,
        peer: bool,
        no_save: bool,
        global: bool,
    },
    #[cfg(all(
        feature = "web",
        any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        )
    ))]
    RemoveWeb { package: String },
    #[cfg(feature = "game")]
    RemoveGame { package: String },
    #[cfg(feature = "ai")]
    RemoveAi { package: String },
    #[cfg(feature = "clo")]
    RemoveClo { package: String },
    #[cfg(feature = "cicd")]
    RemoveCicd { package: String },
    #[cfg(feature = "iot")]
    RemoveIot { package: String },
    #[cfg(feature = "app")]
    RemoveApp { package: String },
    #[cfg(feature = "lib")]
    RemoveLib { package: String },
    #[cfg(all(
        feature = "web",
        any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        )
    ))]
    ListWeb,
    #[cfg(feature = "game")]
    ListGame,
    #[cfg(feature = "ai")]
    ListAi,
    #[cfg(feature = "clo")]
    ListClo,
    #[cfg(feature = "cicd")]
    ListCicd,
    #[cfg(feature = "iot")]
    ListIot,
    #[cfg(feature = "app")]
    ListApp,
    #[cfg(feature = "lib")]
    ListLib,
    #[cfg(all(
        feature = "web",
        any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        )
    ))]
    UpdateWeb { packages: Vec<String> },
    #[cfg(feature = "game")]
    UpdateGame { packages: Vec<String> },
    #[cfg(feature = "ai")]
    UpdateAi { packages: Vec<String> },
    #[cfg(feature = "clo")]
    UpdateClo { packages: Vec<String> },
    #[cfg(feature = "cicd")]
    UpdateCicd { packages: Vec<String> },
    #[cfg(feature = "iot")]
    UpdateIot { packages: Vec<String> },
    #[cfg(feature = "app")]
    UpdateApp { packages: Vec<String> },
    #[cfg(feature = "lib")]
    UpdateLib { packages: Vec<String> },
}

impl From<Commands> for DispatchCommand {
    fn from(command: Commands) -> Self {
        use DispatchCommand::{Common as SomeCommon, Core as SomeCore};

        match command {
            Commands::Init { template } => SomeCommon(CommonCommand::Init { template }),
            Commands::Dev { host, port } => SomeCommon(CommonCommand::Dev { host, port }),
            Commands::Info { package } => SomeCommon(CommonCommand::Info { package }),
            Commands::Search { query } => SomeCommon(CommonCommand::Search { query }),
            Commands::Outdated => SomeCommon(CommonCommand::Outdated),
            Commands::Audit => SomeCommon(CommonCommand::Audit),
            #[cfg(all(
                feature = "web",
                not(any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                ))
            ))]
            Commands::Install {
                packages, frozen, ..
            } => SomeCore(CoreCommand::Install { packages, frozen }),
            #[cfg(all(
                feature = "web",
                not(any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                ))
            ))]
            Commands::Add {
                packages,
                version,
                dev,
                global,
                exact,
                optional,
                peer,
                no_save,
            } => SomeCore(CoreCommand::Add {
                packages,
                version,
                dev,
                global,
                exact,
                optional,
                peer,
                no_save,
            }),
            #[cfg(all(
                feature = "web",
                not(any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                ))
            ))]
            Commands::Remove { package } => SomeCore(CoreCommand::Remove { package }),
            #[cfg(all(
                feature = "web",
                not(any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                ))
            ))]
            Commands::Update { packages } => SomeCore(CoreCommand::Update { packages }),
            #[cfg(all(
                feature = "web",
                not(any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                ))
            ))]
            Commands::List => SomeCore(CoreCommand::List),
            // ── Bare commands (multi-core: auto-detect from .megagate/) ──
            #[cfg(all(
                feature = "web",
                any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                )
            ))]
            Commands::Install {
                packages, frozen, ..
            } => {
                let ecosystem = detect_ecosystem();
                match ecosystem.as_deref() {
                    Some("web") => SomeCore(CoreCommand::InstallWeb { packages, frozen }),
                    Some("game") => SomeCore(CoreCommand::InstallGame { packages }),
                    Some("ai") => SomeCore(CoreCommand::InstallAi { packages }),
                    Some("clo") => SomeCore(CoreCommand::InstallClo { packages }),
                    Some("cicd") => SomeCore(CoreCommand::InstallCicd { packages }),
                    Some("iot") => SomeCore(CoreCommand::InstallIot { packages }),
                    Some("app") => SomeCore(CoreCommand::InstallApp { packages }),
                    Some("lib") => SomeCore(CoreCommand::InstallLib { packages }),
                    _ => SomeCore(CoreCommand::InstallWeb { packages, frozen }),
                }
            }
            #[cfg(all(
                feature = "web",
                any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                )
            ))]
            Commands::Add {
                packages,
                dev,
                global,
                exact,
                optional,
                peer,
                no_save,
                ..
            } => {
                let ecosystem = detect_ecosystem();
                match ecosystem.as_deref() {
                    Some("web") => SomeCore(CoreCommand::AddWeb {
                        packages,
                        dev,
                        exact,
                        optional,
                        peer,
                        no_save,
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
                        global,
                    }),
                }
            }
            #[cfg(all(
                feature = "web",
                any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                )
            ))]
            Commands::Remove { package } => {
                let ecosystem = detect_ecosystem();
                match ecosystem.as_deref() {
                    Some("web") => SomeCore(CoreCommand::RemoveWeb { package }),
                    Some("game") => SomeCore(CoreCommand::RemoveGame { package }),
                    Some("ai") => SomeCore(CoreCommand::RemoveAi { package }),
                    Some("clo") => SomeCore(CoreCommand::RemoveClo { package }),
                    Some("cicd") => SomeCore(CoreCommand::RemoveCicd { package }),
                    Some("iot") => SomeCore(CoreCommand::RemoveIot { package }),
                    Some("app") => SomeCore(CoreCommand::RemoveApp { package }),
                    Some("lib") => SomeCore(CoreCommand::RemoveLib { package }),
                    _ => SomeCore(CoreCommand::RemoveWeb { package }),
                }
            }
            #[cfg(all(
                feature = "web",
                any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                )
            ))]
            Commands::List => {
                let ecosystem = detect_ecosystem();
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
            #[cfg(all(
                feature = "web",
                any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                )
            ))]
            Commands::Update { packages } => {
                let ecosystem = detect_ecosystem();
                match ecosystem.as_deref() {
                    Some("web") => SomeCore(CoreCommand::UpdateWeb { packages }),
                    Some("game") => SomeCore(CoreCommand::UpdateGame { packages }),
                    Some("ai") => SomeCore(CoreCommand::UpdateAi { packages }),
                    Some("clo") => SomeCore(CoreCommand::UpdateClo { packages }),
                    Some("cicd") => SomeCore(CoreCommand::UpdateCicd { packages }),
                    Some("iot") => SomeCore(CoreCommand::UpdateIot { packages }),
                    Some("app") => SomeCore(CoreCommand::UpdateApp { packages }),
                    Some("lib") => SomeCore(CoreCommand::UpdateLib { packages }),
                    _ => SomeCore(CoreCommand::UpdateWeb { packages }),
                }
            }
            #[cfg(feature = "web")]
            Commands::CreateWeb {
                framework,
                project_name,
                ts,
                tailwindcss,
                monorepo,
                backend,
                features,
            } => SomeCore(CoreCommand::CreateWeb {
                framework,
                project_name,
                ts,
                tailwindcss,
                monorepo,
                backend,
                features,
            }),
            #[cfg(feature = "game")]
            Commands::CreateGame {
                framework,
                project_name,
            } => SomeCore(CoreCommand::CreateGame {
                framework,
                project_name,
            }),
            #[cfg(feature = "ai")]
            Commands::CreateAi {
                framework,
                project_name,
            } => SomeCore(CoreCommand::CreateAi {
                framework,
                project_name,
            }),
            #[cfg(feature = "clo")]
            Commands::CreateClo {
                framework,
                project_name,
            } => SomeCore(CoreCommand::CreateClo {
                framework,
                project_name,
            }),
            #[cfg(feature = "cicd")]
            Commands::CreateCicd {
                framework,
                project_name,
            } => SomeCore(CoreCommand::CreateCicd {
                framework,
                project_name,
            }),
            #[cfg(feature = "iot")]
            Commands::CreateIot {
                framework,
                project_name,
            } => SomeCore(CoreCommand::CreateIot {
                framework,
                project_name,
            }),
            #[cfg(feature = "app")]
            Commands::CreateApp {
                framework,
                project_name,
            } => SomeCore(CoreCommand::CreateApp {
                framework,
                project_name,
            }),
            #[cfg(feature = "lib")]
            Commands::CreateLib { project_name } => {
                SomeCore(CoreCommand::CreateLib { project_name })
            }
            #[cfg(all(
                feature = "web",
                any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                )
            ))]
            Commands::InstallWeb { packages, frozen } => {
                SomeCore(CoreCommand::InstallWeb { packages, frozen })
            }
            #[cfg(feature = "game")]
            Commands::InstallGame { packages } => SomeCore(CoreCommand::InstallGame { packages }),
            #[cfg(feature = "ai")]
            Commands::InstallAi { packages } => SomeCore(CoreCommand::InstallAi { packages }),
            #[cfg(feature = "clo")]
            Commands::InstallClo { packages } => SomeCore(CoreCommand::InstallClo { packages }),
            #[cfg(feature = "cicd")]
            Commands::InstallCicd { packages } => SomeCore(CoreCommand::InstallCicd { packages }),
            #[cfg(feature = "iot")]
            Commands::InstallIot { packages } => SomeCore(CoreCommand::InstallIot { packages }),
            #[cfg(feature = "app")]
            Commands::InstallApp { packages } => SomeCore(CoreCommand::InstallApp { packages }),
            #[cfg(feature = "lib")]
            Commands::InstallLib { packages } => SomeCore(CoreCommand::InstallLib { packages }),
            #[cfg(all(
                feature = "web",
                any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                )
            ))]
            Commands::AddWeb {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            } => SomeCore(CoreCommand::AddWeb {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            }),
            #[cfg(feature = "game")]
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
            #[cfg(feature = "ai")]
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
            #[cfg(feature = "clo")]
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
            #[cfg(feature = "cicd")]
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
            #[cfg(feature = "iot")]
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
            #[cfg(feature = "app")]
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
            #[cfg(feature = "lib")]
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
            #[cfg(all(
                feature = "web",
                any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                )
            ))]
            Commands::RemoveWeb { package } => SomeCore(CoreCommand::RemoveWeb { package }),
            #[cfg(feature = "game")]
            Commands::RemoveGame { package } => SomeCore(CoreCommand::RemoveGame { package }),
            #[cfg(feature = "ai")]
            Commands::RemoveAi { package } => SomeCore(CoreCommand::RemoveAi { package }),
            #[cfg(feature = "clo")]
            Commands::RemoveClo { package } => SomeCore(CoreCommand::RemoveClo { package }),
            #[cfg(feature = "cicd")]
            Commands::RemoveCicd { package } => SomeCore(CoreCommand::RemoveCicd { package }),
            #[cfg(feature = "iot")]
            Commands::RemoveIot { package } => SomeCore(CoreCommand::RemoveIot { package }),
            #[cfg(feature = "app")]
            Commands::RemoveApp { package } => SomeCore(CoreCommand::RemoveApp { package }),
            #[cfg(feature = "lib")]
            Commands::RemoveLib { package } => SomeCore(CoreCommand::RemoveLib { package }),
            #[cfg(all(
                feature = "web",
                any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                )
            ))]
            Commands::ListWeb => SomeCore(CoreCommand::ListWeb),
            #[cfg(feature = "game")]
            Commands::ListGame => SomeCore(CoreCommand::ListGame),
            #[cfg(feature = "ai")]
            Commands::ListAi => SomeCore(CoreCommand::ListAi),
            #[cfg(feature = "clo")]
            Commands::ListClo => SomeCore(CoreCommand::ListClo),
            #[cfg(feature = "cicd")]
            Commands::ListCicd => SomeCore(CoreCommand::ListCicd),
            #[cfg(feature = "iot")]
            Commands::ListIot => SomeCore(CoreCommand::ListIot),
            #[cfg(feature = "app")]
            Commands::ListApp => SomeCore(CoreCommand::ListApp),
            #[cfg(feature = "lib")]
            Commands::ListLib => SomeCore(CoreCommand::ListLib),
            #[cfg(all(
                feature = "web",
                any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                )
            ))]
            Commands::UpdateWeb { packages } => SomeCore(CoreCommand::UpdateWeb { packages }),
            #[cfg(feature = "game")]
            Commands::UpdateGame { packages } => SomeCore(CoreCommand::UpdateGame { packages }),
            #[cfg(feature = "ai")]
            Commands::UpdateAi { packages } => SomeCore(CoreCommand::UpdateAi { packages }),
            #[cfg(feature = "clo")]
            Commands::UpdateClo { packages } => SomeCore(CoreCommand::UpdateClo { packages }),
            #[cfg(feature = "cicd")]
            Commands::UpdateCicd { packages } => SomeCore(CoreCommand::UpdateCicd { packages }),
            #[cfg(feature = "iot")]
            Commands::UpdateIot { packages } => SomeCore(CoreCommand::UpdateIot { packages }),
            #[cfg(feature = "app")]
            Commands::UpdateApp { packages } => SomeCore(CoreCommand::UpdateApp { packages }),
            #[cfg(feature = "lib")]
            Commands::UpdateLib { packages } => SomeCore(CoreCommand::UpdateLib { packages }),
        }
    }
}

/// Detect the project ecosystem (core type) from the current working directory.
///
/// Priority:
///   1. `.megagate/project.toml` — ecosystem field
///   2. Auto-detect: `package.json` → web, `Cargo.toml` → lib, `pyproject.toml` → ai
#[cfg(all(
    feature = "web",
    any(
        feature = "game",
        feature = "ai",
        feature = "clo",
        feature = "cicd",
        feature = "iot",
        feature = "app",
        feature = "lib"
    )
))]
fn detect_ecosystem() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;

    // 1. Try .megagate/project.toml
    let megagate_path = cwd.join(".megagate").join("project.toml");
    if megagate_path.exists() {
        let content = std::fs::read_to_string(megagate_path).ok()?;
        for line in content.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("ecosystem = \"") {
                if let Some(eco) = val.strip_suffix('"') {
                    if !eco.is_empty() {
                        return Some(eco.to_string());
                    }
                }
            }
        }
    }

    // 2. Auto-detect
    if cwd.join("package.json").exists() {
        return Some("web".to_string());
    }
    if cwd.join("Cargo.toml").exists() {
        return Some("lib".to_string());
    }
    if cwd.join("pyproject.toml").exists() {
        return Some("ai".to_string());
    }

    None
}
