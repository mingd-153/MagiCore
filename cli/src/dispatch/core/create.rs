use anyhow::Result;

use crate::commands;

use super::super::types::CoreCommand;

pub fn matches(command: &CoreCommand) -> bool {
    match command {
        #[cfg(feature = "web")]
        CoreCommand::CreateWeb { .. } => true,
        #[cfg(feature = "game")]
        CoreCommand::CreateGame { .. } => true,
        #[cfg(feature = "ai")]
        CoreCommand::CreateAi { .. } => true,
        #[cfg(feature = "clo")]
        CoreCommand::CreateClo { .. } => true,
        #[cfg(feature = "cicd")]
        CoreCommand::CreateCicd { .. } => true,
        #[cfg(feature = "iot")]
        CoreCommand::CreateIot { .. } => true,
        #[cfg(feature = "app")]
        CoreCommand::CreateApp { .. } => true,
        #[cfg(feature = "lib")]
        CoreCommand::CreateLib { .. } => true,
        _ => false,
    }
}

pub async fn dispatch(command: CoreCommand) -> Result<()> {
    match command {
        #[cfg(feature = "web")]
        CoreCommand::CreateWeb {
            framework,
            project_name,
            ts,
            tailwindcss,
            monorepo,
            backend,
            features,
        } => {
            commands::core::web::run_create_with_options(
                &framework,
                &project_name,
                Some(commands::core::web::WebCreateOptions {
                    typescript: ts,
                    tailwindcss,
                    monorepo,
                    backend,
                    features,
                }),
            )
            .await
        }
        #[cfg(feature = "game")]
        CoreCommand::CreateGame {
            framework,
            project_name,
        } => commands::core::game::create::run(&framework, &project_name).await,
        #[cfg(feature = "ai")]
        CoreCommand::CreateAi {
            framework,
            project_name,
        } => commands::core::ai::create::run(&framework, &project_name).await,
        #[cfg(feature = "clo")]
        CoreCommand::CreateClo {
            framework,
            project_name,
        } => commands::core::clo::create::run(&framework, &project_name).await,
        #[cfg(feature = "cicd")]
        CoreCommand::CreateCicd {
            framework,
            project_name,
        } => commands::core::cicd::create::run(&framework, &project_name).await,
        #[cfg(feature = "iot")]
        CoreCommand::CreateIot {
            framework,
            project_name,
        } => commands::core::iot::create::run(&framework, &project_name).await,
        #[cfg(feature = "app")]
        CoreCommand::CreateApp {
            framework,
            project_name,
        } => commands::core::app::create::run(&framework, &project_name).await,
        #[cfg(feature = "lib")]
        CoreCommand::CreateLib { project_name } => {
            commands::core::library::create::run(&project_name).await
        }
        _ => unreachable!("non-create command routed to create dispatcher"),
    }
}
