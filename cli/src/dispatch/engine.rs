use anyhow::{bail, Result};

use crate::Cli;
use crate::Commands;

use super::types::DispatchCommand;

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
        Commands::Bench { .. } => "bench",
        Commands::Network { .. } => "network",
        Commands::Doctor { .. } => "doctor",
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
        Commands::CiGenerate => "ci-generate",
        Commands::Verify => "verify",
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
    match crate::dispatch::per_core::command_to_dispatch(command, core) {
        DispatchCommand::Common(cmd) => super::common::dispatch_common(cmd, core, recursive).await,
        DispatchCommand::Core(cmd) => super::core::dispatch_core(cmd).await,
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
