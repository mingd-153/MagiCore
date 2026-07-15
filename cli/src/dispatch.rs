use anyhow::Result;

use crate::Cli;

mod common;
mod core;
mod types;

use types::DispatchCommand;

pub async fn run(cli: Cli) -> Result<()> {
    let core = cli.core.as_deref();

    match cli.command {
        Some(command) => dispatch_command(command.into(), core).await,
        None => {
            let cores = crate::factory::available_cores();
            mg_ui::help::print_custom_help(&cores);
            Ok(())
        }
    }
}

async fn dispatch_command(command: DispatchCommand, core: Option<&str>) -> Result<()> {
    match command {
        DispatchCommand::Common(command) => common::dispatch_common(command, core).await,
        DispatchCommand::Core(command) => core::dispatch_core(command).await,
    }
}
