use anyhow::{bail, Result};

use crate::{Cli, Commands};

mod common;
mod core;
mod types;

use types::{detect_ecosystem, CommonCommand, CoreCommand, DispatchCommand};

pub async fn run(cli: Cli) -> Result<()> {
    mg_ui::set_quiet(cli.quiet);
    let core = cli.core.as_deref();

    if cli.recursive {
        // Publish đã implement recursive (Phase 1); các lệnh khác bị chặn
        if !matches!(cli.command, Some(Commands::Publish { .. })) {
            reject_unsupported_recursive(cli.command.as_ref())?;
        }
    }

    if cli.audit_strict {
        if let Some(command) = cli.command.as_ref() {
            reject_unsupported_audit_strict(command)?;
        }
        std::env::set_var("MG_AUDIT_STRICT", "1");
    }

    match cli.command {
        Some(command) => dispatch_command(command, core, cli.recursive).await,
        None => {
            let cores = crate::factory::available_cores();
            mg_ui::help::print_custom_help(&cores);
            Ok(())
        }
    }
}

fn reject_unsupported_recursive(command: Option<&Commands>) -> Result<()> {
    let Some(command) = command else {
        bail!(
            "--recursive is declared on the CLI surface but workspace recursion is not implemented yet. Refusing to pretend it ran."
        );
    };

    bail!(
        "--recursive is not implemented for '{}' yet. Beta safety rule: refusing a silent no-op on workspace commands.",
        command_name(command)
    )
}

fn reject_unsupported_audit_strict(command: &Commands) -> Result<()> {
    let _ = command;
    Ok(())
}

fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Init { .. } => "init",
        Commands::Info { .. } => "info",
        Commands::Search { .. } => "search",
        Commands::Outdated { .. } => "outdated",
        Commands::Audit { .. } => "audit",
        Commands::SelfUpdate => "self-update",
        Commands::Publish { .. } => "publish",
        Commands::Patch { .. } => "patch",
        Commands::Dedupe { .. } => "dedupe",
        Commands::Template { .. } => "template",
        Commands::Workspace { .. } => "workspace",
        Commands::Store { .. } => "store",
        Commands::Trust { .. } => "trust",
        Commands::Hooks { .. } => "hooks",
        Commands::Docs { .. } => "docs",
        Commands::Telemetry { .. } => "telemetry",
        Commands::Sbom { .. } => "sbom",
        Commands::Login { .. } => "login",
        Commands::Registry { .. } => "registry",
        Commands::Model { .. } => "model",
        Commands::Dev { .. } => "dev",
        Commands::Run { .. } => "run",
        Commands::Build { .. } => "build",
        Commands::Flash { .. } => "flash",
        Commands::Deploy { .. } => "deploy",
        Commands::Start => "start",
        Commands::Exec { .. } => "exec",
        Commands::Dlx { .. } => "dlx",
        Commands::Cache { .. } => "cache",
        Commands::Install { .. } => "install",
        Commands::Add { .. } => "add",
        Commands::Remove { .. } => "remove",
        Commands::Update { .. } => "update",
        Commands::List => "list",
        Commands::Link { .. } => "link",
        Commands::Unlink { .. } => "unlink",
        Commands::Why { .. } => "why",
        Commands::CreateWeb { .. } => "create-web",
        Commands::CreateGame { .. } => "create-game",
        Commands::CreateAi { .. } => "create-ai",
        Commands::CreateClo { .. } => "create-clo",
        Commands::CreateCicd { .. } => "create-cicd",
        Commands::CreateIot { .. } => "create-iot",
        Commands::CreateApp { .. } => "create-app",
        Commands::CreateLib { .. } => "create-lib",
        Commands::CreateHardware { .. } => "create-hardware",
        Commands::InstallWeb { .. } => "install-web",
        Commands::InstallGame { .. } => "install-game",
        Commands::InstallAi { .. } => "install-ai",
        Commands::InstallClo { .. } => "install-clo",
        Commands::InstallCicd { .. } => "install-cicd",
        Commands::InstallIot { .. } => "install-iot",
        Commands::InstallApp { .. } => "install-app",
        Commands::InstallLib { .. } => "install-lib",
        Commands::InstallHardware { .. } => "install-hardware",
        Commands::AddWeb { .. } => "add-web",
        Commands::AddGame { .. } => "add-game",
        Commands::AddAi { .. } => "add-ai",
        Commands::AddClo { .. } => "add-clo",
        Commands::AddCicd { .. } => "add-cicd",
        Commands::AddIot { .. } => "add-iot",
        Commands::AddApp { .. } => "add-app",
        Commands::AddLib { .. } => "add-lib",
        Commands::AddHardware { .. } => "add-hardware",
        Commands::RemoveWeb { .. } => "remove-web",
        Commands::RemoveGame { .. } => "remove-game",
        Commands::RemoveAi { .. } => "remove-ai",
        Commands::RemoveClo { .. } => "remove-clo",
        Commands::RemoveCicd { .. } => "remove-cicd",
        Commands::RemoveIot { .. } => "remove-iot",
        Commands::RemoveApp { .. } => "remove-app",
        Commands::RemoveLib { .. } => "remove-lib",
        Commands::ListWeb => "list-web",
        Commands::ListGame => "list-game",
        Commands::ListAi => "list-ai",
        Commands::ListClo => "list-clo",
        Commands::ListCicd => "list-cicd",
        Commands::ListIot => "list-iot",
        Commands::ListApp => "list-app",
        Commands::ListLib => "list-lib",
        Commands::ListHardware => "list-hardware",
        Commands::UpdateWeb { .. } => "update-web",
        Commands::UpdateGame { .. } => "update-game",
        Commands::UpdateAi { .. } => "update-ai",
        Commands::UpdateClo { .. } => "update-clo",
        Commands::UpdateCicd { .. } => "update-cicd",
        Commands::UpdateIot { .. } => "update-iot",
        Commands::UpdateApp { .. } => "update-app",
        Commands::UpdateLib { .. } => "update-lib",
    }
}

