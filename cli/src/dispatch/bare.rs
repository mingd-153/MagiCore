use crate::dispatch::types::{detect_ecosystem, CoreCommand, DispatchCommand};
use crate::Commands;

/// Bare verb commands (install/add/remove/update/list) → CoreCommand theo --core hoặc detect_ecosystem.
pub fn bare_core_command(
    command: Commands,
    ecosystem: Option<String>,
) -> anyhow::Result<DispatchCommand> {
    use DispatchCommand::Core as SomeCore;

    // Bare commands must use --core or a detected project marker.
    // Lệnh bare phải có --core hoặc marker project, không fallback web âm thầm.
    let ecosystem = ecosystem.or_else(|| detect_ecosystem().ok().flatten());

    let dispatch = match command {
        Commands::Install {
            packages,
            frozen,
            ignore_scripts,
            allow_scripts,
            prefer_dedupe,
            repair,
            dry_run,
            offline,  // T4.1
        } => SomeCore(match require_ecosystem("install", ecosystem.as_deref())? {
            "web" => CoreCommand::InstallWeb {
                packages,
                frozen,
                ignore_scripts,
                allow_scripts,
                prefer_dedupe,
                repair,
                offline,  // T4.1
            },
            "game" => CoreCommand::InstallGame { packages },
            "ai" => CoreCommand::InstallAi { packages, dry_run },
            "clo" => CoreCommand::InstallClo { packages, dry_run },
            "cicd" => CoreCommand::InstallCicd { packages, dry_run },
            "iot" => CoreCommand::InstallIot { packages },
            "app" => CoreCommand::InstallApp { packages, dry_run },
            "lib" => CoreCommand::InstallLib { packages },
            other => return Err(crate::error::unknown_core(other)),
        }),
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
        } => SomeCore(match require_ecosystem("add", ecosystem.as_deref())? {
            "web" => CoreCommand::AddWeb {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                install: !no_install,
                global,
            },
            "game" => CoreCommand::AddGame {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            "ai" => CoreCommand::AddAi {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            "clo" => CoreCommand::AddClo {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            "cicd" => CoreCommand::AddCicd {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            "iot" => CoreCommand::AddIot {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            "app" => CoreCommand::AddApp {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            "hardware" => CoreCommand::AddHardware { packages },
            "lib" => CoreCommand::AddLib {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
            },
            other => return Err(crate::error::unknown_core(other)),
        }),
        Commands::Remove {
            packages,
            no_install,
        } => SomeCore(match require_ecosystem("remove", ecosystem.as_deref())? {
            "web" => CoreCommand::RemoveWeb {
                packages,
                install: !no_install,
            },
            "game" => CoreCommand::RemoveGame { packages },
            "ai" => CoreCommand::RemoveAi { packages },
            "clo" => CoreCommand::RemoveClo { packages },
            "cicd" => CoreCommand::RemoveCicd { packages },
            "iot" => CoreCommand::RemoveIot { packages },
            "app" => CoreCommand::RemoveApp { packages },
            "lib" => CoreCommand::RemoveLib { packages },
            other => return Err(crate::error::unknown_core(other)),
        }),
        Commands::Update { packages, install } => {
            SomeCore(match require_ecosystem("update", ecosystem.as_deref())? {
                "web" => CoreCommand::UpdateWeb { packages, install },
                "game" => CoreCommand::UpdateGame { packages, install },
                "ai" => CoreCommand::UpdateAi { packages, install },
                "clo" => CoreCommand::UpdateClo { packages, install },
                "cicd" => CoreCommand::UpdateCicd { packages, install },
                "iot" => CoreCommand::UpdateIot { packages, install },
                "app" => CoreCommand::UpdateApp { packages, install },
                "lib" => CoreCommand::UpdateLib { packages, install },
                other => return Err(crate::error::unknown_core(other)),
            })
        }
        Commands::List => SomeCore(match require_ecosystem("list", ecosystem.as_deref())? {
            "web" => CoreCommand::ListWeb,
            "game" => CoreCommand::ListGame,
            "ai" => CoreCommand::ListAi,
            "clo" => CoreCommand::ListClo,
            "cicd" => CoreCommand::ListCicd,
            "iot" => CoreCommand::ListIot,
            "app" => CoreCommand::ListApp,
            "lib" => CoreCommand::ListLib,
            "hardware" => CoreCommand::ListHardware,
            other => return Err(crate::error::unknown_core(other)),
        }),
        _ => unreachable!("Unhandled command"),
    };
    Ok(dispatch)
}

fn require_ecosystem<'a>(verb: &str, ecosystem: Option<&'a str>) -> anyhow::Result<&'a str> {
    ecosystem.ok_or_else(|| crate::error::bare_core_not_detected(verb))
}
