use anyhow::Result;

use crate::commands;

use super::super::types::CoreCommand;

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
        CoreCommand::Add {
            packages,
            version,
            dev,
            global,
            exact,
            optional,
            peer,
            no_save,
        } => {
            commands::core::web::add(
                packages, version, dev, exact, optional, peer, no_save, global,
            )
            .await
        }
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
        CoreCommand::Remove { package } => commands::core::web::remove(package).await,
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
        CoreCommand::Update { packages, install } => {
            commands::core::web::update(packages, install).await
        }
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
        CoreCommand::List => commands::core::web::list().await,
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
        CoreCommand::AddWeb {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        } => {
            commands::core::web::add(packages, None, dev, exact, optional, peer, no_save, global)
                .await
        }
        #[cfg(feature = "game")]
        CoreCommand::AddGame {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        } => {
            commands::core::game::add(packages, None, dev, exact, optional, peer, no_save, global)
                .await
        }
        #[cfg(feature = "ai")]
        CoreCommand::AddAi {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        } => {
            commands::core::ai::add(packages, None, dev, exact, optional, peer, no_save, global)
                .await
        }
        #[cfg(feature = "clo")]
        CoreCommand::AddClo {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        } => {
            commands::core::clo::add(packages, None, dev, exact, optional, peer, no_save, global)
                .await
        }
        #[cfg(feature = "cicd")]
        CoreCommand::AddCicd {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        } => {
            commands::core::cicd::add(packages, None, dev, exact, optional, peer, no_save, global)
                .await
        }
        #[cfg(feature = "iot")]
        CoreCommand::AddIot {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        } => {
            commands::core::iot::add(packages, None, dev, exact, optional, peer, no_save, global)
                .await
        }
        #[cfg(feature = "app")]
        CoreCommand::AddApp {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        } => {
            commands::core::app::add(packages, None, dev, exact, optional, peer, no_save, global)
                .await
        }
        #[cfg(feature = "lib")]
        CoreCommand::AddLib {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        } => {
            commands::core::library::add(
                packages, None, dev, exact, optional, peer, no_save, global,
            )
            .await
        }
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
        CoreCommand::RemoveWeb { package } => commands::core::web::remove(package).await,
        #[cfg(feature = "game")]
        CoreCommand::RemoveGame { package } => commands::core::game::remove(package).await,
        #[cfg(feature = "ai")]
        CoreCommand::RemoveAi { package } => commands::core::ai::remove(package).await,
        #[cfg(feature = "clo")]
        CoreCommand::RemoveClo { package } => commands::core::clo::remove(package).await,
        #[cfg(feature = "cicd")]
        CoreCommand::RemoveCicd { package } => commands::core::cicd::remove(package).await,
        #[cfg(feature = "iot")]
        CoreCommand::RemoveIot { package } => commands::core::iot::remove(package).await,
        #[cfg(feature = "app")]
        CoreCommand::RemoveApp { package } => commands::core::app::remove(package).await,
        #[cfg(feature = "lib")]
        CoreCommand::RemoveLib { package } => commands::core::library::remove(package).await,
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
        CoreCommand::ListWeb => commands::core::web::list().await,
        #[cfg(feature = "game")]
        CoreCommand::ListGame => commands::core::game::list().await,
        #[cfg(feature = "ai")]
        CoreCommand::ListAi => commands::core::ai::list().await,
        #[cfg(feature = "clo")]
        CoreCommand::ListClo => commands::core::clo::list().await,
        #[cfg(feature = "cicd")]
        CoreCommand::ListCicd => commands::core::cicd::list().await,
        #[cfg(feature = "iot")]
        CoreCommand::ListIot => commands::core::iot::list().await,
        #[cfg(feature = "app")]
        CoreCommand::ListApp => commands::core::app::list().await,
        #[cfg(feature = "lib")]
        CoreCommand::ListLib => commands::core::library::list().await,
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
        CoreCommand::UpdateWeb { packages, install } => {
            commands::core::web::update(packages, install).await
        }
        #[cfg(feature = "game")]
        CoreCommand::UpdateGame { packages, install } => {
            commands::core::game::update(packages, install).await
        }
        #[cfg(feature = "ai")]
        CoreCommand::UpdateAi { packages, install } => {
            commands::core::ai::update(packages, install).await
        }
        #[cfg(feature = "clo")]
        CoreCommand::UpdateClo { packages, install } => {
            commands::core::clo::update(packages, install).await
        }
        #[cfg(feature = "cicd")]
        CoreCommand::UpdateCicd { packages, install } => {
            commands::core::cicd::update(packages, install).await
        }
        #[cfg(feature = "iot")]
        CoreCommand::UpdateIot { packages, install } => {
            commands::core::iot::update(packages, install).await
        }
        #[cfg(feature = "app")]
        CoreCommand::UpdateApp { packages, install } => {
            commands::core::app::update(packages, install).await
        }
        #[cfg(feature = "lib")]
        CoreCommand::UpdateLib { packages, install } => {
            commands::core::library::update(packages, install).await
        }
        _ => unreachable!("non-package command routed to package dispatcher"),
    }
}