async fn dispatch_command(command: Commands, core: Option<&str>, recursive: bool) -> Result<()> {
    match command_to_dispatch(command, core) {
        DispatchCommand::Common(cmd) => common::dispatch_common(cmd, core, recursive).await,
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
        Commands::Audit { fix } => Some(CommonCommand::Audit { fix }),
        Commands::SelfUpdate => Some(CommonCommand::SelfUpdate),
        Commands::Run { script, args } => Some(CommonCommand::Run { script, args }),
        Commands::Build { target } => Some(CommonCommand::Build { target }),
        Commands::Flash { board, skip_build } => Some(CommonCommand::Flash { board, skip_build }),
        Commands::Deploy { run } => Some(CommonCommand::Deploy { run }),
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
        Commands::Store { cmd } => Some(CommonCommand::Store { cmd }),
        Commands::Trust { cmd } => Some(CommonCommand::Trust { cmd }),
        Commands::Hooks { cmd } => Some(CommonCommand::Hooks { cmd }),
        Commands::Docs { output } => Some(CommonCommand::Docs { output }),
        Commands::Telemetry { cmd } => Some(CommonCommand::Telemetry { cmd }),
        Commands::Sbom { output } => Some(CommonCommand::Sbom { output }),
        Commands::Template { cmd } => Some(CommonCommand::Template { cmd }),
        Commands::Workspace { cmd } => Some(CommonCommand::Workspace { cmd }),
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
        Commands::CreateHardware {
            framework,
            project_name,
        } => Some(CoreCommand::CreateHardware {
            framework,
            project_name,
        }),
        Commands::InstallWeb {
            packages,
            frozen,
            ignore_scripts,
            allow_scripts,
            prefer_dedupe,
            repair,
        } => Some(CoreCommand::InstallWeb {
            packages,
            frozen,
            ignore_scripts,
            allow_scripts,
            prefer_dedupe,
            repair,
        }),
        Commands::InstallGame { packages } => Some(CoreCommand::InstallGame { packages }),
        Commands::InstallAi { packages } => Some(CoreCommand::InstallAi { packages }),
        Commands::InstallClo { packages } => Some(CoreCommand::InstallClo {
            packages,
            dry_run: false,
        }),
        Commands::InstallCicd { packages } => Some(CoreCommand::InstallCicd {
            packages,
            dry_run: false,
        }),
        Commands::InstallIot { packages } => Some(CoreCommand::InstallIot { packages }),
        Commands::InstallApp { packages } => Some(CoreCommand::InstallApp {
            packages,
            dry_run: false,
        }),
        Commands::InstallLib { packages } => Some(CoreCommand::InstallLib { packages }),
        Commands::InstallHardware { packages } => Some(CoreCommand::InstallHardware { packages }),
        Commands::AddWeb {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            no_install,
            global,
        } => Some(CoreCommand::AddWeb {
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
        Commands::AddHardware { packages } => Some(CoreCommand::AddHardware { packages }),
        Commands::RemoveWeb {
            packages,
            no_install,
        } => Some(CoreCommand::RemoveWeb {
            packages,
            install: !no_install,
        }),
        Commands::RemoveGame { packages } => Some(CoreCommand::RemoveGame { packages }),
        Commands::RemoveAi { packages } => Some(CoreCommand::RemoveAi { packages }),
        Commands::RemoveClo { packages } => Some(CoreCommand::RemoveClo { packages }),
        Commands::RemoveCicd { packages } => Some(CoreCommand::RemoveCicd { packages }),
        Commands::RemoveIot { packages } => Some(CoreCommand::RemoveIot { packages }),
        Commands::RemoveApp { packages } => Some(CoreCommand::RemoveApp { packages }),
        Commands::RemoveLib { packages } => Some(CoreCommand::RemoveLib { packages }),
        Commands::ListWeb => Some(CoreCommand::ListWeb),
        Commands::ListGame => Some(CoreCommand::ListGame),
        Commands::ListAi => Some(CoreCommand::ListAi),
        Commands::ListClo => Some(CoreCommand::ListClo),
        Commands::ListCicd => Some(CoreCommand::ListCicd),
        Commands::ListIot => Some(CoreCommand::ListIot),
        Commands::ListApp => Some(CoreCommand::ListApp),
        Commands::ListLib => Some(CoreCommand::ListLib),
        Commands::ListHardware => Some(CoreCommand::ListHardware),
        Commands::UpdateWeb { packages, install } => {
            Some(CoreCommand::UpdateWeb { packages, install })
        }
        Commands::UpdateGame { packages, install } => {
            Some(CoreCommand::UpdateGame { packages, install })
        }
        Commands::UpdateAi { packages, install } => {
            Some(CoreCommand::UpdateAi { packages, install })
        }
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
    let ecosystem = core
        .map(|s| s.to_string())
        .or_else(|| detect_ecosystem().ok().flatten());

    match command {
        Commands::Install {
            packages,
            frozen,
            ignore_scripts,
            allow_scripts,
            prefer_dedupe,
            repair,
            dry_run,
        } => SomeCore(match ecosystem.as_deref() {
            Some("web") => CoreCommand::InstallWeb {
                packages,
                frozen,
                ignore_scripts,
                allow_scripts,
                prefer_dedupe,
                repair,
            },
            Some("game") => CoreCommand::InstallGame { packages },
            Some("ai") => CoreCommand::InstallAi { packages },
            Some("clo") => CoreCommand::InstallClo { packages, dry_run },
            Some("cicd") => CoreCommand::InstallCicd { packages, dry_run },
            Some("iot") => CoreCommand::InstallIot { packages },
            Some("app") => CoreCommand::InstallApp { packages, dry_run },
            Some("lib") => CoreCommand::InstallLib { packages },
            _ => CoreCommand::InstallWeb {
                packages,
                frozen,
                ignore_scripts,
                allow_scripts,
                prefer_dedupe,
                repair,
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
            no_install,
            ..
        } => SomeCore(match ecosystem.as_deref() {
            Some("web") => CoreCommand::AddWeb {
                packages,
                dev,
                exact,
                optional,
                peer,
                no_save,
                install: !no_install,
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
            Some("hardware") => CoreCommand::AddHardware { packages },
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
                install: !no_install,
                global,
            },
        }),
        Commands::Remove {
            packages,
            no_install,
        } => SomeCore(match ecosystem.as_deref() {
            Some("web") => CoreCommand::RemoveWeb {
                packages,
                install: !no_install,
            },
            Some("game") => CoreCommand::RemoveGame { packages },
            Some("ai") => CoreCommand::RemoveAi { packages },
            Some("clo") => CoreCommand::RemoveClo { packages },
            Some("cicd") => CoreCommand::RemoveCicd { packages },
            Some("iot") => CoreCommand::RemoveIot { packages },
            Some("app") => CoreCommand::RemoveApp { packages },
            Some("lib") => CoreCommand::RemoveLib { packages },
            _ => CoreCommand::RemoveWeb {
                packages,
                install: !no_install,
            },
        }),
        Commands::Update { packages, install } => SomeCore(match ecosystem.as_deref() {
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
            Some("hardware") => CoreCommand::ListHardware,
            _ => CoreCommand::ListWeb,
        }),
        _ => unreachable!("Unhandled command"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audit_strict_rejects_materializing_install_commands() {
        let install = Commands::Install {
            packages: vec![],
            frozen: false,
            ignore_scripts: false,
            allow_scripts: false,
            prefer_dedupe: false,
            repair: false,
            dry_run: false,
        };
        assert!(reject_unsupported_audit_strict(&install).is_ok());

        let add = Commands::AddWeb {
            packages: vec!["zod".into()],
            dev: false,
            exact: false,
            optional: false,
            peer: false,
            no_save: false,
            no_install: false,
            global: false,
        };
        assert!(reject_unsupported_audit_strict(&add).is_ok());
    }

    #[test]
    fn audit_strict_allows_audit_and_manifest_only_mutation() {
        assert!(reject_unsupported_audit_strict(&Commands::Audit { fix: false }).is_ok());

        let add = Commands::AddWeb {
            packages: vec!["zod".into()],
            dev: false,
            exact: false,
            optional: false,
            peer: false,
            no_save: false,
            no_install: true,
            global: false,
        };
        assert!(reject_unsupported_audit_strict(&add).is_ok());
    }

    #[test]
    fn recursive_is_rejected_instead_of_silently_ignored() {
        let err = reject_unsupported_recursive(Some(&Commands::Install {
            packages: Vec::new(),
            frozen: false,
            ignore_scripts: false,
            allow_scripts: false,
            prefer_dedupe: false,
            repair: false,
            dry_run: false,
        }))
        .unwrap_err();

        assert!(err
            .to_string()
            .contains("--recursive is not implemented for 'install' yet"));
    }
}
