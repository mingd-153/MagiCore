use anyhow::Result;

use crate::{Cli, Commands};

mod common;
mod core;
mod types;

use types::{CommonCommand, CoreCommand, DispatchCommand, detect_ecosystem};

pub async fn run(cli: Cli) -> Result<()> {
    let core = cli.core.as_deref();

    if cli.audit_strict {
        std::env::set_var("MG_AUDIT_STRICT", "1");
    }

    match cli.command {
        Some(command) => dispatch_command(command, core).await,
        None => {
            let cores = crate::factory::available_cores();
            mg_ui::help::print_custom_help(&cores);
            Ok(())
        }
    }
}

async fn dispatch_command(command: Commands, core: Option<&str>) -> Result<()> {
    match command_to_dispatch(command, core) {
        DispatchCommand::Common(cmd) => common::dispatch_common(cmd, core).await,
        DispatchCommand::Core(cmd) => core::dispatch_core(cmd).await,
    }
}

fn command_to_dispatch(command: Commands, core: Option<&str>) -> DispatchCommand {
    use DispatchCommand::{Common as SomeCommon, Core as SomeCore};

    // Check if it's a common command first
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
        Commands::Link { package } => Some(CommonCommand::Link { package }),
        Commands::Unlink { package } => Some(CommonCommand::Unlink { package }),
        Commands::Why { package } => Some(CommonCommand::Why { package }),
        _ => None,
    };

    if let Some(cmd) = common_cmd {
        return SomeCommon(cmd);
    }

    let explicit_core_cmd = match command.clone() {
        Commands::CreateWeb {
            framework,
            project_name,
            flags,
        } => Some(CoreCommand::CreateWeb {
            framework,
            project_name,
            flags,
        }),
        Commands::CreateGame {
            framework,
            project_name,
        } => Some(CoreCommand::CreateGame {
            framework,
            project_name,
        }),
        Commands::CreateAi {
            framework,
            project_name,
        } => Some(CoreCommand::CreateAi {
            framework,
            project_name,
        }),
        Commands::CreateClo {
            framework,
            project_name,
        } => Some(CoreCommand::CreateClo {
            framework,
            project_name,
        }),
        Commands::CreateCicd {
            framework,
            project_name,
        } => Some(CoreCommand::CreateCicd {
            framework,
            project_name,
        }),
        Commands::CreateIot {
            framework,
            project_name,
        } => Some(CoreCommand::CreateIot {
            framework,
            project_name,
        }),
        Commands::CreateApp {
            framework,
            project_name,
        } => Some(CoreCommand::CreateApp {
            framework,
            project_name,
        }),
        Commands::CreateLib { project_name } => Some(CoreCommand::CreateLib { project_name }),
        Commands::InstallWeb {
            packages,
            frozen,
            ignore_scripts,
        } => Some(CoreCommand::InstallWeb {
            packages,
            frozen,
            ignore_scripts,
        }),
        Commands::InstallGame { packages } => Some(CoreCommand::InstallGame { packages }),
        Commands::InstallAi { packages } => Some(CoreCommand::InstallAi { packages }),
        Commands::InstallClo { packages } => Some(CoreCommand::InstallClo { packages }),
        Commands::InstallCicd { packages } => Some(CoreCommand::InstallCicd { packages }),
        Commands::InstallIot { packages } => Some(CoreCommand::InstallIot { packages }),
        Commands::InstallApp { packages } => Some(CoreCommand::InstallApp { packages }),
        Commands::InstallLib { packages } => Some(CoreCommand::InstallLib { packages }),
        Commands::AddWeb {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        } => Some(CoreCommand::AddWeb {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
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
        } => Some(CoreCommand::AddGame {
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
        } => Some(CoreCommand::AddAi {
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
        } => Some(CoreCommand::AddClo {
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
        } => Some(CoreCommand::AddCicd {
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
        } => Some(CoreCommand::AddIot {
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
        } => Some(CoreCommand::AddApp {
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
        } => Some(CoreCommand::AddLib {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        }),
        Commands::RemoveWeb { package } => Some(CoreCommand::RemoveWeb { package }),
        Commands::RemoveGame { package } => Some(CoreCommand::RemoveGame { package }),
        Commands::RemoveAi { package } => Some(CoreCommand::RemoveAi { package }),
        Commands::RemoveClo { package } => Some(CoreCommand::RemoveClo { package }),
        Commands::RemoveCicd { package } => Some(CoreCommand::RemoveCicd { package }),
        Commands::RemoveIot { package } => Some(CoreCommand::RemoveIot { package }),
        Commands::RemoveApp { package } => Some(CoreCommand::RemoveApp { package }),
        Commands::RemoveLib { package } => Some(CoreCommand::RemoveLib { package }),
        Commands::ListWeb => Some(CoreCommand::ListWeb),
        Commands::ListGame => Some(CoreCommand::ListGame),
        Commands::ListAi => Some(CoreCommand::ListAi),
        Commands::ListClo => Some(CoreCommand::ListClo),
        Commands::ListCicd => Some(CoreCommand::ListCicd),
        Commands::ListIot => Some(CoreCommand::ListIot),
        Commands::ListApp => Some(CoreCommand::ListApp),
        Commands::ListLib => Some(CoreCommand::ListLib),
        Commands::UpdateWeb { packages, install } => {
            Some(CoreCommand::UpdateWeb { packages, install })
        }
        Commands::UpdateGame { packages, install } => {
            Some(CoreCommand::UpdateGame { packages, install })
        }
        Commands::UpdateAi { packages, install } => Some(CoreCommand::UpdateAi { packages, install }),
        Commands::UpdateClo { packages, install } => {
            Some(CoreCommand::UpdateClo { packages, install })
        }
        Commands::UpdateCicd { packages, install } => {
            Some(CoreCommand::UpdateCicd { packages, install })
        }
        Commands::UpdateIot { packages, install } => {
            Some(CoreCommand::UpdateIot { packages, install })
        }
        Commands::UpdateApp { packages, install } => {
            Some(CoreCommand::UpdateApp { packages, install })
        }
        Commands::UpdateLib { packages, install } => {
            Some(CoreCommand::UpdateLib { packages, install })
        }
        _ => None,
    };

    if let Some(cmd) = explicit_core_cmd {
        return SomeCore(cmd);
    }

    // Bare commands - use --core if provided, else auto-detect
    let ecosystem = core.map(|s| s.to_string()).or_else(|| detect_ecosystem().ok().flatten());

    match command {
        Commands::Install {
            packages,
            frozen,
            ignore_scripts,
        } => SomeCore(match ecosystem.as_deref() {
            Some("web") => CoreCommand::InstallWeb {
                packages,
                frozen,
                ignore_scripts,
            },
            Some("game") => CoreCommand::InstallGame { packages },
            Some("ai") => CoreCommand::InstallAi { packages },
            Some("clo") => CoreCommand::InstallClo { packages },
            Some("cicd") => CoreCommand::InstallCicd { packages },
            Some("iot") => CoreCommand::InstallIot { packages },
            Some("app") => CoreCommand::InstallApp { packages },
            Some("lib") => CoreCommand::InstallLib { packages },
            _ => CoreCommand::InstallWeb {
                packages,
                frozen,
                ignore_scripts,
            },
        }),
        Commands::Add {
            packages,
            dev,
            global,
            exact,
            optional,
            peer,
            no_save,
            ..
        } => SomeCore(match ecosystem.as_deref() {
            Some("web") => CoreCommand::AddWeb {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            Some("game") => CoreCommand::AddGame {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            Some("ai") => CoreCommand::AddAi {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            Some("clo") => CoreCommand::AddClo {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            Some("cicd") => CoreCommand::AddCicd {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            Some("iot") => CoreCommand::AddIot {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            Some("app") => CoreCommand::AddApp {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            Some("lib") => CoreCommand::AddLib {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            _ => CoreCommand::AddWeb {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
        }),
        Commands::Remove { package } => SomeCore(match ecosystem.as_deref() {
            Some("web") => CoreCommand::RemoveWeb { package },
            Some("game") => CoreCommand::RemoveGame { package },
            Some("ai") => CoreCommand::RemoveAi { package },
            Some("clo") => CoreCommand::RemoveClo { package },
            Some("cicd") => CoreCommand::RemoveCicd { package },
            Some("iot") => CoreCommand::RemoveIot { package },
            Some("app") => CoreCommand::RemoveApp { package },
            Some("lib") => CoreCommand::RemoveLib { package },
            _ => CoreCommand::RemoveWeb { package },
        }),
        Commands::Update {
            packages,
            install,
        } => SomeCore(match ecosystem.as_deref() {
            Some("web") => CoreCommand::UpdateWeb { packages, install },
            Some("game") => CoreCommand::UpdateGame { packages, install },
            Some("ai") => CoreCommand::UpdateAi { packages, install },
            Some("clo") => CoreCommand::UpdateClo { packages, install },
            Some("cicd") => CoreCommand::UpdateCicd { packages, install },
            Some("iot") => CoreCommand::UpdateIot { packages, install },
            Some("app") => CoreCommand::UpdateApp { packages, install },
            Some("lib") => CoreCommand::UpdateLib { packages, install },
            _ => CoreCommand::UpdateWeb { packages, install },
        }),
        Commands::List => SomeCore(match ecosystem.as_deref() {
            Some("web") => CoreCommand::ListWeb,
            Some("game") => CoreCommand::ListGame,
            Some("ai") => CoreCommand::ListAi,
            Some("clo") => CoreCommand::ListClo,
            Some("cicd") => CoreCommand::ListCicd,
            Some("iot") => CoreCommand::ListIot,
            Some("app") => CoreCommand::ListApp,
            Some("lib") => CoreCommand::ListLib,
            _ => CoreCommand::ListWeb,
        }),
        _ => unreachable!("Unhandled command"),
    }
}
