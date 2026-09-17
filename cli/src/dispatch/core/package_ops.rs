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
            compat_runtime,
            version,
        } => {
            commands::core::add::web::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                install,
                global,
                compat_runtime,
            )
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
            compat_runtime,
            version,
        } => {
            commands::core::add::game::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(not(feature = "game"))]
        CoreCommand::AddGame { .. } => Err(crate::error::core_not_in_build("game")),
        #[cfg(feature = "ai")]
        CoreCommand::AddAi {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,
            version,
        } => {
            commands::core::add::ai::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(not(feature = "ai"))]
        CoreCommand::AddAi { .. } => Err(crate::error::core_not_in_build("ai")),
        #[cfg(feature = "clo")]
        CoreCommand::AddClo {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,
            version,
        } => {
            commands::core::add::clo::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(not(feature = "clo"))]
        CoreCommand::AddClo { .. } => Err(crate::error::core_not_in_build("clo")),
        #[cfg(feature = "cicd")]
        CoreCommand::AddCicd {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,
            version,
        } => {
            commands::core::add::cicd::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(not(feature = "cicd"))]
        CoreCommand::AddCicd { .. } => Err(crate::error::core_not_in_build("cicd")),
        #[cfg(feature = "iot")]
        CoreCommand::AddIot {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,
            version,
        } => {
            commands::core::add::iot::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(not(feature = "iot"))]
        CoreCommand::AddIot { .. } => Err(crate::error::core_not_in_build("iot")),
        #[cfg(feature = "app")]
        CoreCommand::AddApp {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,
            version,
        } => {
            commands::core::add::app::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(not(feature = "app"))]
        CoreCommand::AddApp { .. } => Err(crate::error::core_not_in_build("app")),
        #[cfg(feature = "lib")]
        CoreCommand::AddLib {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
            compat_runtime,
            version,
        } => {
            commands::core::add::library::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(not(feature = "lib"))]
        CoreCommand::AddLib { .. } => Err(crate::error::core_not_in_build("lib")),
        #[cfg(feature = "hardware")]
        CoreCommand::AddHardware {
            packages,
            compat_runtime,
            version,
        } => commands::core::add::hardware::add(packages, compat_runtime, version).await,
        #[cfg(not(feature = "hardware"))]
        CoreCommand::AddHardware { .. } => Err(crate::error::core_not_in_build("hardware")),
        CoreCommand::RemoveWeb {
            packages,
            install,
            compat_runtime,
        } => commands::core::remove::web::remove(packages, install, compat_runtime).await,
        #[cfg(feature = "game")]
        CoreCommand::RemoveGame {
            packages,
            compat_runtime,
        } => commands::core::remove::game::remove(packages, compat_runtime).await,
        #[cfg(not(feature = "game"))]
        CoreCommand::RemoveGame { .. } => Err(crate::error::core_not_in_build("game")),
        #[cfg(feature = "ai")]
        CoreCommand::RemoveAi {
            packages,
            compat_runtime,
        } => commands::core::remove::ai::remove(packages, compat_runtime).await,
        #[cfg(not(feature = "ai"))]
        CoreCommand::RemoveAi { .. } => Err(crate::error::core_not_in_build("ai")),
        #[cfg(feature = "clo")]
        CoreCommand::RemoveClo {
            packages,
            compat_runtime,
        } => commands::core::remove::clo::remove(packages, compat_runtime).await,
        #[cfg(not(feature = "clo"))]
        CoreCommand::RemoveClo { .. } => Err(crate::error::core_not_in_build("clo")),
        #[cfg(feature = "cicd")]
        CoreCommand::RemoveCicd {
            packages,
            compat_runtime,
        } => commands::core::remove::cicd::remove(packages, compat_runtime).await,
        #[cfg(not(feature = "cicd"))]
        CoreCommand::RemoveCicd { .. } => Err(crate::error::core_not_in_build("cicd")),
        #[cfg(feature = "iot")]
        CoreCommand::RemoveIot {
            packages,
            compat_runtime,
        } => commands::core::remove::iot::remove(packages, compat_runtime).await,
        #[cfg(not(feature = "iot"))]
        CoreCommand::RemoveIot { .. } => Err(crate::error::core_not_in_build("iot")),
        #[cfg(feature = "app")]
        CoreCommand::RemoveApp {
            packages,
            compat_runtime,
        } => commands::core::remove::app::remove(packages, compat_runtime).await,
        #[cfg(not(feature = "app"))]
        CoreCommand::RemoveApp { .. } => Err(crate::error::core_not_in_build("app")),
        #[cfg(feature = "lib")]
        CoreCommand::RemoveLib {
            packages,
            compat_runtime,
        } => commands::core::remove::library::remove(packages, compat_runtime).await,
        #[cfg(not(feature = "lib"))]
        CoreCommand::RemoveLib { .. } => Err(crate::error::core_not_in_build("lib")),
        CoreCommand::ListWeb => commands::core::list::web::list().await,
        #[cfg(feature = "game")]
        CoreCommand::ListGame => commands::core::list::game::list().await,
        #[cfg(not(feature = "game"))]
        CoreCommand::ListGame => Err(crate::error::core_not_in_build("game")),
        #[cfg(feature = "ai")]
        CoreCommand::ListAi => commands::core::list::ai::list().await,
        #[cfg(not(feature = "ai"))]
        CoreCommand::ListAi => Err(crate::error::core_not_in_build("ai")),
        #[cfg(feature = "clo")]
        CoreCommand::ListClo => commands::core::list::clo::list().await,
        #[cfg(not(feature = "clo"))]
        CoreCommand::ListClo => Err(crate::error::core_not_in_build("clo")),
        #[cfg(feature = "cicd")]
        CoreCommand::ListCicd => commands::core::list::cicd::list().await,
        #[cfg(not(feature = "cicd"))]
        CoreCommand::ListCicd => Err(crate::error::core_not_in_build("cicd")),
        #[cfg(feature = "iot")]
        CoreCommand::ListIot => commands::core::list::iot::list().await,
        #[cfg(not(feature = "iot"))]
        CoreCommand::ListIot => Err(crate::error::core_not_in_build("iot")),
        #[cfg(feature = "app")]
        CoreCommand::ListApp => commands::core::list::app::list().await,
        #[cfg(not(feature = "app"))]
        CoreCommand::ListApp => Err(crate::error::core_not_in_build("app")),
        #[cfg(feature = "lib")]
        CoreCommand::ListLib => commands::core::list::library::list().await,
        #[cfg(not(feature = "lib"))]
        CoreCommand::ListLib => Err(crate::error::core_not_in_build("lib")),
        #[cfg(feature = "hardware")]
        CoreCommand::ListHardware => commands::core::list::hardware::list().await,
        #[cfg(not(feature = "hardware"))]
        CoreCommand::ListHardware => Err(crate::error::core_not_in_build("hardware")),
        CoreCommand::UpdateWeb {
            packages,
            install,
            compat_runtime,
        } => commands::core::update::web::update(packages, install, compat_runtime).await,
        #[cfg(feature = "game")]
        CoreCommand::UpdateGame {
            packages,
            install,
            compat_runtime,
        } => commands::core::update::game::update(packages, install, compat_runtime).await,
        #[cfg(not(feature = "game"))]
        CoreCommand::UpdateGame { .. } => Err(crate::error::core_not_in_build("game")),
        #[cfg(feature = "ai")]
        CoreCommand::UpdateAi {
            packages,
            install,
            compat_runtime,
        } => commands::core::update::ai::update(packages, install, compat_runtime).await,
        #[cfg(not(feature = "ai"))]
        CoreCommand::UpdateAi { .. } => Err(crate::error::core_not_in_build("ai")),
        #[cfg(feature = "clo")]
        CoreCommand::UpdateClo {
            packages,
            install,
            compat_runtime,
        } => commands::core::update::clo::update(packages, install, compat_runtime).await,
        #[cfg(not(feature = "clo"))]
        CoreCommand::UpdateClo { .. } => Err(crate::error::core_not_in_build("clo")),
        #[cfg(feature = "cicd")]
        CoreCommand::UpdateCicd {
            packages,
            install,
            compat_runtime,
        } => commands::core::update::cicd::update(packages, install, compat_runtime).await,
        #[cfg(not(feature = "cicd"))]
        CoreCommand::UpdateCicd { .. } => Err(crate::error::core_not_in_build("cicd")),
        #[cfg(feature = "iot")]
        CoreCommand::UpdateIot {
            packages,
            install,
            compat_runtime,
        } => commands::core::update::iot::update(packages, install, compat_runtime).await,
        #[cfg(not(feature = "iot"))]
        CoreCommand::UpdateIot { .. } => Err(crate::error::core_not_in_build("iot")),
        #[cfg(feature = "app")]
        CoreCommand::UpdateApp {
            packages,
            install,
            compat_runtime,
        } => commands::core::update::app::update(packages, install, compat_runtime).await,
        #[cfg(not(feature = "app"))]
        CoreCommand::UpdateApp { .. } => Err(crate::error::core_not_in_build("app")),
        #[cfg(feature = "lib")]
        CoreCommand::UpdateLib {
            packages,
            install,
            compat_runtime,
        } => commands::core::update::library::update(packages, install, compat_runtime).await,
        #[cfg(not(feature = "lib"))]
        CoreCommand::UpdateLib { .. } => Err(crate::error::core_not_in_build("lib")),
        _ => unreachable!("non-package command routed to package dispatcher"),
    }
}
