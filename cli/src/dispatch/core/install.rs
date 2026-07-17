use anyhow::Result;

use crate::commands;

use super::super::types::CoreCommand;

pub fn matches(command: &CoreCommand) -> bool {
    matches!(
        command,
        CoreCommand::InstallWeb { .. }
            | CoreCommand::InstallGame { .. }
            | CoreCommand::InstallAi { .. }
            | CoreCommand::InstallClo { .. }
            | CoreCommand::InstallCicd { .. }
            | CoreCommand::InstallIot { .. }
            | CoreCommand::InstallApp { .. }
            | CoreCommand::InstallLib { .. }
    )
}

pub async fn dispatch(command: CoreCommand) -> Result<()> {
    match command {
        CoreCommand::InstallWeb {
            packages,
            frozen,
            ignore_scripts,
        } => commands::core::web::install(packages, frozen, ignore_scripts).await,
        CoreCommand::InstallGame { packages } => commands::core::game::install(packages).await,
        CoreCommand::InstallAi { packages } => commands::core::ai::install(packages).await,
        CoreCommand::InstallClo { packages } => commands::core::clo::install(packages).await,
        CoreCommand::InstallCicd { packages } => commands::core::cicd::install(packages).await,
        CoreCommand::InstallIot { packages } => commands::core::iot::install(packages).await,
        CoreCommand::InstallApp { packages } => commands::core::app::install(packages).await,
        CoreCommand::InstallLib { packages } => commands::core::library::install(packages).await,
        _ => unreachable!("non-install command routed to install dispatcher"),
    }
}
