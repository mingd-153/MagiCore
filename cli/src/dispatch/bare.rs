use crate::Commands;
use crate::dispatch::types::{CoreCommand, DispatchCommand, detect_ecosystem};

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
            offline, // T4.1
            compat_runtime,
        } => SomeCore(match require_ecosystem("install", ecosystem.as_deref())? {
            "web" => CoreCommand::InstallWeb {
                packages,
                frozen,
                ignore_scripts,
                allow_scripts,
                prefer_dedupe,
                repair,
                offline, // T4.1
                compat_runtime,
            },
            "game" => CoreCommand::InstallGame {
                packages,
                compat_runtime,
            },
            "ai" => CoreCommand::InstallAi {
                packages,
                dry_run,
                compat_runtime,
            },
            "clo" => CoreCommand::InstallClo {
                packages,
                dry_run,
                compat_runtime,
            },
            "cicd" => CoreCommand::InstallCicd {
                packages,
                dry_run,
                compat_runtime,
            },
            "iot" => CoreCommand::InstallIot {
                packages,
                compat_runtime,
            },
            "app" => CoreCommand::InstallApp {
                packages,
                dry_run,
                compat_runtime,
            },
            "lib" => CoreCommand::InstallLib {
                packages,
                compat_runtime,
            },
            other => return Err(crate::error::unknown_core(other)),
        }),
        Commands::Add {
            packages,
            version,
            dev,
            global,
            exact,
            optional,
            peer,
            no_save,
            no_install,
            compat_runtime,
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
                compat_runtime,
                version,
            },
            "game" => CoreCommand::AddGame {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
                version,
            },
            "ai" => CoreCommand::AddAi {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
                version,
            },
            "clo" => CoreCommand::AddClo {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
                version,
            },
            "cicd" => CoreCommand::AddCicd {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
                version,
            },
            "iot" => CoreCommand::AddIot {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
                version,
            },
            "app" => CoreCommand::AddApp {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
                version,
            },
            "hardware" => CoreCommand::AddHardware {
                packages,
                compat_runtime,
                version,
            },
            "lib" => CoreCommand::AddLib {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
                version,
            },
            other => return Err(crate::error::unknown_core(other)),
        }),
        Commands::Remove {
            packages,
            no_install,
            compat_runtime,
        } => SomeCore(match require_ecosystem("remove", ecosystem.as_deref())? {
            "web" => CoreCommand::RemoveWeb {
                packages,
                install: !no_install,
                compat_runtime,
            },
            "game" => CoreCommand::RemoveGame {
                packages,
                compat_runtime,
            },
            "ai" => CoreCommand::RemoveAi {
                packages,
                compat_runtime,
            },
            "clo" => CoreCommand::RemoveClo {
                packages,
                compat_runtime,
            },
            "cicd" => CoreCommand::RemoveCicd {
                packages,
                compat_runtime,
            },
            "iot" => CoreCommand::RemoveIot {
                packages,
                compat_runtime,
            },
            "app" => CoreCommand::RemoveApp {
                packages,
                compat_runtime,
            },
            "lib" => CoreCommand::RemoveLib {
                packages,
                compat_runtime,
            },
            other => return Err(crate::error::unknown_core(other)),
        }),
        Commands::Update {
            packages,
            install,
            compat_runtime,
        } => SomeCore(match require_ecosystem("update", ecosystem.as_deref())? {
            "web" => CoreCommand::UpdateWeb {
                packages,
                install,
                compat_runtime,
            },
            "game" => CoreCommand::UpdateGame {
                packages,
                install,
                compat_runtime,
            },
            "ai" => CoreCommand::UpdateAi {
                packages,
                install,
                compat_runtime,
            },
            "clo" => CoreCommand::UpdateClo {
                packages,
                install,
                compat_runtime,
            },
            "cicd" => CoreCommand::UpdateCicd {
                packages,
                install,
                compat_runtime,
            },
            "iot" => CoreCommand::UpdateIot {
                packages,
                install,
                compat_runtime,
            },
            "app" => CoreCommand::UpdateApp {
                packages,
                install,
                compat_runtime,
            },
            "lib" => CoreCommand::UpdateLib {
                packages,
                install,
                compat_runtime,
            },
            other => return Err(crate::error::unknown_core(other)),
        }),
        Commands::List { compat_runtime } => {
            SomeCore(match require_ecosystem("list", ecosystem.as_deref())? {
                "web" => CoreCommand::ListWeb { compat_runtime },
                "game" => CoreCommand::ListGame { compat_runtime },
                "ai" => CoreCommand::ListAi { compat_runtime },
                "clo" => CoreCommand::ListClo { compat_runtime },
                "cicd" => CoreCommand::ListCicd { compat_runtime },
                "iot" => CoreCommand::ListIot { compat_runtime },
                "app" => CoreCommand::ListApp { compat_runtime },
                "lib" => CoreCommand::ListLib { compat_runtime },
                "hardware" => CoreCommand::ListHardware { compat_runtime },
                other => return Err(crate::error::unknown_core(other)),
            })
        }
        _ => unreachable!("Unhandled command"),
    };
    Ok(dispatch)
}

fn require_ecosystem<'a>(verb: &str, ecosystem: Option<&'a str>) -> anyhow::Result<&'a str> {
    ecosystem.ok_or_else(|| crate::error::bare_core_not_detected(verb))
}
