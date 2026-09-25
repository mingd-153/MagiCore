use crate::Commands;
use crate::dispatch::bare;
use crate::dispatch::types::{CommonCommand, CoreCommand, DispatchCommand, detect_ecosystem};

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
        Commands::Dev {
            host,
            port,
            clear,
            compat_runtime,
        } => Some(CommonCommand::Dev {
            host,
            port,
            clear,
            compat_runtime,
        }),
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
        Commands::Audit { fix, format } => Some(CommonCommand::Audit { fix, format }),
        Commands::SelfUpdate {
            version,
            variant,
            dry_run,
            trust_root,
            allow_unsigned,
        } => Some(CommonCommand::SelfUpdate {
            version,
            variant,
            dry_run,
            trust_root,
            allow_unsigned,
        }),
        Commands::SignRelease { manifest, key_hex } => {
            Some(CommonCommand::SignRelease { manifest, key_hex })
        }
        Commands::Config { cmd, local } => Some(CommonCommand::Config { cmd, local }),
        Commands::Stage { dir } => Some(CommonCommand::Stage { dir }),
        Commands::Import {
            dir,
            allow_unsigned,
        } => Some(CommonCommand::Import {
            dir,
            allow_unsigned,
        }),
        Commands::Migrate { cmd } => Some(CommonCommand::Migrate { cmd }),
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
        Commands::Run {
            script,
            args,
            compat_runtime,
        } => Some(CommonCommand::Run {
            script,
            args,
            compat_runtime,
        }),
        Commands::Test {
            args,
            compat_runtime,
        } => Some(CommonCommand::Test {
            args,
            compat_runtime,
        }),
        Commands::Optimizer { force } => Some(CommonCommand::Optimizer { force }),
        Commands::Build {
            target,
            compat_runtime,
        } => Some(CommonCommand::Build {
            target,
            compat_runtime,
        }),
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
        Commands::Capabilities => Some(CommonCommand::Capabilities),
        Commands::Store { cmd } => Some(CommonCommand::Store { cmd }),
        Commands::Bench { args } => Some(CommonCommand::Bench { args }),
        Commands::Network { cmd } => Some(CommonCommand::Network { cmd }),
        Commands::Doctor { cmd } => Some(CommonCommand::Doctor { cmd }),
        Commands::Trust { cmd } => Some(CommonCommand::Trust { cmd }),
        Commands::Hooks { cmd } => Some(CommonCommand::Hooks { cmd }),
        Commands::Docs { output } => Some(CommonCommand::Docs { output }),
        Commands::Completion { shell } => Some(CommonCommand::Completion { shell }),
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
        Commands::CreateLib {
            framework,
            project_name,
        } => Some(CoreCommand::CreateLib {
            framework,
            project_name,
        }),
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
            compat_runtime,
        } => Some(CoreCommand::InstallWeb {
            packages,
            frozen,
            ignore_scripts,
            allow_scripts,
            prefer_dedupe,
            repair,
            offline,
            compat_runtime,
        }),
        Commands::InstallGame {
            packages,
            compat_runtime,
        } => Some(CoreCommand::InstallGame {
            packages,
            compat_runtime,
        }),
        Commands::InstallAi {
            packages,
            dry_run,
            compat_runtime,
            frozen,
        } => Some(CoreCommand::InstallAi {
            packages,
            dry_run,
            compat_runtime,
            frozen,
        }),
        Commands::InstallClo {
            packages,
            compat_runtime,
        } => Some(CoreCommand::InstallClo {
            packages,
            dry_run: false,
            compat_runtime,
        }),
        Commands::InstallCicd {
            packages,
            compat_runtime,
        } => Some(CoreCommand::InstallCicd {
            packages,
            dry_run: false,
            compat_runtime,
        }),
        Commands::InstallIot {
            packages,
            compat_runtime,
        } => Some(CoreCommand::InstallIot {
            packages,
            compat_runtime,
        }),
        Commands::InstallApp {
            packages,
            compat_runtime,
            frozen,
        } => Some(CoreCommand::InstallApp {
            packages,
            dry_run: false,
            compat_runtime,
            frozen,
        }),
        Commands::InstallLib {
            packages,
            compat_runtime,
            frozen,
        } => Some(CoreCommand::InstallLib {
            packages,
            compat_runtime,
            frozen,
        }),
        Commands::InstallHardware {
            packages,
            compat_runtime,
        } => Some(CoreCommand::InstallHardware {
            packages,
            compat_runtime,
        }),
        Commands::AddWeb {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            no_install,
            global,
            compat_runtime,

            version,
        } => Some(CoreCommand::AddWeb {
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
        }),
        Commands::AddGame {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        } => Some(CoreCommand::AddGame {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        }),
        Commands::AddAi {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        } => Some(CoreCommand::AddAi {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        }),
        Commands::AddClo {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        } => Some(CoreCommand::AddClo {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        }),
        Commands::AddCicd {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        } => Some(CoreCommand::AddCicd {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        }),
        Commands::AddIot {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        } => Some(CoreCommand::AddIot {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        }),
        Commands::AddApp {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        } => Some(CoreCommand::AddApp {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        }),
        Commands::AddLib {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        } => Some(CoreCommand::AddLib {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,

            version,
        }),
        Commands::AddHardware {
            packages,
            compat_runtime,

            version,
        } => Some(CoreCommand::AddHardware {
            packages,
            compat_runtime,

            version,
        }),
        Commands::RemoveWeb {
            packages,
            no_install,
            compat_runtime,
        } => Some(CoreCommand::RemoveWeb {
            packages,
            install: !no_install,
            compat_runtime,
        }),
        Commands::RemoveGame {
            packages,
            compat_runtime,
        } => Some(CoreCommand::RemoveGame {
            packages,
            compat_runtime,
        }),
        Commands::RemoveAi {
            packages,
            compat_runtime,
        } => Some(CoreCommand::RemoveAi {
            packages,
            compat_runtime,
        }),
        Commands::RemoveClo {
            packages,
            compat_runtime,
        } => Some(CoreCommand::RemoveClo {
            packages,
            compat_runtime,
        }),
        Commands::RemoveCicd {
            packages,
            compat_runtime,
        } => Some(CoreCommand::RemoveCicd {
            packages,
            compat_runtime,
        }),
        Commands::RemoveIot {
            packages,
            compat_runtime,
        } => Some(CoreCommand::RemoveIot {
            packages,
            compat_runtime,
        }),
        Commands::RemoveApp {
            packages,
            compat_runtime,
        } => Some(CoreCommand::RemoveApp {
            packages,
            compat_runtime,
        }),
        Commands::RemoveLib {
            packages,
            compat_runtime,
        } => Some(CoreCommand::RemoveLib {
            packages,
            compat_runtime,
        }),
        Commands::ListWeb { compat_runtime } => Some(CoreCommand::ListWeb { compat_runtime }),
        Commands::ListGame { compat_runtime } => Some(CoreCommand::ListGame { compat_runtime }),
        Commands::ListAi { compat_runtime } => Some(CoreCommand::ListAi { compat_runtime }),
        Commands::ListClo { compat_runtime } => Some(CoreCommand::ListClo { compat_runtime }),
        Commands::ListCicd { compat_runtime } => Some(CoreCommand::ListCicd { compat_runtime }),
        Commands::ListIot { compat_runtime } => Some(CoreCommand::ListIot { compat_runtime }),
        Commands::ListApp { compat_runtime } => Some(CoreCommand::ListApp { compat_runtime }),
        Commands::ListLib { compat_runtime } => Some(CoreCommand::ListLib { compat_runtime }),
        Commands::ListHardware { compat_runtime } => {
            Some(CoreCommand::ListHardware { compat_runtime })
        }
        Commands::UpdateWeb {
            packages,
            install,
            compat_runtime,
        } => Some(CoreCommand::UpdateWeb {
            packages,
            install,
            compat_runtime,
        }),
        Commands::UpdateGame {
            packages,
            install,
            compat_runtime,
        } => Some(CoreCommand::UpdateGame {
            packages,
            install,
            compat_runtime,
        }),
        Commands::UpdateAi {
            packages,
            install,
            compat_runtime,
        } => Some(CoreCommand::UpdateAi {
            packages,
            install,
            compat_runtime,
        }),
        Commands::UpdateClo {
            packages,
            install,
            compat_runtime,
        } => Some(CoreCommand::UpdateClo {
            packages,
            install,
            compat_runtime,
        }),
        Commands::UpdateCicd {
            packages,
            install,
            compat_runtime,
        } => Some(CoreCommand::UpdateCicd {
            packages,
            install,
            compat_runtime,
        }),
        Commands::UpdateIot {
            packages,
            install,
            compat_runtime,
        } => Some(CoreCommand::UpdateIot {
            packages,
            install,
            compat_runtime,
        }),
        Commands::UpdateApp {
            packages,
            install,
            compat_runtime,
        } => Some(CoreCommand::UpdateApp {
            packages,
            install,
            compat_runtime,
        }),
        Commands::UpdateLib {
            packages,
            install,
            compat_runtime,
        } => Some(CoreCommand::UpdateLib {
            packages,
            install,
            compat_runtime,
        }),
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
        | Commands::List { .. } => match bare::bare_core_command(command, ecosystem)? {
            DispatchCommand::Core(cmd) => SomeCore(cmd),
            DispatchCommand::Common(_) => unreachable!("bare verbs are core commands"),
        },
        _ => unreachable!("Unhandled command"),
    };
    Ok(dispatch)
}
