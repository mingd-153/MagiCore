use anyhow::Result;

use crate::commands;

use super::types::CommonCommand;

pub async fn dispatch_common(command: CommonCommand, core: Option<&str>) -> Result<()> {
    match command {
        CommonCommand::Init { template } => commands::init::run(template).await,
        CommonCommand::Dev { host, port } => commands::dev::run(core, host, port).await,
        CommonCommand::Info { package } => commands::info::run(package).await,
        CommonCommand::Search { query } => commands::search::run(query).await,
        CommonCommand::Outdated => commands::outdated::run(core).await,
        CommonCommand::Audit => commands::audit::run(core).await,
    }
}
