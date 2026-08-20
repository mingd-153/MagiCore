//! `mg list` — router: core detect → file con (v5: LỆNH = folder, CORE = file).

use anyhow::Result;

pub mod ai;
pub mod app;
pub mod cicd;
pub mod clo;
pub mod game;
pub mod hardware;
pub mod iot;
pub mod library;
pub mod web;

pub async fn run(core: &str) -> Result<()> {
    match core {
        "game" => game::list().await,
        "ai" => ai::list().await,
        "web" => web::list().await,
        "clo" => clo::list().await,
        "cicd" => cicd::list().await,
        "iot" => iot::list().await,
        "app" => app::list().await,
        "lib" => library::list().await,
        "hardware" => hardware::list().await,
        other => Err(crate::error::unknown_core(other)),
    }
}
