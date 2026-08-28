use anyhow::Result;
use mgc_types::Ecosystem;

use crate::context::ProjectContext;

pub mod ai;
pub mod app;
pub mod cicd;
pub mod clo;
pub mod game;
pub mod hardware;
pub mod iot;
pub mod lib;
pub mod web;

pub async fn run(core: Option<&str>, fix: bool) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    if ctx.adapter().name() == "web" {
        return web::audit(ctx.adapter(), ctx.root(), fix).await;
    }
    if fix {
        return Err(crate::error::audit_fix_web_only());
    }

    // Convert adapter name to Ecosystem
    let ecosystem = match ctx.adapter().name() {
        "web" => Ecosystem::Web,
        "game" => Ecosystem::Game,
        "ai" => Ecosystem::Ai,
        "clo" => Ecosystem::Cloud,
        "cicd" => Ecosystem::Cicd,
        "iot" => Ecosystem::Iot,
        "app" => Ecosystem::App,
        "lib" => Ecosystem::Lib,
        "hardware" => Ecosystem::Hardware,
        other => return Err(crate::error::unknown_core(other)),
    };

    execute_audit(&ecosystem).await
}

pub async fn execute_audit(ecosystem: &Ecosystem) -> Result<()> {
    match ecosystem {
        Ecosystem::Hardware => hardware::audit().await,
        Ecosystem::Web => Err(crate::error::web_audit_needs_context()),
        Ecosystem::Game => game::audit().await,
        Ecosystem::Ai => ai::audit().await,
        Ecosystem::Cloud => clo::audit().await,
        Ecosystem::Cicd => cicd::audit().await,
        Ecosystem::Iot => iot::audit().await,
        Ecosystem::App => app::audit().await,
        Ecosystem::Lib => lib::audit().await,
    }
}
