use anyhow::Result;

use crate::commands;

use super::types::CommonCommand;

pub async fn dispatch_common(command: CommonCommand, core: Option<&str>) -> Result<()> {
    match command {
        CommonCommand::Init { template } => commands::init::run(template).await,
        CommonCommand::Dev { host, port } => commands::dev::run(core, host, port).await,
        CommonCommand::Info { package, json } => commands::info::run(package, json).await,
        CommonCommand::Search { query, json, exact, page } => {
            commands::search::run(query, json, exact, page).await
        }
        CommonCommand::Outdated { json } => commands::outdated::run(core, json).await,
        CommonCommand::Audit => commands::audit::run(core).await,
    }
}
