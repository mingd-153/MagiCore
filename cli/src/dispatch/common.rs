use anyhow::Result;

use crate::commands;
use crate::context::ProjectContext;

use super::types::CommonCommand;

pub async fn dispatch_common(
    command: CommonCommand,
    core: Option<&str>,
    recursive: bool,
) -> Result<()> {
    match command {
        CommonCommand::Init {
            template,
            signature,
        } => commands::init::run(template, signature).await,
        CommonCommand::Config { cmd, local } => commands::config::run(cmd, local).await,
        CommonCommand::Stage { dir } => {
            commands::publish::stage(dir.map(|d| d.display().to_string())).await
        }
        CommonCommand::Import { dir } => commands::import::run(dir).await,
        CommonCommand::Dev { host, port, clear } => {
            commands::dev::run(core, host, port, clear).await
        }
        CommonCommand::Build { target } => commands::build::run(core, target).await,
        #[cfg(feature = "iot")]
        CommonCommand::Flash { board, skip_build } => {
            commands::core::dev::iot::flash(board.as_deref(), skip_build).await
        }
        #[cfg(not(feature = "iot"))]
        CommonCommand::Flash { .. } => Err(crate::error::core_not_in_build("iot")),
        CommonCommand::Deploy { run } => {
            let _ = run;
            match super::types::detect_ecosystem().ok().flatten().as_deref() {
                #[cfg(feature = "cicd")]
                Some("cicd") => commands::core::dev::cicd::deploy(run).await,
                #[cfg(all(not(feature = "cicd"), feature = "clo"))]
                Some("cicd") => Err(crate::error::core_not_in_build("cicd")),
                #[cfg(feature = "clo")]
                _ => commands::core::dev::clo::deploy(run).await,
                #[cfg(not(feature = "clo"))]
                _ => Err(crate::error::core_not_in_build("clo")),
            }
        }
        #[cfg(feature = "cicd")]
        CommonCommand::CiGenerate => commands::core::dev::cicd::ci_generate(),
        #[cfg(not(feature = "cicd"))]
        CommonCommand::CiGenerate => Err(crate::error::core_not_in_build("cicd")),
        #[cfg(feature = "cicd")]
        CommonCommand::Verify => commands::core::dev::cicd::verify().await,
        #[cfg(not(feature = "cicd"))]
        CommonCommand::Verify => Err(crate::error::core_not_in_build("cicd")),
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
        CommonCommand::Audit { fix } => commands::audit::run(core, fix).await,
        CommonCommand::SelfUpdate => commands::self_update::run().await,
        CommonCommand::Run { script, args } => commands::run::run(script, args, core).await,
        CommonCommand::Dlx { package, args } => commands::dlx::run(package, args).await,
        CommonCommand::Cache {
            action,
            target,
            yes,
            dry_run,
        } => commands::cache::run(action, target, yes, dry_run, core).await,
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
        CommonCommand::Login {
            registry,
            username,
            password,
            local,
        } => {
            commands::login::run(crate::commands::login::LoginArgs {
                registry,
                username,
                password,
                local,
            })
            .await
        }
        CommonCommand::Registry { cmd } => {
            commands::registry::run(crate::commands::registry::RegistryArgs { cmd }).await
        }
        CommonCommand::Model { cmd } => {
            commands::model::run(crate::commands::model::ModelArgs { cmd }).await
        }
        CommonCommand::Publish {
            tag,
            access,
            dry_run,
            json,
            otp,
            force,
            ignore_scripts,
            no_git_checks,
            publish_branch,
            batch,
            report_summary,
            patch,
            minor,
            major,
            registry,
            token,
        } => {
            commands::publish::run(
                crate::commands::publish::PublishArgs {
                    tag,
                    access,
                    dry_run,
                    json,
                    otp,
                    force,
                    ignore_scripts,
                    no_git_checks,
                    publish_branch,
                    batch,
                    report_summary,
                    patch,
                    minor,
                    major,
                    registry,
                    token,
                },
                recursive,
            )
            .await
        }
        CommonCommand::Patch { cmd } => {
            commands::patch::run(crate::commands::patch::PatchArgs { cmd }).await
        }
        CommonCommand::Dedupe {
            dry_run,
            prefer_latest,
            json,
        } => {
            commands::dedupe::run(crate::commands::dedupe::DedupeArgs {
                dry_run,
                prefer_latest,
                json,
            })
            .await
        }
        CommonCommand::Store { cmd } => commands::store::run(cmd).await,
        CommonCommand::Bench { args } => commands::bench::handle(args).await,
        CommonCommand::Trust { cmd } => commands::trust::run(cmd).await,
        CommonCommand::Hooks { cmd } => commands::hooks::handle(cmd),
        CommonCommand::Docs { output } => commands::docs::handle(output),
        CommonCommand::Telemetry { cmd } => commands::telemetry::handle(cmd),
        CommonCommand::Network { cmd } => commands::network::handle(cmd),
        CommonCommand::Doctor { cmd } => commands::doctor::handle(cmd),
        CommonCommand::Sbom { output } => commands::sbom::run(core, output).await,
        CommonCommand::Template { cmd } => commands::template::run(cmd).await,
        CommonCommand::Workspace { cmd } => commands::workspace::run(cmd).await,
        CommonCommand::Mcp => commands::mcp::run().await,
    }
}
