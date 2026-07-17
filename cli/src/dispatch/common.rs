use anyhow::Result;

use crate::commands;
use crate::context::ProjectContext;

use super::types::CommonCommand;

pub async fn dispatch_common(command: CommonCommand, core: Option<&str>) -> Result<()> {
    match command {
        CommonCommand::Init { template } => commands::init::run(template).await,
        CommonCommand::Dev { host, port, clear } => {
            commands::dev::run(core, host, port, clear).await
        }
        CommonCommand::Build => commands::build::run(core).await,
        CommonCommand::Start => commands::start::run(core).await,
        CommonCommand::Exec { command, args } => commands::exec::run(core, command, args),
        CommonCommand::Info { package, json } => commands::info::run(package, json).await,
        CommonCommand::Search {
            query,
            json,
            exact,
            page,
        } => commands::search::run(query, json, exact, page).await,
        CommonCommand::Outdated { json } => commands::outdated::run(core, json).await,
        CommonCommand::Audit => commands::audit::run(core).await,
        CommonCommand::SelfUpdate => commands::self_update::run().await,
        CommonCommand::Run { script, args } => commands::run::run(script, args, core).await,
        CommonCommand::Dlx { package, args } => commands::dlx::run(package, args).await,
        CommonCommand::Link { package } => {
            let ctx = ProjectContext::load_with_core(core)?;
            commands::core::shared::link(ctx.adapter(), ctx.root(), package.as_deref()).await
        }
        CommonCommand::Unlink { package } => {
            let ctx = ProjectContext::load_with_core(core)?;
            commands::core::shared::unlink(ctx.adapter(), ctx.root(), package.as_deref()).await
        }
        CommonCommand::Why { package } => {
            let ctx = ProjectContext::load_with_core(core)?;
            commands::core::shared::why(ctx.adapter(), ctx.root(), &package).await
        }
    }
}
