use anyhow::Result;

use crate::commands;

use super::super::types::CoreCommand;

pub async fn dispatch(command: CoreCommand) -> Result<()> {
    match command {
        CoreCommand::AddWeb {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            install,
            global,
        } => {
            commands::core::web::add(
                packages, None, dev, exact, optional, peer, no_save, install, global,
            )
            .await
        }
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
        CoreCommand::RemoveWeb { packages, install } => {
            commands::core::web::remove(packages, install).await
        }
        CoreCommand::RemoveGame { packages } => commands::core::game::remove(packages).await,
        CoreCommand::RemoveAi { packages } => commands::core::ai::remove(packages).await,
        CoreCommand::RemoveClo { packages } => commands::core::clo::remove(packages).await,
        CoreCommand::RemoveCicd { packages } => commands::core::cicd::remove(packages).await,
        CoreCommand::RemoveIot { packages } => commands::core::iot::remove(packages).await,
        CoreCommand::RemoveApp { packages } => commands::core::app::remove(packages).await,
        CoreCommand::RemoveLib { packages } => commands::core::library::remove(packages).await,
        CoreCommand::ListWeb => commands::core::web::list().await,
        CoreCommand::ListGame => commands::core::game::list().await,
        CoreCommand::ListAi => commands::core::ai::list().await,
        CoreCommand::ListClo => commands::core::clo::list().await,
        CoreCommand::ListCicd => commands::core::cicd::list().await,
        CoreCommand::ListIot => commands::core::iot::list().await,
        CoreCommand::ListApp => commands::core::app::list().await,
        CoreCommand::ListLib => commands::core::library::list().await,
        CoreCommand::UpdateWeb { packages, install } => {
            commands::core::web::update(packages, install).await
        }
        CoreCommand::UpdateGame { packages, install } => {
            commands::core::game::update(packages, install).await
        }
        CoreCommand::UpdateAi { packages, install } => {
            commands::core::ai::update(packages, install).await
        }
        CoreCommand::UpdateClo { packages, install } => {
            commands::core::clo::update(packages, install).await
        }
        CoreCommand::UpdateCicd { packages, install } => {
            commands::core::cicd::update(packages, install).await
        }
        CoreCommand::UpdateIot { packages, install } => {
            commands::core::iot::update(packages, install).await
        }
        CoreCommand::UpdateApp { packages, install } => {
            commands::core::app::update(packages, install).await
        }
        CoreCommand::UpdateLib { packages, install } => {
            commands::core::library::update(packages, install).await
        }
        _ => unreachable!("non-package command routed to package dispatcher"),
    }
}
