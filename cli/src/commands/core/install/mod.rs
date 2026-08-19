//! `mg install` — router: core detect → file con (v5: LỆNH = folder, CORE = file).

use anyhow::Result;

pub mod ai;
pub mod app;
pub mod web;
pub mod cicd;
pub mod clo;
pub mod game;
pub mod iot;
pub mod library;
pub mod hardware;

pub async fn run(core: &str, packages: Vec<String>) -> Result<()> {
    match core {
        "game" => game::install(packages).await,
        "ai" => ai::install(packages, false).await,
        "web" => web::install(packages, false, false, false, false, false)
            .await,
                "clo" => clo::install(packages, false).await,
        "cicd" => cicd::install(packages, false).await,
        "iot" => iot::install(packages).await,
        "app" => app::install(packages, false).await,
        "lib" => library::install(packages).await,
        "hardware" => hardware::install(packages).await,
        other => Err(crate::error::unknown_core(other)),
    }
}