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
            | CoreCommand::InstallHardware { .. }
    )
}

pub async fn dispatch(command: CoreCommand) -> Result<()> {
    match command {
        CoreCommand::InstallWeb {
            packages,
            frozen,
            ignore_scripts,
            allow_scripts,
            prefer_dedupe,
            repair,
        } => {
            commands::core::install::web::install(
                packages,
                frozen,
                ignore_scripts,
                allow_scripts,
                prefer_dedupe,
                repair,
            )
            .await
        }
        #[cfg(feature = "game")]
        CoreCommand::InstallGame { packages } => {
            commands::core::install::game::install(packages).await
        }
        #[cfg(not(feature = "game"))]
        CoreCommand::InstallGame { .. } => Err(crate::error::core_not_in_build("game")),
        #[cfg(feature = "ai")]
        CoreCommand::InstallAi { packages, dry_run } => {
            commands::core::install::ai::install(packages, dry_run).await
        }
        #[cfg(not(feature = "ai"))]
        CoreCommand::InstallAi { .. } => Err(crate::error::core_not_in_build("ai")),
        #[cfg(feature = "clo")]
        CoreCommand::InstallClo { packages, dry_run } => {
            commands::core::install::clo::install(packages, dry_run).await
        }
        #[cfg(not(feature = "clo"))]
        CoreCommand::InstallClo { .. } => Err(crate::error::core_not_in_build("clo")),
        #[cfg(feature = "cicd")]
        CoreCommand::InstallCicd { packages, dry_run } => {
            commands::core::install::cicd::install(packages, dry_run).await
        }
        #[cfg(not(feature = "cicd"))]
        CoreCommand::InstallCicd { .. } => Err(crate::error::core_not_in_build("cicd")),
        #[cfg(feature = "iot")]
        CoreCommand::InstallIot { packages } => {
            commands::core::install::iot::install(packages).await
        }
        #[cfg(not(feature = "iot"))]
        CoreCommand::InstallIot { .. } => Err(crate::error::core_not_in_build("iot")),
        #[cfg(feature = "app")]
        CoreCommand::InstallApp { packages, dry_run } => {
            commands::core::install::app::install(packages, dry_run).await
        }
        #[cfg(not(feature = "app"))]
        CoreCommand::InstallApp { .. } => Err(crate::error::core_not_in_build("app")),
        #[cfg(feature = "lib")]
        CoreCommand::InstallLib { packages } => {
            commands::core::install::library::install(packages).await
        }
        #[cfg(not(feature = "lib"))]
        CoreCommand::InstallLib { .. } => Err(crate::error::core_not_in_build("lib")),
        #[cfg(feature = "hardware")]
        CoreCommand::InstallHardware { packages } => {
            commands::core::install::hardware::install(packages).await
        }
        #[cfg(not(feature = "hardware"))]
        CoreCommand::InstallHardware { .. } => Err(crate::error::core_not_in_build("hardware")),
        _ => unreachable!("non-install command routed to install dispatcher"),
    }
}
