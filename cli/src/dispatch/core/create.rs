use anyhow::Result;

use crate::commands;

use super::super::types::CoreCommand;

pub fn matches(command: &CoreCommand) -> bool {
    matches!(
        command,
        CoreCommand::CreateWeb { .. }
            | CoreCommand::CreateGame { .. }
            | CoreCommand::CreateAi { .. }
            | CoreCommand::CreateClo { .. }
            | CoreCommand::CreateCicd { .. }
            | CoreCommand::CreateIot { .. }
            | CoreCommand::CreateApp { .. }
            | CoreCommand::CreateLib { .. }
            | CoreCommand::CreateHardware { .. }
    )
}

pub async fn dispatch(command: CoreCommand) -> Result<()> {
    match command {
        CoreCommand::CreateWeb {
            framework,
            project_name,
            flags,
        } => {
            commands::core::create::web::run_create_with_options(
                &framework,
                &project_name,
                Some(flags),
            )
            .await
        }
        #[cfg(feature = "game")]
        CoreCommand::CreateGame {
            framework,
            project_name,
        } => commands::core::create::game::run(&framework, &project_name).await,
        #[cfg(not(feature = "game"))]
        CoreCommand::CreateGame { .. } => Err(crate::error::core_not_in_build("game")),
        #[cfg(feature = "ai")]
        CoreCommand::CreateAi {
            framework,
            project_name,
        } => commands::core::create::ai::run(&framework, &project_name).await,
        #[cfg(not(feature = "ai"))]
        CoreCommand::CreateAi { .. } => Err(crate::error::core_not_in_build("ai")),
        #[cfg(feature = "clo")]
        CoreCommand::CreateClo {
            framework,
            project_name,
        } => commands::core::create::clo::run(&framework, &project_name).await,
        #[cfg(not(feature = "clo"))]
        CoreCommand::CreateClo { .. } => Err(crate::error::core_not_in_build("clo")),
        #[cfg(feature = "cicd")]
        CoreCommand::CreateCicd {
            framework,
            project_name,
        } => commands::core::create::cicd::run(&framework, &project_name).await,
        #[cfg(not(feature = "cicd"))]
        CoreCommand::CreateCicd { .. } => Err(crate::error::core_not_in_build("cicd")),
        #[cfg(feature = "iot")]
        CoreCommand::CreateIot {
            framework,
            project_name,
        } => commands::core::create::iot::run(&framework, &project_name).await,
        #[cfg(not(feature = "iot"))]
        CoreCommand::CreateIot { .. } => Err(crate::error::core_not_in_build("iot")),
        #[cfg(feature = "app")]
        CoreCommand::CreateApp {
            framework,
            project_name,
        } => commands::core::create::app::run(&framework, &project_name).await,
        #[cfg(not(feature = "app"))]
        CoreCommand::CreateApp { .. } => Err(crate::error::core_not_in_build("app")),
        #[cfg(feature = "lib")]
        CoreCommand::CreateLib {
            framework,
            project_name,
        } => commands::core::create::library::run(&framework, &project_name).await,
        #[cfg(not(feature = "lib"))]
        CoreCommand::CreateLib { .. } => Err(crate::error::core_not_in_build("lib")),
        #[cfg(feature = "hardware")]
        CoreCommand::CreateHardware {
            framework,
            project_name,
        } => commands::core::create::hardware::run(&framework, &project_name).await,
        #[cfg(not(feature = "hardware"))]
        CoreCommand::CreateHardware { .. } => Err(crate::error::core_not_in_build("hardware")),
        _ => unreachable!("non-create command routed to create dispatcher"),
    }
}
