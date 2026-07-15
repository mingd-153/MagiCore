use anyhow::Result;

use crate::commands;

use super::super::types::CoreCommand;

pub fn matches(command: &CoreCommand) -> bool {
    match command {
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
        CoreCommand::Install { .. } => true,
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
        CoreCommand::InstallWeb { .. } => true,
        #[cfg(feature = "game")]
        CoreCommand::InstallGame { .. } => true,
        #[cfg(feature = "ai")]
        CoreCommand::InstallAi { .. } => true,
        #[cfg(feature = "clo")]
        CoreCommand::InstallClo { .. } => true,
        #[cfg(feature = "cicd")]
        CoreCommand::InstallCicd { .. } => true,
        #[cfg(feature = "iot")]
        CoreCommand::InstallIot { .. } => true,
        #[cfg(feature = "app")]
        CoreCommand::InstallApp { .. } => true,
        #[cfg(feature = "lib")]
        CoreCommand::InstallLib { .. } => true,
        _ => false,
    }
}

pub async fn dispatch(command: CoreCommand) -> Result<()> {
    match command {
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
        CoreCommand::Install {
            packages, frozen, ..
        } => commands::core::web::install(packages, frozen).await,
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
        CoreCommand::InstallWeb { packages, frozen } => {
            commands::core::web::install(packages, frozen).await
        }
        #[cfg(feature = "game")]
        CoreCommand::InstallGame { packages } => commands::core::game::install(packages).await,
        #[cfg(feature = "ai")]
        CoreCommand::InstallAi { packages } => commands::core::ai::install(packages).await,
        #[cfg(feature = "clo")]
        CoreCommand::InstallClo { packages } => commands::core::clo::install(packages).await,
        #[cfg(feature = "cicd")]
        CoreCommand::InstallCicd { packages } => commands::core::cicd::install(packages).await,
        #[cfg(feature = "iot")]
        CoreCommand::InstallIot { packages } => commands::core::iot::install(packages).await,
        #[cfg(feature = "app")]
        CoreCommand::InstallApp { packages } => commands::core::app::install(packages).await,
        #[cfg(feature = "lib")]
        CoreCommand::InstallLib { packages } => commands::core::library::install(packages).await,
        _ => unreachable!("non-install command routed to install dispatcher"),
    }
}
