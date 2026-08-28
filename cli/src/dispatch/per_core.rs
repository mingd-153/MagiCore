use crate::dispatch::bare;
use crate::dispatch::types::{detect_ecosystem, CommonCommand, CoreCommand, DispatchCommand};
use crate::Commands;

pub fn command_to_dispatch(
    command: Commands,
    core: Option<&str>,
) -> anyhow::Result<DispatchCommand> {
    use DispatchCommand::{Common as SomeCommon, Core as SomeCore};

    // Check if it's a common command first
    let common_cmd = match command.clone() {
        Commands::Init {
            template,
            signature,
        } => Some(CommonCommand::Init {
            template,
            signature,
        }),
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
        Commands::Config { cmd, local } => Some(CommonCommand::Config { cmd, local }),
        Commands::Stage { dir } => Some(CommonCommand::Stage { dir }),
        Commands::Import { dir } => Some(CommonCommand::Import { dir }),
        Commands::Sbom {
            format,
            output,
            name,
            version,
            dir,
        } => Some(CommonCommand::Sbom {
            format,
            output,
            name,
            version,
            dir,
        }),
        Commands::Run { script, args } => Some(CommonCommand::Run { script, args }),
        Commands::Build { target } => Some(CommonCommand::Build { target }),
        Commands::Flash { board, skip_build } => Some(CommonCommand::Flash { board, skip_build }),
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
        Commands::Mcp => Some(CommonCommand::Mcp),
        Commands::Store { cmd } => Some(CommonCommand::Store { cmd }),
        Commands::Bench { args } => Some(CommonCommand::Bench { args }),
        Commands::Network { cmd } => Some(CommonCommand::Network { cmd }),
        Commands::Doctor { cmd } => Some(CommonCommand::Doctor { cmd }),
        Commands::Trust { cmd } => Some(CommonCommand::Trust { cmd }),
        Commands::Hooks { cmd } => Some(CommonCommand::Hooks { cmd }),
        Commands::Docs { output } => Some(CommonCommand::Docs { output }),
        Commands::Telemetry { cmd } => Some(CommonCommand::Telemetry { cmd }),
        Commands::Template { cmd } => Some(CommonCommand::Template { cmd }),
        Commands::Workspace { cmd } => Some(CommonCommand::Workspace { cmd }),
        _ => None,
    };

    if let Some(cmd) = common_cmd {
        return Ok(SomeCommon(cmd));
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
            offline,
        } => Some(CoreCommand::InstallWeb {
            packages,
            frozen,
            ignore_scripts,
            allow_scripts,
            prefer_dedupe,
            repair,
            offline,
        }),
        Commands::InstallGame { packages } => Some(CoreCommand::InstallGame { packages }),
        Commands::InstallAi { packages, dry_run } => {
            Some(CoreCommand::InstallAi { packages, dry_run })
        }
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
        return Ok(SomeCore(cmd));
    }

    let ecosystem = core
        .map(|s| s.to_string())
        .or_else(|| detect_ecosystem().ok().flatten());

    let dispatch = match command {
        Commands::Install { .. }
        | Commands::Add { .. }
        | Commands::Remove { .. }
        | Commands::Update { .. }
        | Commands::List => match bare::bare_core_command(command, ecosystem)? {
            DispatchCommand::Core(cmd) => SomeCore(cmd),
            DispatchCommand::Common(_) => unreachable!("bare verbs are core commands"),
        },
        _ => unreachable!("Unhandled command"),
    };
    Ok(dispatch)
}
