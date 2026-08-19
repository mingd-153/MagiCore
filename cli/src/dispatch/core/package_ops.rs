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
            commands::core::add::web::add(
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
            commands::core::add::game::add(packages, None, dev, exact, optional, peer, no_save, global)
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
            commands::core::add::ai::add(packages, None, dev, exact, optional, peer, no_save, global)
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
            commands::core::add::clo::add(packages, None, dev, exact, optional, peer, no_save, global)
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
            commands::core::add::cicd::add(packages, None, dev, exact, optional, peer, no_save, global)
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
            commands::core::add::iot::add(packages, None, dev, exact, optional, peer, no_save, global)
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
            commands::core::add::app::add(packages, None, dev, exact, optional, peer, no_save, global)
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
            commands::core::add::library::add(
                packages, None, dev, exact, optional, peer, no_save, global,
            )
            .await
        }
        CoreCommand::AddHardware { packages } => commands::core::add::hardware::add(packages).await,
        CoreCommand::RemoveWeb { packages, install } => {
            commands::core::remove::web::remove(packages, install).await
        }
        CoreCommand::RemoveGame { packages } => commands::core::remove::game::remove(packages).await,
        CoreCommand::RemoveAi { packages } => commands::core::remove::ai::remove(packages).await,
        CoreCommand::RemoveClo { packages } => commands::core::remove::clo::remove(packages).await,
        CoreCommand::RemoveCicd { packages } => commands::core::remove::cicd::remove(packages).await,
        CoreCommand::RemoveIot { packages } => commands::core::remove::iot::remove(packages).await,
        CoreCommand::RemoveApp { packages } => commands::core::remove::app::remove(packages).await,
        CoreCommand::RemoveLib { packages } => commands::core::remove::library::remove(packages).await,
        CoreCommand::ListWeb => commands::core::list::web::list().await,
        CoreCommand::ListGame => commands::core::list::game::list().await,
        CoreCommand::ListAi => commands::core::list::ai::list().await,
        CoreCommand::ListClo => commands::core::list::clo::list().await,
        CoreCommand::ListCicd => commands::core::list::cicd::list().await,
        CoreCommand::ListIot => commands::core::list::iot::list().await,
        CoreCommand::ListApp => commands::core::list::app::list().await,
        CoreCommand::ListLib => commands::core::list::library::list().await,
        CoreCommand::ListHardware => commands::core::list::hardware::list().await,
        CoreCommand::UpdateWeb { packages, install } => {
            commands::core::update::web::update(packages, install).await
        }
        CoreCommand::UpdateGame { packages, install } => {
            commands::core::update::game::update(packages, install).await
        }
        CoreCommand::UpdateAi { packages, install } => {
            commands::core::update::ai::update(packages, install).await
        }
        CoreCommand::UpdateClo { packages, install } => {
            commands::core::update::clo::update(packages, install).await
        }
        CoreCommand::UpdateCicd { packages, install } => {
            commands::core::update::cicd::update(packages, install).await
        }
        CoreCommand::UpdateIot { packages, install } => {
            commands::core::update::iot::update(packages, install).await
        }
        CoreCommand::UpdateApp { packages, install } => {
            commands::core::update::app::update(packages, install).await
        }
        CoreCommand::UpdateLib { packages, install } => {
            commands::core::update::library::update(packages, install).await
        }
        _ => unreachable!("non-package command routed to package dispatcher"),
    }
}
