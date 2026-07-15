use anyhow::Result;

use super::types::CoreCommand;

mod create;
mod install;
mod package_ops;

pub async fn dispatch_core(command: CoreCommand) -> Result<()> {
    match command {
        create_command if create::matches(&create_command) => {
            create::dispatch(create_command).await
        }
        install_command if install::matches(&install_command) => {
            install::dispatch(install_command).await
        }
        package_command => package_ops::dispatch(package_command).await,
    }
}
