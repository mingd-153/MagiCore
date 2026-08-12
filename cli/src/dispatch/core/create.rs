use anyhow::Result;

use crate::commands;

use super::super::types::CoreCommand;

pub fn matches(command: &CoreCommand) -> bool {
    match command {
        CoreCommand::CreateWeb { .. } => true,
        CoreCommand::CreateGame { .. } => true,
        CoreCommand::CreateAi { .. } => true,
        CoreCommand::CreateClo { .. } => true,
        CoreCommand::CreateCicd { .. } => true,
        CoreCommand::CreateIot { .. } => true,
        CoreCommand::CreateApp { .. } => true,
        CoreCommand::CreateLib { .. } => true,
        CoreCommand::CreateHardware { .. } => true,
        _ => false,
    }
}

pub async fn dispatch(command: CoreCommand) -> Result<()> {
    match command {
        CoreCommand::CreateWeb {
            framework,
            project_name,
            flags,
        } => {
            commands::core::web::run_create_with_options(&framework, &project_name, Some(flags))
                .await
        }
        CoreCommand::CreateGame {
            framework,
            project_name,
        } => commands::core::game::create::run(&framework, &project_name).await,
        CoreCommand::CreateAi {
            framework,
            project_name,
        } => commands::core::ai::create::run(&framework, &project_name).await,
        CoreCommand::CreateClo {
            framework,
            project_name,
        } => commands::core::clo::create::run(&framework, &project_name).await,
        CoreCommand::CreateCicd {
            framework,
            project_name,
        } => commands::core::cicd::create::run(&framework, &project_name).await,
        CoreCommand::CreateIot {
            framework,
            project_name,
        } => commands::core::iot::create::run(&framework, &project_name).await,
        CoreCommand::CreateApp {
            framework,
            project_name,
        } => commands::core::app::create::run(&framework, &project_name).await,
        CoreCommand::CreateLib { project_name } => {
            commands::core::library::create::run(&project_name).await
        }
        CoreCommand::CreateHardware {
            framework,
            project_name,
        } => commands::core::hardware::create::run(&framework, &project_name).await,
        _ => unreachable!("non-create command routed to create dispatcher"),
    }
}
